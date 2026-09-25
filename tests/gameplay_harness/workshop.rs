//! Owns the industrial workshop actor, controlled-event runtime, and scenario execution.

use std::env;

use super::capability_boundary::{
    seed_capability_only_energy_store, seed_capability_only_equipment,
};
use super::configuration::{
    MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT, ScenarioPlanMode, scenario_seeds_from,
};
use super::contracts::{assert_anchor_diversity, assert_scenario_contracts};
use super::environment::ROOM_TEMPERATURE;
#[cfg(not(test))]
use super::fresh_seed::fresh_root;
use super::industrial_support::install_equipment_on_grounded_support;
use super::inventory_support::add_solid_stockpile;
use super::maintenance_timing::{
    advance_equipment_maintenance_to, finish_active_equipment_maintenance,
};
use super::manual_power_timing::{advance_manual_power_to, finish_manual_power_work};
use super::ore_fixture::copper_ore_composition;
#[cfg(not(test))]
use super::output::has_verbose_output;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioChoiceReport,
    ScenarioPolicyVariation, ScenarioProgressReport, ScenarioReport, ScenarioResourceReport,
    ScenarioStructureReport, StructuralPreference,
};
#[cfg(not(test))]
use super::report::{print_content_summary, print_harness_summary};
use super::scenario::{ScenarioDeliveryVariation, ScenarioVariation, WORKSHOP_SUPPORT_LENGTH};
use super::seed::mix64;
use super::temporal::advance_idle_ticks;
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use deep_hearth::content::gameplay_fixture::{
    ControlledMaterialDelivery, authorize_controlled_material_delivery,
    commit_controlled_material_delivery, seed_composed_lot, seed_grounded_active_structure,
    seed_lot, seed_player_survival_at_hydration_warning_boundary,
};
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_JAW_CRUSHER,
    FORM_CRUSHED, FORM_LOG, FORM_ORE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_WOOD,
    PROCESS_CRUSH_ORE, PROCESS_MELT_PURE_COPPER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    build_registries,
};
use deep_hearth::core::quantity::{Area, Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::energy::{
    EnergySinkError, EnergyStoreId, EnergySupplyError, calculate_mass_specific_energy,
};
use deep_hearth::equipment::{
    EquipmentId, EquipmentProviderError, EquipmentSupportError, validate_assemble_equipment,
    validate_mount_equipment, validate_relocate_equipment,
};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, validate_mount_stockpile,
};
use deep_hearth::labor::{
    ManualPowerDestinationTargetAssessment, ManualPowerDestinationTargetBlocker,
    ManualPowerDestinationTargetRequest, ManualPowerEnergyEnvelopeRequest, ManualPowerError,
    ManualPowerRequest, PlayerWork, ValidatedManualPowerStart,
    assess_manual_power_destination_target, assess_manual_power_energy_envelope,
    validate_start_manual_power,
};
use deep_hearth::maintenance::{Condition, MaintenanceBand};
use deep_hearth::material::{COMPOSITION_PARTS_PER_MILLION, CommodityKey};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionRequest, ComminutionResolutionError, PoweredOreBottleneck, PoweredOreMassConstraint,
    PoweredOreMassEnvelope, PoweredOreReplenishmentConstraint, ResolvedComminution,
    assess_powered_ore_mass_envelope, resolve_comminution_process,
};
use deep_hearth::production::{
    ProductionAvailabilityChange, ProductionJobId, ProductionSuspensionReason,
    validate_start_process,
};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::structural::{
    StructuralAssessment, StructuralElementGeometry, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructuralStage, analyze_structure,
};
use deep_hearth::survival::{assess_survival, initialize_player_survival};
use deep_hearth::thermal::{
    MeltingBatchError, MeltingRequest, MeltingResolutionError, resolve_melting_process,
};

