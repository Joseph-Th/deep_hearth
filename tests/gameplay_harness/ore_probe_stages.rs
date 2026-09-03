//! Canonical powered stage execution for the ore-preparation capability probe.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct PoweredStageResult {
    pub(super) energy: Energy,
    pub(super) duration: TickSpan,
    pub(super) condition_after: Condition,
}

pub(super) struct ComminutionStageRequest<'a> {
    pub(super) stage: &'static str,
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
    pub(super) selections: &'a [MaterialLotSelection],
    pub(super) equipment: EquipmentId,
    pub(super) destination: StockpileId,
    pub(super) activity: &'static str,
    pub(super) failure_context: &'static str,
}

pub(super) fn execute_comminution_stage(
    registries: &Registries,
    episode: &mut OreProbeEpisode,
    request: ComminutionStageRequest<'_>,
) -> Result<PoweredStageResult, OreProbeOutcome> {
    let resolved = match resolve_comminution_process(
        registries,
        &episode.state,
        ComminutionRequest::new(
            request.process,
            request.source,
            request.selections,
            request.equipment,
            episode.ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            store: _,
            available,
            requested,
        })) if episode.case.role() != FocusedProbeRole::MaintainedAnchor => {
            return Err(report_ore_energy_stop(
                registries,
                &episode.state,
                episode.ids,
                episode.case,
                episode.initial_matter,
                OreEnergyStop {
                    stage: request.stage,
                    available,
                    requested,
                },
            ));
        }
        Err(ComminutionResolutionError::BatchMassExceeded { .. })
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                request.stage,
                OreStopReason::EquipmentCapacity,
            ));
        }
        Err(ComminutionResolutionError::ConditionDuration(_))
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                request.stage,
                OreStopReason::ConditionLifetime,
            ));
        }
        Err(error) => panic!("{}: {error}", request.failure_context),
    };
    let duration = resolved.process_resolution().duration();
    let result = PoweredStageResult {
        energy: resolved.required_energy(),
        duration,
        condition_after: resolved.condition_after(),
    };
    let job = validate_start_process(
        registries,
        &episode.state,
        resolved.process_resolution(),
        request.source,
        request.destination,
    )
    .unwrap_or_else(|error| panic!("ore preparation {} start failed: {error}", request.stage))
    .commit(&mut episode.state)
    .unwrap_or_else(|error| panic!("ore preparation {} commit failed: {error}", request.stage));
    finish_uninterrupted_production_job(
        registries,
        &mut episode.state,
        job,
        duration,
        request.activity,
    );
    validate_loaded_state(registries, &episode.state).unwrap_or_else(|error| {
        panic!(
            "ore preparation post-{} audit failed: {error}",
            request.stage
        )
    });
    Ok(result)
}

pub(super) fn assert_anchor_route_boundaries(
    registries: &Registries,
    episode: &OreProbeEpisode,
    crushed_selection: &[MaterialLotSelection],
) {
    if episode.case.role() != FocusedProbeRole::MaintainedAnchor {
        return;
    }
    match resolve_screening_process(
        registries,
        &episode.state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            episode.ids.crushed_storage,
            crushed_selection,
            episode.ids.screen,
            episode.ids.drive,
        ),
    ) {
        Ok(_)
        | Err(ScreeningResolutionError::Batch(ScreeningBatchError::UnresolvedParticleClass {
            ..
        })) => {}
        Err(error) => panic!("direct-screen route failed unexpectedly: {error}"),
    }
    match resolve_comminution_process(
        registries,
        &episode.state,
        ComminutionRequest::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            episode.ids.crushed_storage,
            crushed_selection,
            episode.ids.grinder,
            episode.ids.drive,
        ),
    ) {
        Ok(_)
        | Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange { .. },
        )) => {}
        Err(error) => panic!("direct fine-grind route failed unexpectedly: {error}"),
    }
}

#[derive(Clone, Copy)]
pub(super) struct RegrindStageResult {
    pub(super) powered: PoweredStageResult,
    pub(super) fine_output_fits_undersize: bool,
    pub(super) oversize_profile_is_preserved: bool,
}

pub(super) fn execute_regrind_stage(
    registries: &Registries,
    episode: &mut OreProbeEpisode,
    oversize_mass: Mass,
    grinder_condition: Condition,
) -> Result<RegrindStageResult, OreProbeOutcome> {
    let screen_definition = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let grinder_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let fine_grind_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));
    let fine_output_fits_undersize = fine_grind_definition
        .output_particle_size_distribution()
        .classes()
        .iter()
        .all(|class| class.range().maximum_diameter() <= screen_definition.aperture());
    if oversize_mass.is_zero() {
        return Ok(RegrindStageResult {
            powered: PoweredStageResult {
                energy: Energy::ZERO,
                duration: TickSpan::new(0),
                condition_after: grinder_condition,
            },
            fine_output_fits_undersize,
            oversize_profile_is_preserved: true,
        });
    }

    let fine_selection = select_stockpile_mass(
        &episode.state,
        episode.ids.oversize_storage,
        oversize_mass,
        "screen oversize output",
    );
    let ground_classes = grinder_definition
        .output_particle_size_distribution()
        .classes();
    let oversize_profile_is_preserved = fine_selection.iter().all(|selection| {
        episode
            .state
            .inventory()
            .get_lot(selection.lot())
            .is_some_and(|lot| {
                lot.composition() == &episode.input_composition
                    && lot
                        .particle_size_distribution()
                        .is_some_and(|distribution| {
                            distribution.classes().iter().all(|class| {
                                class.range().minimum_diameter() > screen_definition.aperture()
                                    && ground_classes.contains(class)
                            })
                        })
            })
    });
    let powered = execute_comminution_stage(
        registries,
        episode,
        ComminutionStageRequest {
            stage: "regrind-oversize",
            process: PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            source: episode.ids.oversize_storage,
            selections: fine_selection.as_slice(),
            equipment: episode.ids.grinder,
            destination: episode.ids.undersize_storage,
            activity: "ore fine grinding",
            failure_context: "canonical fine-grinding probe resolution failed",
        },
    )?;
    Ok(RegrindStageResult {
        powered,
        fine_output_fits_undersize,
        oversize_profile_is_preserved,
    })
}

