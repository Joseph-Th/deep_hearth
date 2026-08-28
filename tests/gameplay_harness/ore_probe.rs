//! Focused ore-preparation capability probe.

use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::ore_setup::{OrePreparationProbeIds, OrePreparationSetup, setup_ore_preparation_probe};
use super::production_support::{
    finish_uninterrupted_production_job, select_stockpile_mass, varied_healthy_condition,
};
use super::seed::mix64;
use super::support::nominal_equipment_mass_capability;
use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, FORM_CONCENTRATE, FORM_TAILINGS, MATERIAL_CLAY,
    MATERIAL_COPPER, MATERIAL_STONE, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{AggregateMass, Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{EnergySupplyError, calculate_mass_specific_energy};
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionBatchError, ComminutionRequest, ComminutionResolutionError,
    ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    ConstituentSeparationResolutionError, ScreeningBatchError, ScreeningProcessDefinition,
    ScreeningRequest, ScreeningResolutionError, resolve_comminution_process,
    resolve_constituent_separation_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, validate_start_process, validate_start_process_routed,
};
use deep_hearth::registry::Registries;

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn represented_copper_ppm_mg(state: &AppState, stockpiles: &[StockpileId]) -> u128 {
    stockpiles
        .iter()
        .flat_map(|stockpile| state.inventory().lot_ids(*stockpile))
        .map(|lot| {
            let record = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("ore preparation accounting lot disappeared"));
            u128::from(record.mass().milligrams())
                * u128::from(record.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum()
}

pub(super) fn probe_parameters(registries: &Registries, seed: u64) -> OrePreparationSetup {
    let crusher = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let grinder = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let screening = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let fine_grind = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));
    let concentration = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("canonical copper concentration definition disappeared"));

    let distribution = grinder.output_particle_size_distribution();
    let aperture = screening.aperture();
    let mut undersize_weight = 0_u64;
    for class in distribution.classes() {
        let range = class.range();
        if range.maximum_diameter() <= aperture {
            undersize_weight += u64::from(class.weight());
        } else if range.minimum_diameter() <= aperture {
            panic!(
                "authored grinder particle class {}..={}um crosses screen aperture {}um",
                range.minimum_diameter().micrometers(),
                range.maximum_diameter().micrometers(),
                aperture.micrometers()
            );
        }
    }
    let total_weight = distribution.total_weight();
    let representable_unit = total_weight / greatest_common_divisor(total_weight, undersize_weight);

    let mut batch_limits = vec![
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            crusher.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            grinder.max_batch_mass_capability(),
        ),
        nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_DRY_SCREEN,
            screening.max_batch_mass_capability(),
        ),
    ];
    if undersize_weight < total_weight {
        batch_limits.push(nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_GRINDING_MILL,
            fine_grind.max_batch_mass_capability(),
        ));
    }
    let maximum_batch = batch_limits
        .into_iter()
        .map(Mass::milligrams)
        .min()
        .unwrap_or_else(|| panic!("ore preparation probe has no authored batch constraints"));
    let maximum_units = maximum_batch / representable_unit;
    assert!(
        maximum_units > 0,
        "authored screen partition cannot be represented within the equipment batch limits"
    );
    let minimum_units = maximum_units.div_ceil(2);
    let unit_count =
        minimum_units + mix64(seed ^ 0x0AE5_1A5E) % (maximum_units - minimum_units + 1);
    let batch_mass = Mass::from_milligrams(representable_unit * unit_count);
    let copper_ppm = 300_000 + (mix64(seed ^ 0xC0FF_EE11) % 400_001) as u32;
    let clay_share_ppm = 100_000 + (mix64(seed ^ 0x4741_4E47_5545_4D49) % 500_001) as u32;
    let required_energy_upper_bound = [
        crusher.specific_energy(),
        grinder.specific_energy(),
        screening.specific_energy(),
        fine_grind.specific_energy(),
        concentration.specific_energy(),
    ]
    .into_iter()
    .map(|specific| calculate_mass_specific_energy(batch_mass, specific))
    .try_fold(Energy::ZERO, |total, energy| total.checked_add(energy))
    .unwrap_or_else(|| panic!("ore preparation chain energy upper bound overflowed"));
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    assert!(
        drive_capacity >= required_energy_upper_bound,
        "authored industrial drive must remain capable of the maintained full-chain ore contract"
    );
    // Seed-only pressure makes exact replays independent of runner role. Fresh samples range from
    // materially under-provisioned to comfortably funded, so the actor can encounter real finite-
    // energy stops instead of every generated world being pre-funded to success.
    let energy_budget_ppm = 400_000 + (mix64(seed ^ 0x454E_4552_4759_4845) % 950_001) as u32;
    let varied_budget = Energy::from_nanojoules(
        required_energy_upper_bound
            .nanojoules()
            .checked_mul(u128::from(energy_budget_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("ore preparation energy budget scaling overflowed")),
    );
    let drive_energy = std::cmp::min(varied_budget, drive_capacity);
    OrePreparationSetup {
        batch_mass,
        copper_ppm,
        clay_share_ppm,
        crusher_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            mix64(seed ^ 0x4352_5553_4843_4F4E),
        ),
        grinder_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_GRINDING_MILL,
            mix64(seed ^ 0x4752_494E_4443_4F4E),
        ),
        screen_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_DRY_SCREEN,
            mix64(seed ^ 0x5343_5245_454E_434F),
        ),
        separator_condition: varied_healthy_condition(
            registries,
            EQUIPMENT_GRAVITY_SEPARATOR,
            mix64(seed ^ 0x5345_5041_5241_544F),
        ),
        drive_energy,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OreStopReason {
    FiniteEnergy,
    EquipmentCapacity,
    ConditionLifetime,
}