#[path = "workshop/crush_planning.rs"]
pub(super) mod crush_planning;
use crush_planning::*;
#[path = "workshop/manual_recovery.rs"]
mod manual_recovery;
use manual_recovery::*;
#[path = "workshop/maintenance.rs"]
mod maintenance;
use maintenance::{MaintenanceAttempt, service_crusher};
#[path = "workshop/structure.rs"]
mod structure;
use structure::*;

/// Smallest nonzero matter quantity that an explicit lot selection can represent.
///
/// Production capability requirements describe provider discovery and must not be reinterpreted as
/// operation-level minimum batch sizes. Powered ore resolution accepts any nonzero selected mass up
/// to the provider's condition-adjusted batch ceiling.
const MINIMUM_SELECTABLE_MASS: Mass = Mass::from_milligrams(1);

fn advance_running_production_to_tick(
    registries: &Registries,
    state: &mut AppState,
    job: ProductionJobId,
    target_tick: u64,
    context: &'static str,
) {
    let record = state.production().get_job(job).unwrap_or_else(|| {
        panic!("gameplay harness {context} job disappeared before bounded advance")
    });
    assert!(
        target_tick > state.tick().value() && target_tick < record.completes_at().value(),
        "gameplay harness {context} bounded advance must stop strictly before scheduled completion"
    );
    while state.tick().value() < target_tick {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[job],
                ..TickEventAllowance::default()
            },
            context,
        );
        assert!(
            outcome.production_completions().is_empty(),
            "gameplay harness {context} production completed before the declared world event"
        );
    }
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(|record| !record.is_suspended()),
        "gameplay harness {context} job must remain active at the bounded observation tick"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PowerChoiceBasis {
    Policy,
    SingleSource,
}

/// Stable identities the workshop actor may use while planning and executing actions.
///
/// Controlled-delivery source/destination identities are deliberately absent. The environment owns
/// them only through the opaque transfer authorization until the event occurs.
#[derive(Clone, Copy)]
struct WorkshopIds {
    ore_source: StockpileId,
    crushed_storage: StockpileId,
    maintenance_source: StockpileId,
    maintenance_spent: StockpileId,
    ore_lot: MaterialLotId,
    crusher: EquipmentId,
    hand_crank: EquipmentId,
    furnace: EquipmentId,
    small_drive: EnergyStoreId,
    large_drive: EnergyStoreId,
    electrical_buffer: EnergyStoreId,
    compact_support: StructuralElementId,
    reinforced_support: StructuralElementId,
}