#[derive(Clone, Copy)]
pub(super) struct ScreeningStageResult {
    pub(super) powered: PoweredStageResult,
    pub(super) undersize_mass: Mass,
    pub(super) oversize_mass: Mass,
}

pub(super) struct ScreeningStageRequest<'a> {
    pub(super) source: StockpileId,
    pub(super) selections: &'a [MaterialLotSelection],
    pub(super) undersize_destination: StockpileId,
    pub(super) oversize_destination: StockpileId,
}

pub(super) fn execute_screening_stage(
    registries: &Registries,
    episode: &mut OreProbeEpisode,
    request: ScreeningStageRequest<'_>,
) -> Result<ScreeningStageResult, OreProbeOutcome> {
    let resolved = match resolve_screening_process(
        registries,
        &episode.state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            request.source,
            request.selections,
            episode.ids.screen,
            episode.ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ScreeningResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            store: _,
            available,
            requested,
        })) if episode.case.role() != FocusedProbeRole::MaintainedAnchor => {
            return Err(report_ore_energy_stop(
                registries,
                &episode.state,
                episode.ids,
                episode.case,
                episode.initial_matter,
                OreEnergyStop {
                    stage: "screen",
                    available,
                    requested,
                },
            ));
        }
        Err(ScreeningResolutionError::BatchMassExceeded { .. })
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                "screen",
                OreStopReason::EquipmentCapacity,
            ));
        }
        Err(ScreeningResolutionError::ConditionDuration(_))
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                "screen",
                OreStopReason::ConditionLifetime,
            ));
        }
        Err(error) => panic!("canonical screening probe resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let undersize_mass = resolved.undersize_mass();
    let oversize_mass = resolved.oversize_mass();
    let result = ScreeningStageResult {
        powered: PoweredStageResult {
            energy: resolved.required_energy(),
            duration,
            condition_after: resolved.condition_after(),
        },
        undersize_mass,
        oversize_mass,
    };
    let mut routes = Vec::with_capacity(2);
    if !undersize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            request.undersize_destination,
        ));
    }
    if !oversize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::OVERSIZE_STREAM,
            request.oversize_destination,
        ));
    }
    let job = validate_start_process_routed(
        registries,
        &episode.state,
        resolved.process_resolution(),
        request.source,
        &routes,
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut episode.state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut episode.state,
        job,
        duration,
        "ore screening",
    );
    validate_loaded_state(registries, &episode.state)
        .unwrap_or_else(|error| panic!("ore preparation post-screen audit failed: {error}"));
    Ok(result)
}

#[derive(Clone, Copy)]
pub(super) struct ConcentrationStageResult {
    pub(super) powered: PoweredStageResult,
    pub(super) target_mass: Mass,
    pub(super) residue_mass: Mass,
}

pub(super) fn execute_concentration_stage(
    registries: &Registries,
    episode: &mut OreProbeEpisode,
    selections: &[MaterialLotSelection],
) -> Result<ConcentrationStageResult, OreProbeOutcome> {
    let resolved = match resolve_constituent_separation_process(
        registries,
        &episode.state,
        ConstituentSeparationRequest::new(
            PROCESS_CONCENTRATE_COPPER,
            episode.ids.undersize_storage,
            selections,
            episode.ids.separator,
            episode.ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ConstituentSeparationResolutionError::Energy(
            EnergySupplyError::InsufficientEnergy {
                store: _,
                available,
                requested,
            },
        )) if episode.case.role() != FocusedProbeRole::MaintainedAnchor => {
            return Err(report_ore_energy_stop(
                registries,
                &episode.state,
                episode.ids,
                episode.case,
                episode.initial_matter,
                OreEnergyStop {
                    stage: "concentrate",
                    available,
                    requested,
                },
            ));
        }
        Err(ConstituentSeparationResolutionError::BatchMassExceeded { .. })
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                "concentrate",
                OreStopReason::EquipmentCapacity,
            ));
        }
        Err(ConstituentSeparationResolutionError::ConditionDuration(_))
            if episode.case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return Err(report_ore_runtime_stop(
                registries,
                &episode.state,
                episode.case,
                episode.initial_matter,
                "concentrate",
                OreStopReason::ConditionLifetime,
            ));
        }
        Err(error) => panic!("copper concentration resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let result = ConcentrationStageResult {
        powered: PoweredStageResult {
            energy: resolved.required_energy(),
            duration,
            condition_after: resolved.condition_after(),
        },
        target_mass: resolved.target_mass(),
        residue_mass: resolved.residue_mass(),
    };
    let job = validate_start_process_routed(
        registries,
        &episode.state,
        resolved.process_resolution(),
        episode.ids.undersize_storage,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                episode.ids.concentrate_storage,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                episode.ids.tailings_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("copper concentration start failed: {error}"))
    .commit(&mut episode.state)
    .unwrap_or_else(|error| panic!("copper concentration commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut episode.state,
        job,
        duration,
        "copper concentration",
    );
    validate_loaded_state(registries, &episode.state)
        .unwrap_or_else(|error| panic!("ore preparation post-concentration audit failed: {error}"));
    Ok(result)
}
