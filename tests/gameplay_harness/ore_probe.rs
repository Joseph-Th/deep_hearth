//! Focused ore-preparation capability probe.

use super::equipment_support::nominal_equipment_mass_capability;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::material_selection::select_stockpile_mass;
use super::ore_setup::{OrePreparationProbeIds, OrePreparationSetup, setup_ore_preparation_probe};
use super::production_support::varied_healthy_condition;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, FORM_CONCENTRATE, FORM_TAILINGS, MATERIAL_CLAY,
    MATERIAL_COPPER, MATERIAL_STONE, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_SCREEN_CRUSHED_ORE,
};
use deep_hearth::core::quantity::{AggregateMass, Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::TickSpan;
use deep_hearth::energy::{EnergySupplyError, calculate_mass_specific_energy};
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::MaterialComposition;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionBatchError, ComminutionRequest, ComminutionResolutionError,
    ConstituentSeparationProcessDefinition, ConstituentSeparationRequest,
    ConstituentSeparationResolutionError, ScreeningBatchError, ScreeningProcessDefinition,
    ScreeningRequest, ScreeningResolutionError, resolve_comminution_process,
    resolve_constituent_separation_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessId, ProcessOutputRoute, validate_start_process, validate_start_process_routed,
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

fn ore_chain_energy_requirement(registries: &Registries, batch_mass: Mass) -> Energy {
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
    let undersize_weight = distribution
        .classes()
        .iter()
        .filter(|class| class.range().maximum_diameter() <= aperture)
        .map(|class| u64::from(class.weight()))
        .sum::<u64>();
    let total_weight = distribution.total_weight();
    let oversize_weight = total_weight
        .checked_sub(undersize_weight)
        .unwrap_or_else(|| unreachable!("screen undersize weight cannot exceed total weight"));
    let weighted_oversize = u128::from(batch_mass.milligrams()) * u128::from(oversize_weight);
    assert_eq!(
        weighted_oversize % u128::from(total_weight),
        0,
        "planned ore batch must preserve the authored screen partition at whole-milligram resolution"
    );
    let oversize_mass = Mass::from_milligrams(
        u64::try_from(weighted_oversize / u128::from(total_weight))
            .unwrap_or_else(|_| unreachable!("screened oversize mass fits u64")),
    );
    [
        calculate_mass_specific_energy(batch_mass, crusher.specific_energy()),
        calculate_mass_specific_energy(batch_mass, grinder.specific_energy()),
        calculate_mass_specific_energy(batch_mass, screening.specific_energy()),
        calculate_mass_specific_energy(oversize_mass, fine_grind.specific_energy()),
        calculate_mass_specific_energy(batch_mass, concentration.specific_energy()),
    ]
    .into_iter()
    .try_fold(Energy::ZERO, |total, energy| total.checked_add(energy))
    .unwrap_or_else(|| panic!("ore preparation chain energy requirement overflowed"))
}

pub(super) fn energy_fundable_batch_mass(
    registries: &Registries,
    offered: Mass,
    representable_unit_mg: u64,
    available: Energy,
) -> Mass {
    assert!(representable_unit_mg > 0);
    assert_eq!(offered.milligrams() % representable_unit_mg, 0);
    let offered_units = offered.milligrams() / representable_unit_mg;
    let mut admitted = 0_u64;
    let mut rejected = offered_units;
    while admitted < rejected {
        let candidate_units = admitted + (rejected - admitted).div_ceil(2);
        let candidate = Mass::from_milligrams(
            candidate_units
                .checked_mul(representable_unit_mg)
                .unwrap_or_else(|| panic!("planned ore batch mass overflowed")),
        );
        if ore_chain_energy_requirement(registries, candidate) <= available {
            admitted = candidate_units;
        } else {
            rejected = candidate_units - 1;
        }
    }
    Mass::from_milligrams(
        admitted
            .checked_mul(representable_unit_mg)
            .unwrap_or_else(|| panic!("planned ore batch mass overflowed")),
    )
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
    let required_energy = ore_chain_energy_requirement(registries, batch_mass);
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    assert!(
        drive_capacity >= required_energy,
        "authored industrial drive must remain capable of the maintained full-chain ore contract"
    );
    // Seed-only pressure makes exact replays independent of runner role. Fresh samples range from
    // materially under-provisioned to comfortably funded so the actor must size work to visible
    // stored energy instead of receiving a pre-funded success path.
    let energy_budget_ppm = 400_000 + (mix64(seed ^ 0x454E_4552_4759_4845) % 950_001) as u32;
    let varied_budget = Energy::from_nanojoules(
        required_energy
            .nanojoules()
            .checked_mul(u128::from(energy_budget_ppm))
            .map(|scaled| scaled / 1_000_000)
            .unwrap_or_else(|| panic!("ore preparation energy budget scaling overflowed")),
    );
    let drive_energy = std::cmp::min(varied_budget, drive_capacity);
    OrePreparationSetup {
        batch_mass,
        representable_unit_mg: representable_unit,
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
    Completed {
        offered_mass: Mass,
        processed_mass: Mass,
    },
    Stopped {
        stage: &'static str,
        reason: OreStopReason,
    },
}

#[derive(Clone, Copy)]
struct OreEnergyStop {
    stage: &'static str,
    available: Energy,
    requested: Energy,
}

fn report_ore_energy_stop(
    registries: &Registries,
    state: &AppState,
    ids: OrePreparationProbeIds,
    case: FocusedProbeCase,
    initial_matter: AggregateMass,
    stop: OreEnergyStop,
) -> OreProbeOutcome {
    let OreEnergyStop {
        stage,
        available,
        requested,
    } = stop;
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

struct OreProbeEpisode {
    case: FocusedProbeCase,
    state: AppState,
    ids: OrePreparationProbeIds,
    offered_batch_mass: Mass,
    batch_mass: Mass,
    initial_matter: AggregateMass,
    initial_energy: Energy,
    initial_crusher_condition: Condition,
    initial_grinder_condition: Condition,
    initial_screen_condition: Condition,
    initial_separator_condition: Condition,
    input_composition: MaterialComposition,
    input_copper_ppm: u32,
    input_stone_ppm: u32,
    input_clay_ppm: u32,
}

fn prepare_ore_probe(registries: &Registries, case: FocusedProbeCase) -> OreProbeEpisode {
    let seed = case.seed();
    let setup = probe_parameters(registries, seed);
    let offered_batch_mass = setup.batch_mass;
    let batch_mass = energy_fundable_batch_mass(
        registries,
        offered_batch_mass,
        setup.representable_unit_mg,
        setup.drive_energy,
    );
    assert!(
        !batch_mass.is_zero(),
        "ore preparation generated stored work below one screen-representable full-chain batch"
    );
    let initial_crusher_condition = setup.crusher_condition;
    let initial_grinder_condition = setup.grinder_condition;
    let initial_screen_condition = setup.screen_condition;
    let (state, ids) = setup_ore_preparation_probe(registries, seed, setup);
    let initial_separator_condition = state
        .equipment()
        .get_equipment(ids.separator)
        .map(|equipment| equipment.condition())
        .unwrap_or_else(|| panic!("ore preparation assembled separator disappeared"));
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

    OreProbeEpisode {
        case,
        state,
        ids,
        offered_batch_mass,
        batch_mass,
        initial_matter,
        initial_energy,
        initial_crusher_condition,
        initial_grinder_condition,
        initial_screen_condition,
        initial_separator_condition,
        input_composition,
        input_copper_ppm,
        input_stone_ppm,
        input_clay_ppm,
    }
}

#[derive(Clone, Copy)]
struct PoweredStageResult {
    energy: Energy,
    duration: TickSpan,
    condition_after: Condition,
}

struct ComminutionStageRequest<'a> {
    stage: &'static str,
    process: ProcessId,
    source: StockpileId,
    selections: &'a [MaterialLotSelection],
    equipment: EquipmentId,
    destination: StockpileId,
    activity: &'static str,
    failure_context: &'static str,
}

fn execute_comminution_stage(
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

fn assert_anchor_route_boundaries(
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
struct RegrindStageResult {
    powered: PoweredStageResult,
    fine_output_fits_undersize: bool,
    oversize_profile_is_preserved: bool,
}

fn execute_regrind_stage(
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

#[path = "ore_completion.rs"]
mod completion;
use completion::{OreCompletionEvidence, finalize_completed_ore_probe};

#[derive(Clone, Copy)]
struct ScreeningStageResult {
    powered: PoweredStageResult,
    undersize_mass: Mass,
    oversize_mass: Mass,
}

struct ScreeningStageRequest<'a> {
    source: StockpileId,
    selections: &'a [MaterialLotSelection],
    undersize_destination: StockpileId,
    oversize_destination: StockpileId,
}

fn execute_screening_stage(
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

fn execute_concentration_stage(
    registries: &Registries,
    episode: &mut OreProbeEpisode,
    selections: &[MaterialLotSelection],
) -> Result<PoweredStageResult, OreProbeOutcome> {
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
    let result = PoweredStageResult {
        energy: resolved.required_energy(),
        duration,
        condition_after: resolved.condition_after(),
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

pub(super) fn evaluate_ore_preparation_capability_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> OreProbeOutcome {
    let mut episode = prepare_ore_probe(registries, case);
    let ids = episode.ids;
    let batch_mass = episode.batch_mass;
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
    let crush_selection = [MaterialLotSelection::new(ids.ore_lot, batch_mass)];
    let crush = match execute_comminution_stage(
        registries,
        &mut episode,
        ComminutionStageRequest {
            stage: "crush",
            process: PROCESS_CRUSH_ORE,
            source: ids.ore_source,
            selections: &crush_selection,
            equipment: ids.crusher,
            destination: ids.crushed_storage,
            activity: "ore crushing",
            failure_context: "canonical crushing probe resolution failed",
        },
    ) {
        Ok(result) => result,
        Err(outcome) => return outcome,
    };
    let crushed_selection = select_stockpile_mass(
        &episode.state,
        ids.crushed_storage,
        batch_mass,
        "crushed ore output",
    );
    let crusher_output_matches_authoring = crushed_selection.iter().all(|selection| {
        episode
            .state
            .inventory()
            .get_lot(selection.lot())
            .and_then(|lot| lot.particle_size_distribution())
            == Some(crusher_definition.output_particle_size_distribution())
    });
    assert_anchor_route_boundaries(registries, &episode, crushed_selection.as_slice());

    let grind = match execute_comminution_stage(
        registries,
        &mut episode,
        ComminutionStageRequest {
            stage: "grind",
            process: PROCESS_GRIND_CRUSHED_ORE,
            source: ids.crushed_storage,
            selections: crushed_selection.as_slice(),
            equipment: ids.grinder,
            destination: ids.ground_storage,
            activity: "ore grinding",
            failure_context: "canonical grinding probe resolution failed",
        },
    ) {
        Ok(result) => result,
        Err(outcome) => return outcome,
    };
    let grinder_condition = grind.condition_after;
    assert_eq!(
        episode
            .state
            .equipment()
            .get_equipment(ids.grinder)
            .map(|equipment| equipment.condition()),
        Some(grinder_condition),
        "grinder condition must match the resolved wear projection"
    );

    let ground_selection = select_stockpile_mass(
        &episode.state,
        ids.ground_storage,
        batch_mass,
        "ground ore output",
    );
    let grinding_matches_authoring = ground_selection.iter().all(|selection| {
        episode
            .state
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

    let screen = match execute_screening_stage(
        registries,
        &mut episode,
        ScreeningStageRequest {
            source: ids.ground_storage,
            selections: ground_selection.as_slice(),
            undersize_destination: ids.undersize_storage,
            oversize_destination: ids.oversize_storage,
        },
    ) {
        Ok(result) => result,
        Err(outcome) => return outcome,
    };
    let screened_oversize_mass = screen.oversize_mass;

    let regrind = match execute_regrind_stage(
        registries,
        &mut episode,
        screened_oversize_mass,
        grinder_condition,
    ) {
        Ok(result) => result,
        Err(outcome) => return outcome,
    };
    let selection = select_stockpile_mass(
        &episode.state,
        ids.undersize_storage,
        batch_mass,
        "full fine liberated feed for industrial copper concentration",
    );
    let concentration =
        match execute_concentration_stage(registries, &mut episode, selection.as_slice()) {
            Ok(result) => result,
            Err(outcome) => return outcome,
        };
    finalize_completed_ore_probe(
        registries,
        &episode,
        OreCompletionEvidence {
            crush,
            grind,
            screen,
            regrind,
            concentration,
            crusher_output_matches_authoring,
            grinding_matches_authoring,
            grinding_resolved_screen_cut,
        },
    )
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
                OreProbeOutcome::Completed {
                    offered_mass,
                    processed_mass,
                } if processed_mass < offered_mass
            ),
            "ore coverage seed 2 must preserve visible finite-work pressure through adaptive batching"
        );
    }
}