fn assemble_workshop_hand_crank(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK)
        .unwrap_or_else(|| panic!("canonical reinforced hand crank disappeared"));
    let profile = definition
        .assembly_profile()
        .unwrap_or_else(|| panic!("canonical reinforced hand crank lost its assembly profile"));
    let capacity = profile
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("workshop hand-crank material capacity overflowed"));
    let source = add_solid_stockpile(state, capacity);
    for input in profile.inputs() {
        seed_lot(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_assemble_equipment(
        registries,
        state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        source,
    )
    .unwrap_or_else(|error| panic!("workshop hand-crank assembly failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("workshop hand-crank assembly commit failed: {error}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrushBatchOutcome {
    bottleneck: PoweredOreBottleneck,
    completed: bool,
}

struct CrushBatchExecution {
    mass: Mass,
    option: CrushOption,
    batch_index: u16,
}

/// Observable inputs available to the workshop actor while choosing actions.
///
/// Hidden controller state such as the future delivery tick is deliberately absent so decision code
/// cannot accidentally inspect information that a player would not have.
struct ScenarioActorRuntime<'state> {
    policy: ScenarioPolicyVariation,
    nominal_batch_mass: Mass,
    current_support: &'state mut StructuralElementId,
    alternate_support: &'state mut StructuralElementId,
    report: ScenarioActorReport<'state>,
}

struct ScenarioActorReport<'state> {
    structure: &'state mut ScenarioStructureReport,
    choices: &'state mut ScenarioChoiceReport,
    progress: &'state mut ScenarioProgressReport,
    resources: &'state mut ScenarioResourceReport,
}

impl<'state> ScenarioActorRuntime<'state> {
    fn new(
        policy: ScenarioPolicyVariation,
        nominal_batch_mass: Mass,
        current_support: &'state mut StructuralElementId,
        alternate_support: &'state mut StructuralElementId,
        report: ScenarioActorReport<'state>,
    ) -> Self {
        Self {
            policy,
            nominal_batch_mass,
            current_support,
            alternate_support,
            report,
        }
    }
}

/// Scenario-controller state that owns all future controlled-event facts hidden from the actor.
struct ControlledDeliveryRuntime<'state> {
    delivery: ScenarioDeliveryVariation,
    authorization: &'state mut Option<ControlledMaterialDelivery>,
}

fn stored_work_from_nominal_batches(
    batch_energy: Energy,
    full_batches: u8,
    partial_batch_ppm: u32,
) -> Energy {
    let full = batch_energy
        .nanojoules()
        .checked_mul(u128::from(full_batches))
        .unwrap_or_else(|| panic!("gameplay stored-work full-batch scaling overflowed"));
    let partial = batch_energy
        .nanojoules()
        .checked_mul(u128::from(partial_batch_ppm))
        .map(|scaled| scaled / u128::from(COMPOSITION_PARTS_PER_MILLION))
        .unwrap_or_else(|| panic!("gameplay stored-work partial-batch scaling overflowed"));
    Energy::from_nanojoules(
        full.checked_add(partial)
            .unwrap_or_else(|| panic!("gameplay stored-work total overflowed")),
    )
}

fn bounds(x: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(value) => value,
        Err(error) => panic!("gameplay harness bounds failed: {error}"),
    }
}

fn active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
    cross_section: Area,
) -> StructuralElementId {
    let geometry =
        StructuralElementGeometry::new(bounds(x), WORKSHOP_SUPPORT_LENGTH, cross_section)
            .unwrap_or_else(|error| panic!("gameplay harness support geometry failed: {error}"));
    seed_grounded_active_structure(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        FORM_LOG,
    )
}

fn seed_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
    fraction_divisor: u128,
) -> EnergyStoreId {
    let authored = match registries.energy().get_store(definition) {
        Some(authored) => authored,
        None => panic!(
            "canonical energy definition {} is missing",
            definition.value()
        ),
    };
    let amount = Energy::from_nanojoules(authored.capacity().nanojoules() / fraction_divisor);
    seed_capability_only_energy_store(registries, state, definition, amount)
}

