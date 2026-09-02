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
use deep_hearth::energy::EnergySupplyError;
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

#[path = "ore_probe_generation.rs"]
pub(super) mod generation;
use generation::{OreProbeEpisode, prepare_ore_probe};

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

#[path = "ore_probe_stages.rs"]
mod stages;
use stages::{
    ComminutionStageRequest, PoweredStageResult, RegrindStageResult, ScreeningStageRequest,
    ScreeningStageResult, assert_anchor_route_boundaries, execute_comminution_stage,
    execute_concentration_stage, execute_regrind_stage, execute_screening_stage,
};

#[path = "ore_completion.rs"]
mod completion;
use completion::{OreCompletionEvidence, finalize_completed_ore_probe};

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
        assert_eq!(
            outcome,
            OreProbeOutcome::Stopped {
                stage: "grind",
                reason: OreStopReason::FiniteEnergy,
            },
            "ore coverage seed 2 must preserve a canonical mid-chain finite-work blocker"
        );
    }
}