impl OreStopReason {
    const fn label(self) -> &'static str {
        match self {
            Self::FiniteEnergy => "finite-energy",
            Self::EquipmentCapacity => "equipment-capacity",
            Self::ConditionLifetime => "condition-lifetime",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OreProbeOutcome {
    Completed,
    Stopped {
        stage: &'static str,
        reason: OreStopReason,
    },
}

fn report_ore_energy_stop(
    registries: &Registries,
    state: &AppState,
    ids: OrePreparationProbeIds,
    case: FocusedProbeCase,
    initial_matter: AggregateMass,
    stage: &'static str,
    available: Energy,
    requested: Energy,
) -> OreProbeOutcome {
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("ore preparation stop-state audit failed: {error}"));
    let current_matter = calculate_matter_accounting(state)
        .unwrap_or_else(|error| panic!("ore preparation stop-state matter audit failed: {error}"))
        .total();
    assert_eq!(
        current_matter, initial_matter,
        "energy-limited ore preparation must conserve represented matter before stopping"
    );
    let stored = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared at energy-limited stop"));
    assert_eq!(stored, available);
    std::println!(
        "ORE REVIEW seed=0x{:016X} sample={} role=capability-only outcome=stopped stage={stage} blocker=finite-energy available={}nJ requested={}nJ tick={} matter=conserved",
        case.seed(),
        focused_probe_role_label(case.role()),
        available.nanojoules(),
        requested.nanojoules(),
        state.tick().value(),
    );
    OreProbeOutcome::Stopped {
        stage,
        reason: OreStopReason::FiniteEnergy,
    }
}

fn report_ore_runtime_stop(
    registries: &Registries,
    state: &AppState,
    case: FocusedProbeCase,
    initial_matter: AggregateMass,
    stage: &'static str,
    reason: OreStopReason,
) -> OreProbeOutcome {
    validate_loaded_state(registries, state)
        .unwrap_or_else(|error| panic!("ore preparation stop-state audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(state)
            .unwrap_or_else(|error| panic!(
                "ore preparation stop-state matter audit failed: {error}"
            ))
            .total(),
        initial_matter,
        "runtime-limited ore preparation must conserve represented matter before stopping"
    );
    std::println!(
        "ORE REVIEW seed=0x{:016X} sample={} role=capability-only outcome=stopped stage={stage} blocker={} tick={} matter=conserved",
        case.seed(),
        focused_probe_role_label(case.role()),
        reason.label(),
        state.tick().value(),
    );
    OreProbeOutcome::Stopped { stage, reason }
}

pub(super) fn evaluate_ore_preparation_capability_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> OreProbeOutcome {
    let seed = case.seed();
    let setup = probe_parameters(registries, seed);
    let batch_mass = setup.batch_mass;
    let initial_crusher_condition = setup.crusher_condition;
    let initial_grinder_condition = setup.grinder_condition;
    let initial_screen_condition = setup.screen_condition;
    let (mut state, ids) = setup_ore_preparation_probe(registries, seed, setup);
    let initial_separator_condition = state
        .equipment()
        .get_equipment(ids.separator)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation assembled separator disappeared"));
    let crusher_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let grinder_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical grinder definition disappeared"));
    let screen_definition = registries
        .ore_processing()
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("canonical screen definition disappeared"));
    let fine_grind_definition = registries
        .ore_processing()
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("canonical fine-grind definition disappeared"));
    let concentration_definition = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("canonical copper concentration definition disappeared"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation initial matter accounting failed: {error}"))
        .total();
    let initial_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared"));
    let input_composition = state
        .inventory()
        .get_lot(ids.ore_lot)
        .unwrap_or_else(|| panic!("ore preparation input lot disappeared after setup"))
        .composition()
        .clone();
    let input_copper_ppm = input_composition.parts_per_million(MATERIAL_COPPER);
    let input_stone_ppm = input_composition.parts_per_million(MATERIAL_STONE);
    let input_clay_ppm = input_composition.parts_per_million(MATERIAL_CLAY);

    let crush_selection = [MaterialLotSelection::new(ids.ore_lot, batch_mass)];
    let crushed = match resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &crush_selection,
            ids.crusher,
            ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            store: _,
            available,
            requested,
        })) if case.role() != FocusedProbeRole::MaintainedAnchor => {
            return report_ore_energy_stop(
                registries,
                &state,
                ids,
                case,
                initial_matter,
                "crush",
                available,
                requested,
            );
        }
        Err(ComminutionResolutionError::BatchMassExceeded { .. })
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "crush",
                OreStopReason::EquipmentCapacity,
            );
        }
        Err(ComminutionResolutionError::ConditionDuration(_))
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "crush",
                OreStopReason::ConditionLifetime,
            );
        }
        Err(error) => panic!("canonical crushing probe resolution failed: {error}"),
    };
    let crush_duration = crushed.process_resolution().duration();
    let crush_energy = crushed.required_energy();
    let crusher_condition = crushed.condition_after();
    let crush_job = validate_start_process(
        registries,
        &state,
        crushed.process_resolution(),
        ids.ore_source,
        ids.crushed_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation crushing commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        crush_job,
        crush_duration,
        "ore crushing",
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-crush audit failed: {error}"));

    let crushed_selection = select_stockpile_mass(
        &state,
        ids.crushed_storage,
        batch_mass,
        "crushed ore output",
    );
    let crusher_output_matches_authoring = crushed_selection.iter().all(|selection| {
        state
            .inventory()
            .get_lot(selection.lot())
            .and_then(|lot| lot.particle_size_distribution())
            == Some(crusher_definition.output_particle_size_distribution())
    });
    if case.role() == FocusedProbeRole::MaintainedAnchor {
        match resolve_screening_process(
            registries,
            &state,
            ScreeningRequest::new(
                PROCESS_SCREEN_CRUSHED_ORE,
                ids.crushed_storage,
                crushed_selection.as_slice(),
                ids.screen,
                ids.drive,
            ),
        ) {
            Ok(_)
            | Err(ScreeningResolutionError::Batch(
                ScreeningBatchError::UnresolvedParticleClass { .. },
            )) => {}
            Err(error) => panic!("direct-screen route failed unexpectedly: {error}"),
        }
        match resolve_comminution_process(
            registries,
            &state,
            ComminutionRequest::new(
                PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
                ids.crushed_storage,
                crushed_selection.as_slice(),
                ids.grinder,
                ids.drive,
            ),
        ) {
            Ok(_)
            | Err(ComminutionResolutionError::Batch(
                ComminutionBatchError::InputParticleSizeOutsideOperatingRange { .. },
            )) => {}
            Err(error) => panic!("direct fine-grind route failed unexpectedly: {error}"),
        }
    }

    let ground = match resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_GRIND_CRUSHED_ORE,
            ids.crushed_storage,
            crushed_selection.as_slice(),
            ids.grinder,
            ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            store: _,
            available,
            requested,
        })) if case.role() != FocusedProbeRole::MaintainedAnchor => {
            return report_ore_energy_stop(
                registries,
                &state,
                ids,
                case,
                initial_matter,
                "grind",
                available,
                requested,
            );
        }
        Err(ComminutionResolutionError::BatchMassExceeded { .. })
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "grind",
                OreStopReason::EquipmentCapacity,
            );
        }
        Err(ComminutionResolutionError::ConditionDuration(_))
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "grind",
                OreStopReason::ConditionLifetime,
            );
        }
        Err(error) => panic!("canonical grinding probe resolution failed: {error}"),
    };
    let grind_duration = ground.process_resolution().duration();
    let grind_energy = ground.required_energy();
    let grinder_condition = ground.condition_after();
    let grind_job = validate_start_process(
        registries,
        &state,
        ground.process_resolution(),
        ids.crushed_storage,
        ids.ground_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinding start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation grinding commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        grind_job,
        grind_duration,
        "ore grinding",
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-grind audit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(ids.grinder)
            .map(|equipment| equipment.condition()),
        Some(grinder_condition),
        "grinder condition must match the resolved wear projection"
    );

    let ground_selection =
        select_stockpile_mass(&state, ids.ground_storage, batch_mass, "ground ore output");
    let grinding_matches_authoring = ground_selection.iter().all(|selection| {
        state
            .inventory()
            .get_lot(selection.lot())
            .and_then(|lot| lot.particle_size_distribution())
            == Some(grinder_definition.output_particle_size_distribution())
    });
    let ground_classes = grinder_definition
        .output_particle_size_distribution()
        .classes();
    let grinding_resolved_screen_cut = ground_classes.iter().all(|class| {
        class.range().maximum_diameter() <= screen_definition.aperture()
            || class.range().minimum_diameter() > screen_definition.aperture()
    });

    let screened = match resolve_screening_process(
        registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ids.ground_storage,
            ground_selection.as_slice(),
            ids.screen,
            ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ScreeningResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            store: _,
            available,
            requested,
        })) if case.role() != FocusedProbeRole::MaintainedAnchor => {
            return report_ore_energy_stop(
                registries,
                &state,
                ids,
                case,
                initial_matter,
                "screen",
                available,
                requested,
            );
        }
        Err(ScreeningResolutionError::BatchMassExceeded { .. })
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "screen",
                OreStopReason::EquipmentCapacity,
            );
        }
        Err(ScreeningResolutionError::ConditionDuration(_))
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "screen",
                OreStopReason::ConditionLifetime,
            );
        }
        Err(error) => panic!("canonical screening probe resolution failed: {error}"),
    };
    let screen_duration = screened.process_resolution().duration();
    let screen_energy = screened.required_energy();
    let screen_condition = screened.condition_after();
    let screened_undersize_mass = screened.undersize_mass();
    let screened_oversize_mass = screened.oversize_mass();
    let mut routes = Vec::with_capacity(2);
    if !screened_undersize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            ids.undersize_storage,
        ));
    }
    if !screened_oversize_mass.is_zero() {
        routes.push(ProcessOutputRoute::new(
            ScreeningProcessDefinition::OVERSIZE_STREAM,
            ids.oversize_storage,
        ));
    }
    let screen_job = validate_start_process_routed(
        registries,
        &state,
        screened.process_resolution(),
        ids.ground_storage,
        &routes,
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        screen_job,
        screen_duration,
        "ore screening",
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-screen audit failed: {error}"));

    let fine_output_fits_undersize = fine_grind_definition
        .output_particle_size_distribution()
        .classes()
        .iter()
        .all(|class| class.range().maximum_diameter() <= screen_definition.aperture());
    let (fine_energy, fine_duration_ticks, final_grinder_projection, oversize_profile_is_preserved) =
        if screened_oversize_mass.is_zero() {
            (Energy::ZERO, 0, grinder_condition, true)
        } else {
            let fine_selection = select_stockpile_mass(
                &state,
                ids.oversize_storage,
                screened_oversize_mass,
                "screen oversize output",
            );
            let oversize_profile_is_preserved = fine_selection.iter().all(|selection| {
                state
                    .inventory()
                    .get_lot(selection.lot())
                    .is_some_and(|lot| {
                        lot.composition() == &input_composition
                            && lot
                                .particle_size_distribution()
                                .is_some_and(|distribution| {
                                    distribution.classes().iter().all(|class| {
                                        class.range().minimum_diameter()
                                            > screen_definition.aperture()
                                            && ground_classes.contains(class)
                                    })
                                })
                    })
            });
            let fine_ground = match resolve_comminution_process(
                registries,
                &state,
                ComminutionRequest::new(
                    PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
                    ids.oversize_storage,
                    fine_selection.as_slice(),
                    ids.grinder,
                    ids.drive,
                ),
            ) {
                Ok(resolved) => resolved,
                Err(ComminutionResolutionError::Energy(
                    EnergySupplyError::InsufficientEnergy {
                        store: _,
                        available,
                        requested,
                    },
                )) if case.role() != FocusedProbeRole::MaintainedAnchor => {
                    return report_ore_energy_stop(
                        registries,
                        &state,
                        ids,
                        case,
                        initial_matter,
                        "regrind-oversize",
                        available,
                        requested,
                    );
                }
                Err(ComminutionResolutionError::BatchMassExceeded { .. })
                    if case.role() != FocusedProbeRole::MaintainedAnchor =>
                {
                    return report_ore_runtime_stop(
                        registries,
                        &state,
                        case,
                        initial_matter,
                        "regrind-oversize",
                        OreStopReason::EquipmentCapacity,
                    );
                }
                Err(ComminutionResolutionError::ConditionDuration(_))
                    if case.role() != FocusedProbeRole::MaintainedAnchor =>
                {
                    return report_ore_runtime_stop(
                        registries,
                        &state,
                        case,
                        initial_matter,
                        "regrind-oversize",
                        OreStopReason::ConditionLifetime,
                    );
                }
                Err(error) => {
                    panic!("canonical fine-grinding probe resolution failed: {error}")
                }
            };
            let fine_duration = fine_ground.process_resolution().duration();
            let fine_energy = fine_ground.required_energy();
            let final_grinder_projection = fine_ground.condition_after();
            let fine_job = validate_start_process(
                registries,
                &state,
                fine_ground.process_resolution(),
                ids.oversize_storage,
                ids.undersize_storage,
            )
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding commit failed: {error}"));
            finish_uninterrupted_production_job(
                registries,
                &mut state,
                fine_job,
                fine_duration,
                "ore fine grinding",
            );
            validate_loaded_state(registries, &state).unwrap_or_else(|error| {
                panic!("ore preparation post-regrind audit failed: {error}")
            });
            (
                fine_energy,
                fine_duration.value(),
                final_grinder_projection,
                oversize_profile_is_preserved,
            )
        };

    let selection = select_stockpile_mass(
        &state,
        ids.undersize_storage,
        batch_mass,
        "full fine liberated feed for industrial copper concentration",
    );
    let concentrated = match resolve_constituent_separation_process(
        registries,
        &state,
        ConstituentSeparationRequest::new(
            PROCESS_CONCENTRATE_COPPER,
            ids.undersize_storage,
            selection.as_slice(),
            ids.separator,
            ids.drive,
        ),
    ) {
        Ok(resolved) => resolved,
        Err(ConstituentSeparationResolutionError::Energy(
            EnergySupplyError::InsufficientEnergy {
                store: _,
                available,
                requested,
            },
        )) if case.role() != FocusedProbeRole::MaintainedAnchor => {
            return report_ore_energy_stop(
                registries,
                &state,
                ids,
                case,
                initial_matter,
                "concentrate",
                available,
                requested,
            );
        }
        Err(ConstituentSeparationResolutionError::BatchMassExceeded { .. })
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "concentrate",
                OreStopReason::EquipmentCapacity,
            );
        }
        Err(ConstituentSeparationResolutionError::ConditionDuration(_))
            if case.role() != FocusedProbeRole::MaintainedAnchor =>
        {
            return report_ore_runtime_stop(
                registries,
                &state,
                case,
                initial_matter,
                "concentrate",
                OreStopReason::ConditionLifetime,
            );
        }
        Err(error) => panic!("copper concentration resolution failed: {error}"),
    };
    let concentration_duration = concentrated.process_resolution().duration();
    let concentration_duration_ticks = concentration_duration.value();
    let concentration_energy = concentrated.required_energy();
    let final_separator_projection = concentrated.condition_after();
    let concentration_batches = 1_u64;
    let job = validate_start_process_routed(
        registries,
        &state,
        concentrated.process_resolution(),
        ids.undersize_storage,
        &[
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::TARGET_STREAM,
                ids.concentrate_storage,
            ),
            ProcessOutputRoute::new(
                ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                ids.tailings_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("copper concentration start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("copper concentration commit failed: {error}"));
    finish_uninterrupted_production_job(
        registries,
        &mut state,
        job,
        concentration_duration,
        "copper concentration",
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("ore preparation post-concentration audit failed: {error}"));

    let final_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation final matter accounting failed: {error}"))
        .total();
    let final_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared after completion"));
    let final_crusher_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation crusher disappeared after completion"));
    let final_grinder_condition = state
        .equipment()
        .get_equipment(ids.grinder)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation grinder disappeared after completion"));
    let final_screen_condition = state
        .equipment()
        .get_equipment(ids.screen)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation screen disappeared after completion"));
    let final_separator_condition = state
        .equipment()
        .get_equipment(ids.separator)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation separator disappeared after completion"));
    let undersize_mass = state
        .inventory()
        .get_stockpile(ids.undersize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation undersize storage disappeared"));
    let oversize_mass = state
        .inventory()
        .get_stockpile(ids.oversize_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation oversize storage disappeared"));
    let concentrate_mass = state
        .inventory()
        .get_stockpile(ids.concentrate_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation concentrate storage disappeared"));
    let tailings_mass = state
        .inventory()
        .get_stockpile(ids.tailings_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("ore preparation tailings storage disappeared"));
    let concentrate_identity_is_valid =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .all(|lot| {
                state.inventory().get_lot(lot).is_some_and(|lot| {
                    lot.commodity().material() == MATERIAL_COPPER
                        && lot.commodity().form() == FORM_CONCENTRATE
                        && lot.composition().parts_per_million(MATERIAL_COPPER) != 0
                })
            });
    let concentrate_contains_gangue =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .any(|lot| {
                state.inventory().get_lot(lot).is_some_and(|lot| {
                    lot.composition().parts_per_million(MATERIAL_COPPER) < 1_000_000
                })
            });
    let concentrate_distribution_is_fine =
        state
            .inventory()
            .lot_ids(ids.concentrate_storage)
            .all(|lot| {
                state
                    .inventory()
                    .get_lot(lot)
                    .and_then(|lot| lot.particle_size_distribution())
                    .is_some_and(|distribution| {
                        distribution.classes().iter().all(|class| {
                            class.range().maximum_diameter() <= screen_definition.aperture()
                        })
                    })
            });
    let tailings_retain_unrecovered_copper =
        state.inventory().lot_ids(ids.tailings_storage).any(|lot| {
            state
                .inventory()
                .get_lot(lot)
                .is_some_and(|lot| lot.composition().parts_per_million(MATERIAL_COPPER) != 0)
        });
    let tailings_distribution_is_fine =
        state.inventory().lot_ids(ids.tailings_storage).all(|lot| {
            state.inventory().get_lot(lot).is_some_and(|lot| {
                lot.commodity().form() == FORM_TAILINGS
                    && lot
                        .particle_size_distribution()
                        .is_some_and(|distribution| {
                            distribution.classes().iter().all(|class| {
                                class.range().maximum_diameter() <= screen_definition.aperture()
                            })
                        })
            })
        });
    let represented_copper =
        represented_copper_ppm_mg(&state, &[ids.concentrate_storage, ids.tailings_storage]);
    let concentrate_copper = represented_copper_ppm_mg(&state, &[ids.concentrate_storage]);
    let expected_copper = u128::from(batch_mass.milligrams()) * u128::from(input_copper_ppm);
    let expected_recovered_copper_milligrams = u64::try_from(
        expected_copper * u128::from(concentration_definition.target_recovery_ppm())
            / 1_000_000_000_000_u128,
    )
    .unwrap_or_else(|_| panic!("ore preparation recovered copper mass exceeded u64"));
    let concentrate_grade_ppm =
        u32::try_from(concentrate_copper / u128::from(concentrate_mass.milligrams()))
            .unwrap_or_else(|_| {
                panic!("ore preparation concentrate grade exceeded normalized ppm")
            });
    let consumed_energy = crush_energy
        .checked_add(grind_energy)
        .and_then(|energy| energy.checked_add(screen_energy))
        .and_then(|energy| energy.checked_add(fine_energy))
        .and_then(|energy| energy.checked_add(concentration_energy))
        .unwrap_or_else(|| panic!("ore preparation consumed energy overflowed"));

    assert_eq!(
        final_matter, initial_matter,
        "ore preparation must conserve world matter"
    );
    assert_eq!(
        initial_energy.checked_sub(consumed_energy),
        Some(final_energy),
        "ore preparation must consume exactly the resolved work energy"
    );
    assert_eq!(
        final_crusher_condition, crusher_condition,
        "crusher condition must match resolved wear"
    );
    assert_eq!(
        final_grinder_condition, final_grinder_projection,
        "grinder condition must match resolved wear"
    );
    assert_eq!(
        final_screen_condition, screen_condition,
        "screen condition must match resolved wear"
    );
    assert_eq!(
        final_separator_condition, final_separator_projection,
        "separator condition must match resolved wear"
    );
    assert_eq!(
        undersize_mass,
        Mass::ZERO,
        "prepared feed must be consumed into concentrate and tailings"
    );
    assert_eq!(
        oversize_mass,
        Mass::ZERO,
        "oversize storage must be empty after regrind"
    );
    assert_eq!(
        concentrate_mass.checked_add(tailings_mass),
        Some(batch_mass),
        "concentration outputs must conserve the prepared feed mass"
    );
    assert_eq!(
        represented_copper, expected_copper,
        "concentration must conserve exact represented copper content"
    );
    assert!(
        !concentrate_mass.is_zero(),
        "concentration must recover copper"
    );
    assert!(
        !tailings_mass.is_zero(),
        "concentration must produce physical tailings"
    );
    assert_eq!(
        concentrate_copper,
        u128::from(expected_recovered_copper_milligrams) * 1_000_000,
        "industrial concentration must apply the authored finite target recovery to exact copper content"
    );
    assert!(
        concentrate_mass.milligrams() > expected_recovered_copper_milligrams,
        "finite non-target recovery must carry physical gangue into the concentrate stream"
    );
    assert!(
        concentrate_grade_ppm > input_copper_ppm && concentrate_grade_ppm < 1_000_000,
        "industrial concentration must improve feed grade without fabricating pure copper"
    );

    let qualitative_requirements = [
        (
            "crusher output matches authored particle state",
            crusher_output_matches_authoring,
        ),
        (
            "grinder output matches authored particle state",
            grinding_matches_authoring,
        ),
        (
            "grinder output resolves the authored screen cut",
            grinding_resolved_screen_cut,
        ),
        (
            "fine-grind output fits the authored screen undersize",
            screened_oversize_mass.is_zero() || fine_output_fits_undersize,
        ),
        (
            "screen oversize preserves its particle profile",
            oversize_profile_is_preserved,
        ),
        (
            "probe feed exercises variable multi-constituent gangue",
            input_composition.components().len() >= 3,
        ),
        (
            "copper concentrate retains target identity while carrying selectively recovered gangue",
            concentrate_identity_is_valid && concentrate_contains_gangue,
        ),
        (
            "copper concentrate retains the liberated fine-particle state",
            concentrate_distribution_is_fine,
        ),
        (
            "finite concentration recovery leaves unrecovered copper in physical tailings",
            tailings_retain_unrecovered_copper,
        ),
        (
            "tailings retain the physically prepared fine particle state in a terminal current-tier form",
            tailings_distribution_is_fine,
        ),
        (
            "industrial concentration accepts the prepared batch as one operation",
            concentration_batches == 1,
        ),
    ];
    for (name, observed) in qualitative_requirements {
        assert!(
            observed,
            "ore-preparation capability contract failed: {name}"
        );
    }

    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "CAPABILITY ORE_PREP seed=0x{seed:016X} sample={} outcome=completed reachability=bootstrapped-industrial installation=required+structurally-supported role=capability-evidence player-loop=not-claimed system-depth=[particle-state,routing,finite-work,wear,constituent-concentration] batch={}mg feed=[copper:{}ppm stone:{}ppm clay:{}ppm] concentrate={}mg tailings={}mg concentrate-grade={}ppm target-recovery={}ppm gangue-recovery={}ppm initial-condition=[crusher:{} grinder:{} screen:{} separator:{}ppm] stored-work=[initial:{}nJ consumed:{}nJ remaining:{}nJ] stages=[crush:{}t grind:{}t screen:{}t regrind:{}t concentrate:{}b/{}t] matter=conserved composition=exact energy=resolved",
            focused_probe_role_label(case.role()),
            batch_mass.milligrams(),
            input_copper_ppm,
            input_stone_ppm,
            input_clay_ppm,
            concentrate_mass.milligrams(),
            tailings_mass.milligrams(),
            concentrate_grade_ppm,
            concentration_definition.target_recovery_ppm(),
            concentration_definition.non_target_recovery_ppm(),
            initial_crusher_condition.parts_per_million(),
            initial_grinder_condition.parts_per_million(),
            initial_screen_condition.parts_per_million(),
            initial_separator_condition.parts_per_million(),
            initial_energy.nanojoules(),
            consumed_energy.nanojoules(),
            final_energy.nanojoules(),
            crush_duration.value(),
            grind_duration.value(),
            screen_duration.value(),
            fine_duration_ticks,
            concentration_batches,
            concentration_duration_ticks,
        );
    } else {
        std::println!(
            "ORE REVIEW seed=0x{seed:016X} sample={} role=capability-only outcome=completed pipeline=crush->grind->screen->regrind->concentrate batch={}mg feed=[copper:{}ppm stone:{}ppm clay:{}ppm] concentrate={}mg tailings={}mg concentrate-grade={}ppm target-recovery={}ppm gangue-recovery={}ppm stored-work=[used:{}nJ remaining:{}nJ] durations=[{}+{}+{}+{}t concentration:{}b/{}t] matter=conserved composition=exact",
            focused_probe_role_label(case.role()),
            batch_mass.milligrams(),
            input_copper_ppm,
            input_stone_ppm,
            input_clay_ppm,
            concentrate_mass.milligrams(),
            tailings_mass.milligrams(),
            concentrate_grade_ppm,
            concentration_definition.target_recovery_ppm(),
            concentration_definition.non_target_recovery_ppm(),
            consumed_energy.nanojoules(),
            final_energy.nanojoules(),
            crush_duration.value(),
            grind_duration.value(),
            screen_duration.value(),
            fine_duration_ticks,
            concentration_batches,
            concentration_duration_ticks,
        );
    }
    OreProbeOutcome::Completed
}

pub(super) fn run_ore_preparation_capability_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) {
    let outcome = evaluate_ore_preparation_capability_probe(registries, case);
    if case.role() == FocusedProbeRole::MaintainedCoverage {
        assert_eq!(case.seed(), 2, "unknown maintained ore coverage seed");
        assert!(
            matches!(
                outcome,
                OreProbeOutcome::Stopped {
                    reason: OreStopReason::FiniteEnergy,
                    ..
                }
            ),
            "ore coverage seed 2 must preserve a canonical finite-work stop"
        );
    }
}