fn setup_workshop(
    registries: &Registries,
    variation: ScenarioVariation,
) -> (AppState, WorkshopIds, Option<ControlledMaterialDelivery>) {
    let mut state = AppState::new();
    let ore_mass = variation.ore.order_mass;
    let ore_source = add_solid_stockpile(&mut state, ore_mass);
    let crushed_storage = add_solid_stockpile(&mut state, ore_mass);
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let replacement_unit = maintenance_profile.full_service_replacement_mass();
    let replacement_total_milligrams = replacement_unit
        .milligrams()
        .checked_mul(u64::from(variation.crusher.maintenance_replacement_units))
        .unwrap_or_else(|| panic!("gameplay harness maintenance stock overflowed"));
    let replacement_total = Mass::from_milligrams(replacement_total_milligrams);
    let maintenance_capacity =
        Mass::from_milligrams(replacement_total_milligrams.max(replacement_unit.milligrams()));
    let maintenance_source = add_solid_stockpile(&mut state, maintenance_capacity);
    let maintenance_spent = add_solid_stockpile(&mut state, maintenance_capacity);

    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        ore_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(
            variation.ore.ore_copper_ppm,
            variation.ore.gangue_clay_share_ppm,
        ),
    );
    if !replacement_total.is_zero() {
        seed_lot(
            registries,
            &mut state,
            maintenance_source,
            maintenance_profile.replacement(),
            replacement_total,
            ROOM_TEMPERATURE,
        );
    }

    let crusher = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        variation.crusher.initial_crusher_condition,
    );
    let hand_crank = assemble_workshop_hand_crank(registries, &mut state);
    let furnace = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    );
    install_equipment_on_grounded_support(registries, &mut state, furnace, 6);
    let electrical_buffer = seed_energy_store(registries, &mut state, ENERGY_ELECTRICAL_BUFFER, 2);

    let compact_support = active_support(
        registries,
        &mut state,
        0,
        variation.structure.compact_support_area,
    );
    let reinforced_support = active_support(
        registries,
        &mut state,
        2,
        variation.structure.reinforced_support_area,
    );
    let background_storage =
        add_solid_stockpile(&mut state, variation.structure.reinforced_background_mass);
    seed_lot(
        registries,
        &mut state,
        background_storage,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        variation.structure.reinforced_background_mass,
        ROOM_TEMPERATURE,
    );
    let _ = validate_mount_stockpile(registries, &state, background_storage, reinforced_support)
        .unwrap_or_else(|error| panic!("gameplay harness background storage mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness background storage commit failed: {error}")
        });

    let delivery_source = add_solid_stockpile(&mut state, variation.delivery.mass);
    let delivery_destination = add_solid_stockpile(&mut state, variation.delivery.mass);
    seed_lot(
        registries,
        &mut state,
        delivery_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        variation.delivery.mass,
        ROOM_TEMPERATURE,
    );
    let delivery_support = if variation.delivery.destination_is_compact {
        compact_support
    } else {
        reinforced_support
    };
    let _ = validate_mount_stockpile(registries, &state, delivery_destination, delivery_support)
        .unwrap_or_else(|error| panic!("gameplay harness delivery storage mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("gameplay harness delivery storage commit failed: {error}"));

    let comminution = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
    let batch_energy = calculate_mass_specific_energy(
        variation.ore.nominal_batch_mass,
        comminution.specific_energy(),
    );
    let small_drive_energy = std::cmp::min(
        stored_work_from_nominal_batches(
            batch_energy,
            variation.crusher.small_drive_batch_budget,
            variation.crusher.small_drive_partial_batch_ppm,
        ),
        registries
            .energy()
            .get_store(ENERGY_MECHANICAL_SMALL_DRIVE)
            .map(|definition| definition.capacity())
            .unwrap_or_else(|| panic!("canonical small-drive definition disappeared")),
    );
    let small_drive = seed_capability_only_energy_store(
        registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        small_drive_energy,
    );
    let large_drive_energy = std::cmp::min(
        stored_work_from_nominal_batches(
            batch_energy,
            variation.crusher.large_drive_batch_budget,
            variation.crusher.large_drive_partial_batch_ppm,
        ),
        registries
            .energy()
            .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
            .map(|definition| definition.capacity())
            .unwrap_or_else(|| panic!("canonical large-drive definition disappeared")),
    );
    let large_drive = seed_capability_only_energy_store(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        large_drive_energy,
    );

    let delivery_authorization = Some(authorize_controlled_material_delivery(
        registries,
        &state,
        delivery_source,
        delivery_destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        variation.delivery.mass,
    ));
    if variation.survival.start_at_hydration_warning_boundary {
        seed_player_survival_at_hydration_warning_boundary(registries, &mut state);
    } else {
        initialize_player_survival(registries, &mut state)
            .unwrap_or_else(|error| panic!("workshop survival initialization failed: {error}"));
    }

    (
        state,
        WorkshopIds {
            ore_source,
            crushed_storage,
            maintenance_source,
            maintenance_spent,
            ore_lot,
            crusher,
            hand_crank,
            furnace,
            small_drive,
            large_drive,
            electrical_buffer,
            compact_support,
            reinforced_support,
        },
        delivery_authorization,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobAdvanceOutcome {
    Completed,
    Suspended,
}

fn advance_job_until_completion_or_suspension(
    registries: &Registries,
    state: &mut AppState,
    job: ProductionJobId,
) -> JobAdvanceOutcome {
    let Some(record) = state.production().get_job(job) else {
        return JobAdvanceOutcome::Completed;
    };
    if record.is_suspended() {
        return JobAdvanceOutcome::Suspended;
    }
    let scheduled_completion = record.completes_at();
    let remaining_ticks = scheduled_completion
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| {
            panic!(
                "active gameplay harness production job {} is scheduled in the past",
                job.value()
            )
        });
    for _ in 0..remaining_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness job tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[job],
                production_availability_changes: true,
                ..TickEventAllowance::default()
            },
            "workshop production job",
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == job)
        {
            return JobAdvanceOutcome::Completed;
        }
        if outcome
            .production_availability_changes()
            .iter()
            .any(|change| {
                matches!(
                    change,
                    ProductionAvailabilityChange::Suspended {
                        job: changed_job,
                        reason: _reason,
                        suspended_at: _suspended_at,
                        remaining_active_time: _remaining_active_time,
                    } if *changed_job == job
                )
            })
        {
            return JobAdvanceOutcome::Suspended;
        }
    }
    match state.production().get_job(job) {
        None => JobAdvanceOutcome::Completed,
        Some(record) if record.is_suspended() => JobAdvanceOutcome::Suspended,
        Some(record) => panic!(
            "active gameplay harness production job {} remained scheduled at {} after reaching bounded due tick {}",
            job.value(),
            record.completes_at().value(),
            scheduled_completion.value()
        ),
    }
}

fn crush_batch(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    execution: CrushBatchExecution,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) -> CrushBatchOutcome {
    let CrushBatchExecution {
        mass,
        option,
        batch_index,
    } = execution;
    let available_power_microwatts = option
        .resolved
        .available_power()
        .whole_microwatts()
        .unwrap_or_else(|| {
            panic!("workshop crushing power must be an exact whole-microwatt value")
        });
    println!(
        "  crush#{batch_index}: drive={} mass={}mg rate={}mg/s power={}uW duration={}t constraints=[throughput:{}t energy:{}t] condition={}ppm->{}ppm bottleneck={:?}",
        option.name,
        mass.milligrams(),
        option.resolved.processing_rate().milligrams_per_second(),
        available_power_microwatts,
        option.resolved.process_resolution().duration().value(),
        option.resolved.throughput_duration().value(),
        option.resolved.energy_duration().value(),
        option.resolved.condition_before().parts_per_million(),
        option.resolved.condition_after().parts_per_million(),
        option.resolved.bottleneck(),
    );
    let bottleneck = option.resolved.bottleneck();
    let start = validate_start_process(
        registries,
        state,
        option.resolved.process_resolution(),
        ids.ore_source,
        ids.crushed_storage,
    )
    .unwrap_or_else(|error| panic!("gameplay harness crushing start failed: {error}"));
    let job = start
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay harness crushing commit failed: {error}"));
    let admitted_job = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("gameplay harness admitted crushing job disappeared"));
    let started_at = admitted_job.started_at().value();
    let completes_at = admitted_job.completes_at().value();
    if !actor.report.progress.delivery_applied
        && started_at < controller.delivery.delivery_at_tick
        && controller.delivery.delivery_at_tick < completes_at
    {
        advance_running_production_to_tick(
            registries,
            state,
            job,
            controller.delivery.delivery_at_tick,
            "workshop crushing before controlled event",
        );
        let assessment = apply_delivery(registries, state, ids, controller, actor);
        if assessment.stage() == StructuralStage::Failed {
            let outcome = advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("gameplay harness suspension tick failed: {error}"));
            assert_tick_events_within(
                &outcome,
                TickEventAllowance {
                    production_jobs: &[job],
                    production_availability_changes: true,
                    ..TickEventAllowance::default()
                },
                "workshop suspension tick",
            );
            assert!(
                outcome.production_completions().is_empty(),
                "gameplay harness suspension tick unexpectedly completed the suspended job"
            );
            let suspension = outcome
                .production_availability_changes()
                .iter()
                .find_map(|change| match *change {
                    ProductionAvailabilityChange::Suspended {
                        job: changed_job,
                        reason,
                        suspended_at: _suspended_at,
                        remaining_active_time,
                    } if changed_job == job => Some((reason, remaining_active_time)),
                    ProductionAvailabilityChange::Suspended { .. }
                    | ProductionAvailabilityChange::SuspensionReasonChanged { .. } => None,
                    ProductionAvailabilityChange::Resumed {
                        job: _job,
                        reason: _reason,
                        resumed_at: _resumed_at,
                        scheduled_completion: _scheduled_completion,
                    } => None,
                })
                .unwrap_or_else(|| {
                    panic!("failed crusher support did not suspend its in-flight production job")
                });
            assert_eq!(
                suspension.0,
                ProductionSuspensionReason::EquipmentSupportUnavailable {
                    equipment: ids.crusher,
                }
            );
            actor.report.structure.production_suspension = true;
            println!(
                "  interruption: crush#{batch_index} suspends with {} active tick(s) remaining; consumed matter and work stay owned as work-in-process",
                suspension.1.value()
            );
            adapt_after_delivery(registries, state, ids, actor, assessment);
            if actor.report.structure.structural_stop {
                actor.report.structure.stranded_work_in_process = true;
                println!(
                    "  work-in-process: crush#{batch_index} remains suspended; no output or final condition outcome is committed while structural recovery is unavailable"
                );
                return CrushBatchOutcome {
                    bottleneck,
                    completed: false,
                };
            }

            let outcome = advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("gameplay harness resume tick failed: {error}"));
            assert_tick_events_within(
                &outcome,
                TickEventAllowance {
                    production_jobs: &[job],
                    production_availability_changes: true,
                    ..TickEventAllowance::default()
                },
                "workshop resume tick",
            );
            let resumed = outcome
                .production_availability_changes()
                .iter()
                .any(|change| {
                    matches!(
                        change,
                        ProductionAvailabilityChange::Resumed {
                            job: changed_job,
                            reason: ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
                            resumed_at: _resumed_at,
                            scheduled_completion: _scheduled_completion,
                        } if *changed_job == job && *equipment == ids.crusher
                    )
                });
            assert!(
                resumed,
                "relocated crusher job did not resume on the next canonical tick"
            );
            let completed_on_resume_tick = outcome
                .production_completions()
                .iter()
                .any(|completion| completion.job() == job);
            if completed_on_resume_tick {
                println!(
                    "  recovery: suspended crush#{batch_index} resumes and completes on the same canonical tick because one active tick remained"
                );
            } else {
                println!(
                    "  recovery: suspended crush#{batch_index} resumes with its original work-in-process and remaining active time"
                );
            }
            assert_eq!(
                advance_job_until_completion_or_suspension(registries, state, job),
                JobAdvanceOutcome::Completed,
                "recovered crusher job suspended again without another structural mutation"
            );
        } else {
            let completed = advance_job_until_completion_or_suspension(registries, state, job);
            assert_eq!(
                completed,
                JobAdvanceOutcome::Completed,
                "active support unexpectedly suspended crusher production"
            );
            if assessment.stage() != StructuralStage::Stable {
                adapt_after_delivery(registries, state, ids, actor, assessment);
            }
        }
    } else {
        assert_eq!(
            advance_job_until_completion_or_suspension(registries, state, job),
            JobAdvanceOutcome::Completed,
            "crusher production suspended before the controlled delivery changed support state"
        );
    }
    CrushBatchOutcome {
        bottleneck,
        completed: true,
    }
}

#[path = "workshop/finalize.rs"]
mod finalize;
#[path = "workshop/runner.rs"]
pub(super) mod runner;

pub(super) fn run_gameplay_harness(mode: ScenarioPlanMode) {
    runner::run_gameplay_harness(mode);
}
