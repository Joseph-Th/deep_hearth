//! Industrial workshop actor, controlled-event runtime, and maintained scenario execution.

use std::env;

use super::capability_boundary::{
    seed_capability_only_energy_store, seed_capability_only_equipment,
};
use super::configuration::{
    MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT, ScenarioPlanMode, scenario_seeds_from,
};
use super::contracts::{assert_anchor_diversity, assert_scenario_contracts};
use super::fresh_seed::fresh_root;
use super::has_verbose_output;
use super::industrial_support::install_equipment_on_grounded_support;
use super::ore_fixture::copper_ore_composition;
use super::report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioChoiceReport,
    ScenarioPolicyVariation, ScenarioProgressReport, ScenarioReport, ScenarioResourceReport,
    ScenarioStructureReport, StructuralPreference, print_content_summary, print_harness_summary,
};
use super::scenario::{ScenarioDeliveryVariation, ScenarioVariation, WORKSHOP_SUPPORT_LENGTH};
use super::seed::mix64;
use super::support::{ROOM_TEMPERATURE, add_solid_stockpile};
use deep_hearth::content::gameplay_fixture::{
    authorize_controlled_material_delivery, materialize_structure, seed_composed_lot, seed_lot,
    seed_player_survival_at_hydration_warning,
};
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_JAW_CRUSHER,
    FORM_CRUSHED, FORM_INGOT, FORM_LOG, FORM_ORE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_WOOD, PROCESS_CRUSH_ORE, PROCESS_MELT_PURE_COPPER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use deep_hearth::core::quantity::{Area, Energy, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{TickSpan, WorldSeed};
use deep_hearth::energy::{
    EnergySinkError, EnergyStoreId, EnergySupplyError, calculate_mass_specific_energy,
};
use deep_hearth::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, EquipmentMaintenanceResolutionError,
    EquipmentProviderError, EquipmentSupportError, resolve_equipment_maintenance,
    validate_assemble_equipment, validate_equipment_maintenance, validate_mount_equipment,
    validate_relocate_equipment,
};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialTransferResolution, StockpileId,
    validate_material_transfer, validate_mount_stockpile,
};
use deep_hearth::labor::{
    ManualPowerError, ManualPowerRequest, PlayerWorkStartError, ValidatedManualPowerStart,
    validate_start_manual_power,
};
use deep_hearth::maintenance::{Condition, MaintenanceBand};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionRequest, ComminutionResolutionError, PoweredOreBottleneck, ResolvedComminution,
    resolve_comminution_process,
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
    StructuralLoadKind, StructuralStage, add_structural_element, analyze_structure,
    validate_activate_structural_element,
};
use deep_hearth::survival::{assess_survival, initialize_player_survival};
use deep_hearth::thermal::{
    MeltingBatchError, MeltingRequest, MeltingResolutionError, resolve_melting_process,
};

#[path = "workshop/crush_planning.rs"]
mod crush_planning;
use crush_planning::*;

struct ManualRecoveryProbe {
    option: Option<ManualRecoveryOption>,
    survival_limited: bool,
    policy_declined: bool,
    equipment_limited: bool,
    storage_limited: bool,
}

enum ManualRecoverySearch {
    Available {
        mass: Mass,
        option: Box<ManualRecoveryOption>,
        adaptive_constraint: Option<ManualRecoveryConstraint>,
    },
    DeclinedForSurvival,
    SurvivalLimited,
    EquipmentLimited,
    StorageLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManualRecoveryConstraint {
    SurvivalPolicy,
    SurvivalReserve,
    EquipmentCondition,
    StorageCapacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PowerChoiceBasis {
    Policy,
    SingleSource,
}

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
    delivery_support: StructuralElementId,
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
        report: &'state mut ScenarioReport,
    ) -> Self {
        Self {
            policy,
            nominal_batch_mass,
            current_support,
            alternate_support,
            report: ScenarioActorReport {
                structure: &mut report.structure,
                choices: &mut report.choices,
                progress: &mut report.progress,
                resources: &mut report.resources,
            },
        }
    }
}

/// Scenario-controller state that may inject the preauthorized hidden world event.
struct ControlledDeliveryRuntime<'state> {
    delivery: ScenarioDeliveryVariation,
    authorization: &'state mut Option<MaterialTransferResolution>,
}

fn seed_capability_store_exact(
    registries: &Registries,
    state: &mut AppState,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    seed_capability_only_energy_store(registries, state, definition, amount)
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
        .map(|scaled| scaled / 1_000_000)
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
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("gameplay harness support allocation failed: {error}"),
    };
    materialize_structure(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(activation) => activation,
        Err(error) => panic!("gameplay harness support activation failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("gameplay harness support activation commit failed: {error}");
    }
    element
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
    seed_capability_store_exact(registries, state, definition, amount)
}

fn setup_workshop(
    registries: &Registries,
    variation: ScenarioVariation,
) -> (AppState, WorkshopIds, Option<MaterialTransferResolution>) {
    let mut state = AppState::new(WorldSeed::new(variation.world_seed));
    if variation.survival.start_at_hydration_warning {
        seed_player_survival_at_hydration_warning(registries, &mut state);
    } else {
        initialize_player_survival(registries, &mut state)
            .unwrap_or_else(|error| panic!("workshop survival initialization failed: {error}"));
    }
    let ore_mass = variation.ore.order_mass;
    let ore_source = add_solid_stockpile(&mut state, ore_mass);
    let crushed_storage = add_solid_stockpile(&mut state, ore_mass);
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let replacement_unit = maintenance_profile.replacement_mass();
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
    validate_mount_stockpile(registries, &state, background_storage, reinforced_support)
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
    validate_mount_stockpile(registries, &state, delivery_destination, delivery_support)
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
    let small_drive_energy = stored_work_from_nominal_batches(
        batch_energy,
        variation.crusher.small_drive_batch_budget,
        variation.crusher.small_drive_partial_batch_ppm,
    );
    let small_drive = seed_capability_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        small_drive_energy,
    );
    let large_drive_energy = stored_work_from_nominal_batches(
        batch_energy,
        variation.crusher.large_drive_batch_budget,
        variation.crusher.large_drive_partial_batch_ppm,
    );
    let large_drive = seed_capability_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        large_drive_energy,
    );

    let delivery_authorization = Some(authorize_controlled_material_delivery(
        delivery_source,
        delivery_destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        variation.delivery.mass,
    ));

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
            delivery_support,
            compact_support,
            reinforced_support,
        },
        delivery_authorization,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaintenanceAttempt {
    Serviced,
    SupplyExhausted,
}

fn service_crusher(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    report: &mut ScenarioReport,
) -> MaintenanceAttempt {
    let resolution = match resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(
            ids.crusher,
            ids.maintenance_source,
            ids.maintenance_spent,
        ),
    ) {
        Ok(resolution) => resolution,
        Err(EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
            stockpile: _stockpile,
            commodity: _commodity,
            available,
            required,
        }) => {
            report.maintenance.supply_exhausted = true;
            println!(
                "  maintenance supply: service needs {}mg replacement stock but only {}mg remains",
                required.milligrams(),
                available.milligrams(),
            );
            return MaintenanceAttempt::SupplyExhausted;
        }
        Err(error) => panic!("gameplay harness maintenance resolution failed: {error}"),
    };
    let before = resolution.condition_before();
    let after = resolution.condition_after();
    let material_mass = resolution.material_mass();
    let spent_commodity = resolution.spent_commodity();
    let spent_form = registries
        .materials()
        .get_form(spent_commodity.form())
        .map(|form| form.name())
        .unwrap_or_else(|| panic!("gameplay harness maintenance spent form disappeared"));
    let outcome = validate_equipment_maintenance(registries, state, resolution)
        .unwrap_or_else(|error| panic!("gameplay harness maintenance validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay harness maintenance commit failed: {error}"));
    assert_eq!(outcome.condition_before(), before);
    assert_eq!(outcome.condition_after(), after);
    assert_eq!(outcome.material_mass(), material_mass);
    report.maintenance.services = report
        .maintenance
        .services
        .checked_add(1)
        .unwrap_or_else(|| panic!("gameplay harness maintenance service count overflowed"));
    report.maintenance.replacement_spent = report
        .maintenance
        .replacement_spent
        .checked_add(material_mass)
        .unwrap_or_else(|| panic!("gameplay harness maintenance material accounting overflowed"));
    assert!(
        state
            .inventory()
            .get_stockpile(ids.maintenance_spent)
            .is_some_and(|stockpile| stockpile.get_mass(spent_commodity) >= material_mass),
        "gameplay maintenance must preserve spent matter in its authored non-reusable form"
    );
    println!(
        "  maintenance service: spend={}mg replacement stock condition={}ppm->{}ppm; output becomes {spent_form} and is no longer replacement stock",
        material_mass.milligrams(),
        before.parts_per_million(),
        after.parts_per_million(),
    );
    MaintenanceAttempt::Serviced
}

fn finish_operation(registries: &Registries, state: &mut AppState, duration: TickSpan) {
    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(registries, state) {
            panic!("gameplay harness tick failed: {error}");
        }
    }
}

fn stage_rank(stage: StructuralStage) -> u8 {
    match stage {
        StructuralStage::Stable => 0,
        StructuralStage::Strained => 1,
        StructuralStage::Cracking => 2,
        StructuralStage::Failed => 3,
    }
}

struct ManualRecoveryOption {
    name: &'static str,
    store: EnergyStoreId,
    energy: Energy,
    start: ValidatedManualPowerStart,
}

fn manual_recovery_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    name: &'static str,
    store: EnergyStoreId,
) -> Result<Option<ManualRecoveryOption>, ManualPowerError> {
    let comminution = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
    let required = calculate_mass_specific_energy(mass, comminution.specific_energy());
    let stored = state
        .energy()
        .get_store(store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("gameplay harness {name} drive disappeared"));
    let Some(energy) = required.checked_sub(stored) else {
        return Ok(None);
    };
    if energy.is_zero() {
        return Ok(None);
    }
    validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, ids.hand_crank, store, energy),
    )
    .map(|start| {
        Some(ManualRecoveryOption {
            name,
            store,
            energy,
            start,
        })
    })
}

fn execute_manual_recovery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    option: ManualRecoveryOption,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) {
    let budget = option.start.resource_budget();
    let work = option.start.work();
    let started_at = work.started_at().value();
    let completes_at = work.completes_at().value();
    let duration = completes_at - started_at;
    let stored_before = state
        .energy()
        .get_store(option.store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("manual-recovery {} drive disappeared", option.name));
    let survival_before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("workshop survival state disappeared before manual recovery"));
    println!(
        "  manual recovery: crank {}nJ into {} drive over {}t; projected body cost={}nJ/{}uL, reserves={}nJ/{}uL",
        option.energy.nanojoules(),
        option.name,
        duration,
        budget.metabolic_energy().nanojoules(),
        budget.hydration().microliters(),
        survival_before.metabolic_energy().nanojoules(),
        survival_before.hydration().microliters(),
    );
    option
        .start
        .commit(state)
        .unwrap_or_else(|error| panic!("manual-recovery start commit failed: {error}"));

    let event_tick = controller.delivery.delivery_at_tick;
    let mut event_assessment = None;
    if !actor.report.progress.delivery_applied
        && event_tick > started_at
        && event_tick < completes_at
    {
        finish_operation(
            registries,
            state,
            TickSpan::new(event_tick - state.tick().value()),
        );
        println!(
            "  interruption: controlled world event occurs during manual charging; structural response waits until the charging work releases player attention"
        );
        event_assessment = Some(apply_delivery(registries, state, ids, controller, actor));
    }
    if state.tick().value() < completes_at {
        finish_operation(
            registries,
            state,
            TickSpan::new(completes_at - state.tick().value()),
        );
    }
    if !actor.report.progress.delivery_applied && state.tick().value() == event_tick {
        event_assessment = Some(apply_delivery(registries, state, ids, controller, actor));
    }
    if let Some(assessment) = event_assessment {
        adapt_after_delivery(registries, state, ids, actor, assessment);
    }

    assert_eq!(state.player_work().active(), None);
    let stored_after = state
        .energy()
        .get_store(option.store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("manual-recovery {} drive disappeared", option.name));
    assert_eq!(
        stored_after,
        stored_before
            .checked_add(option.energy)
            .unwrap_or_else(|| panic!("manual-recovery energy accounting overflowed")),
        "manual power must add exactly its validated generated work"
    );
    actor.report.choices.manual_recharges = actor
        .report
        .choices
        .manual_recharges
        .checked_add(1)
        .unwrap_or_else(|| panic!("manual-recovery count overflowed"));
    actor.report.resources.manually_generated_energy = actor
        .report
        .resources
        .manually_generated_energy
        .checked_add(option.energy)
        .unwrap_or_else(|| panic!("manual-recovery generated-energy accounting overflowed"));
    actor.report.resources.manual_power_ticks = actor
        .report
        .resources
        .manual_power_ticks
        .checked_add(duration)
        .unwrap_or_else(|| panic!("manual-recovery duration accounting overflowed"));
    actor.report.resources.manual_power_metabolic_energy = actor
        .report
        .resources
        .manual_power_metabolic_energy
        .checked_add(budget.metabolic_energy())
        .unwrap_or_else(|| panic!("manual-recovery metabolic accounting overflowed"));
    actor.report.resources.manual_power_hydration = actor
        .report
        .resources
        .manual_power_hydration
        .checked_add(budget.hydration())
        .unwrap_or_else(|| panic!("manual-recovery hydration accounting overflowed"));
}

fn probe_manual_recovery_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    preference: EnergyRecoveryPreference,
) -> ManualRecoveryProbe {
    let mut options = Vec::new();
    let mut survival_limited = false;
    let mut equipment_limited = false;
    let mut storage_limited = false;
    for (name, store) in [("small", ids.small_drive), ("large", ids.large_drive)] {
        match manual_recovery_option(registries, state, ids, mass, name, store) {
            Ok(Some(option)) => options.push(option),
            Ok(None) => {}
            Err(ManualPowerError::Work(
                PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
                | PlayerWorkStartError::InsufficientHydration { .. },
            )) => survival_limited = true,
            Err(ManualPowerError::EnergySink(EnergySinkError::InsufficientCapacity { .. })) => {
                storage_limited = true;
            }
            Err(
                ManualPowerError::ZeroEquipmentPower { .. }
                | ManualPowerError::ConditionDuration(_),
            ) => equipment_limited = true,
            Err(error) => panic!("workshop manual-power recovery projection failed: {error}"),
        }
    }

    let survival = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("workshop survival state disappeared before manual recovery"));
    let physiology = registries.survival().physiology();
    let before_policy_filter = options.len();
    if preference == EnergyRecoveryPreference::ProtectSurvival {
        options.retain(|option| {
            let budget = option.start.resource_budget();
            let energy_after = survival
                .metabolic_energy()
                .checked_sub(budget.metabolic_energy());
            let hydration_after = survival.hydration().checked_sub(budget.hydration());
            energy_after.is_some_and(|value| value >= physiology.hungry_below())
                && hydration_after.is_some_and(|value| value >= physiology.thirsty_below())
        });
    }
    let policy_declined = before_policy_filter > 0 && options.is_empty();
    let option = options.into_iter().min_by_key(|option| {
        let budget = option.start.resource_budget();
        let duration =
            option.start.work().completes_at().value() - option.start.work().started_at().value();
        (
            budget.metabolic_energy(),
            budget.hydration(),
            duration,
            option.name,
        )
    });
    ManualRecoveryProbe {
        option,
        survival_limited,
        policy_declined,
        equipment_limited,
        storage_limited,
    }
}

fn largest_manual_recovery(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    desired: Mass,
    preference: EnergyRecoveryPreference,
) -> ManualRecoverySearch {
    let desired_probe = probe_manual_recovery_option(registries, state, ids, desired, preference);
    if let Some(option) = desired_probe.option {
        return ManualRecoverySearch::Available {
            mass: desired,
            option: Box::new(option),
            adaptive_constraint: None,
        };
    }

    let adaptive_constraint = if desired_probe.policy_declined {
        Some(ManualRecoveryConstraint::SurvivalPolicy)
    } else if desired_probe.survival_limited {
        Some(ManualRecoveryConstraint::SurvivalReserve)
    } else if desired_probe.equipment_limited {
        Some(ManualRecoveryConstraint::EquipmentCondition)
    } else if desired_probe.storage_limited {
        Some(ManualRecoveryConstraint::StorageCapacity)
    } else {
        None
    };

    let mut low = 1_u64;
    let mut high = desired.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mass = Mass::from_milligrams(midpoint);
        let probe = probe_manual_recovery_option(registries, state, ids, mass, preference);
        if let Some(option) = probe.option {
            best = Some((mass, option));
            low = midpoint + 1;
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    if let Some((mass, option)) = best {
        return ManualRecoverySearch::Available {
            mass,
            option: Box::new(option),
            adaptive_constraint,
        };
    }

    let minimum_probe =
        probe_manual_recovery_option(registries, state, ids, Mass::from_milligrams(1), preference);
    if minimum_probe.policy_declined || desired_probe.policy_declined {
        ManualRecoverySearch::DeclinedForSurvival
    } else if minimum_probe.survival_limited || desired_probe.survival_limited {
        ManualRecoverySearch::SurvivalLimited
    } else if minimum_probe.equipment_limited || desired_probe.equipment_limited {
        ManualRecoverySearch::EquipmentLimited
    } else if minimum_probe.storage_limited || desired_probe.storage_limited {
        ManualRecoverySearch::StorageLimited
    } else {
        panic!(
            "manual recovery search found no viable option without a classified physical or policy constraint"
        )
    }
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
    println!(
        "  crush#{batch_index}: drive={} mass={}mg rate={}mg/s power={}uW duration={}t constraints=[throughput:{}t energy:{}t] condition={}ppm->{}ppm bottleneck={:?}",
        option.name,
        mass.milligrams(),
        option.resolved.processing_rate().milligrams_per_second(),
        option.resolved.available_power().whole_microwatts(),
        option.resolved.process_resolution().duration().value(),
        option.resolved.throughput_duration().value(),
        option.resolved.energy_duration().value(),
        option.resolved.condition_before().parts_per_million(),
        option.resolved.condition_after().parts_per_million(),
        option.resolved.bottleneck(),
    );
    let duration = option.resolved.process_resolution().duration();
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
    let started_at = state.tick().value();
    let completes_at = started_at
        .checked_add(duration.value())
        .unwrap_or_else(|| panic!("gameplay harness crushing completion tick overflowed"));
    if !actor.report.progress.delivery_applied
        && started_at < controller.delivery.delivery_at_tick
        && controller.delivery.delivery_at_tick < completes_at
    {
        finish_operation(
            registries,
            state,
            TickSpan::new(controller.delivery.delivery_at_tick - started_at),
        );
        let assessment = apply_delivery(registries, state, ids, controller, actor);
        if assessment.stage() == StructuralStage::Failed {
            let outcome = advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("gameplay harness suspension tick failed: {error}"));
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
            println!(
                "  recovery: suspended crush#{batch_index} resumes with its original work-in-process and remaining active time"
            );
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

fn structural_assessment(
    analysis: &deep_hearth::structural::StructuralAnalysis,
    element: StructuralElementId,
) -> StructuralAssessment {
    analysis
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == element)
        .copied()
        .unwrap_or_else(|| panic!("gameplay harness structural assessment missing"))
}

fn structural_label(assessment: StructuralAssessment) -> String {
    if assessment.stage() == StructuralStage::Failed {
        "Failed".to_owned()
    } else {
        format!(
            "{:?}/{}ppm",
            assessment.stage(),
            assessment.utilization_ppm()
        )
    }
}

fn analyze_workshop_supports(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
) -> (StructuralAssessment, StructuralAssessment) {
    let analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        state.structures(),
    )
    .unwrap_or_else(|error| panic!("workshop structural analysis failed: {error}"));
    (
        structural_assessment(&analysis, ids.compact_support),
        structural_assessment(&analysis, ids.reinforced_support),
    )
}

fn transfer_controlled_delivery(
    registries: &Registries,
    state: &mut AppState,
    authorization: MaterialTransferResolution,
) {
    validate_material_transfer(registries, state, authorization)
        .unwrap_or_else(|error| panic!("workshop controlled delivery validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("workshop controlled delivery commit failed: {error}"));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrusherRelocationOutcome {
    Relocated,
    Blocked,
}

fn try_relocate_crusher(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    current_support: &mut StructuralElementId,
    alternate_support: &mut StructuralElementId,
    structure: &mut ScenarioStructureReport,
) -> CrusherRelocationOutcome {
    let relocation = match validate_relocate_equipment(
        registries,
        state,
        ids.crusher,
        *alternate_support,
    ) {
        Ok(relocation) => relocation,
        Err(EquipmentSupportError::TargetNotActive {
            element: _element,
            lifecycle,
        }) => {
            println!(
                "  recovery blocked: alternate bay is {lifecycle:?} after the stored-matter delivery"
            );
            return CrusherRelocationOutcome::Blocked;
        }
        Err(error) => panic!("crusher recovery relocation validation failed: {error}"),
    };
    let preview_assessment =
        structural_assessment(relocation.structural_analysis(), *alternate_support);
    if preview_assessment.stage() == StructuralStage::Failed {
        println!(
            "  recovery blocked: mounting the crusher on the alternate bay would fail it at {}ppm utilization",
            preview_assessment.utilization_ppm()
        );
        return CrusherRelocationOutcome::Blocked;
    }

    let abandoned_support = *current_support;
    let assessment = structural_assessment(relocation.structural_analysis(), *alternate_support);
    debug_assert_ne!(assessment.stage(), StructuralStage::Failed);
    relocation
        .commit(state)
        .unwrap_or_else(|error| panic!("crusher recovery relocation commit failed: {error}"));
    println!(
        "  recovery: relocated crusher to alternate support -> {:?}/{}ppm utilization",
        assessment.stage(),
        assessment.utilization_ppm()
    );
    let abandoned = state
        .structures()
        .get_element(abandoned_support)
        .unwrap_or_else(|| panic!("abandoned workshop support disappeared during recovery"));
    if abandoned.lifecycle() == StructuralLifecycle::Failed || abandoned.is_cracked() {
        structure.structural_damage_debt = true;
        println!(
            "  recovery debt: previous bay remains {:?} cracked={} after relocation; restoring production did not repair the structure",
            abandoned.lifecycle(),
            abandoned.is_cracked(),
        );
    } else {
        println!(
            "  recovery note: previous bay retains {}mN of inventory-owned stored-matter load after relocation",
            abandoned
                .load(StructuralLoadKind::StoredMatter)
                .millinewtons(),
        );
    }
    std::mem::swap(current_support, alternate_support);
    structure.support_relocation = true;
    CrusherRelocationOutcome::Relocated
}

fn adapt_after_delivery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    actor: &mut ScenarioActorRuntime<'_>,
    after: StructuralAssessment,
) {
    if after.stage() == StructuralStage::Failed {
        let suspended_wip = state.production().jobs().find(|job| {
            job.is_suspended()
                && job
                    .equipment_provider()
                    .is_some_and(|provider| provider.equipment() == ids.crusher)
        });
        if let Some(job) = suspended_wip {
            let suspension = job
                .suspension()
                .unwrap_or_else(|| panic!("suspended crusher job lost suspension state"));
            println!(
                "  consequence: failed support suspends job {} with {} active tick(s) remaining; its selected ore is still conserved as work-in-process",
                job.id().value(),
                suspension.remaining_active_time().value()
            );
        }
        let untouched_mass = state
            .inventory()
            .get_lot(ids.ore_lot)
            .map(|lot| lot.mass())
            .unwrap_or(Mass::ZERO);
        if !untouched_mass.is_zero() {
            let probe_mass = Mass::from_milligrams(
                untouched_mass
                    .milligrams()
                    .min(actor.nominal_batch_mass.milligrams()),
            );
            let selection = [MaterialLotSelection::new(ids.ore_lot, probe_mass)];
            let blocked = resolve_comminution_process(
                registries,
                state,
                ComminutionRequest::new(
                    PROCESS_CRUSH_ORE,
                    ids.ore_source,
                    &selection,
                    ids.crusher,
                    ids.small_drive,
                ),
            );
            actor.report.structure.support_failure_blocked_production = matches!(
                blocked,
                Err(ComminutionResolutionError::Equipment(
                    EquipmentProviderError::StructuralSupportNotActive {
                        equipment: _equipment,
                        element: _element,
                        lifecycle: _lifecycle,
                    }
                ))
            );
            println!(
                "  consequence: failed support blocks the next untouched ore operation={}",
                actor.report.structure.support_failure_blocked_production
            );
        } else {
            if suspended_wip.is_none() {
                println!(
                    "  consequence: support failed after the work order was already complete; recovery still leaves structural damage debt"
                );
            } else {
                println!(
                    "  queue state: no untouched ore remains behind the suspended work-in-process"
                );
            }
        }
        if try_relocate_crusher(
            registries,
            state,
            ids,
            actor.current_support,
            actor.alternate_support,
            actor.report.structure,
        ) == CrusherRelocationOutcome::Blocked
        {
            actor.report.structure.structural_stop = true;
            println!(
                "  structural frontier: no surviving bay can carry the crusher, so new production remains blocked"
            );
        }
        return;
    }

    if after.stage() == StructuralStage::Cracking || after.stage() == StructuralStage::Strained {
        if actor.policy.structural_preference == StructuralPreference::MoveOnlyForFailure {
            println!(
                "  decision: remain on current support at {}; player policy moves equipment only when support actually fails",
                structural_label(after)
            );
            return;
        }
        let alternate = match validate_relocate_equipment(
            registries,
            state,
            ids.crusher,
            *actor.alternate_support,
        ) {
            Ok(alternate) => alternate,
            Err(EquipmentSupportError::TargetNotActive {
                element: _element,
                lifecycle,
            }) => {
                println!(
                    "  decision: remain on current support; alternate bay is {lifecycle:?} after the stored-matter delivery"
                );
                return;
            }
            Err(error) => panic!("crusher relocation prediction failed: {error}"),
        };
        let alternate_assessment =
            structural_assessment(alternate.structural_analysis(), *actor.alternate_support);
        if (
            stage_rank(alternate_assessment.stage()),
            alternate_assessment.utilization_ppm(),
        ) < (stage_rank(after.stage()), after.utilization_ppm())
        {
            println!(
                "  decision: alternate bay with the crusher mounted would be {}; relocate before failure",
                structural_label(alternate_assessment)
            );
            let relocated = try_relocate_crusher(
                registries,
                state,
                ids,
                actor.current_support,
                actor.alternate_support,
                actor.report.structure,
            );
            debug_assert_eq!(relocated, CrusherRelocationOutcome::Relocated);
        } else {
            println!(
                "  decision: remain on current support at {}; alternate bay with the crusher mounted would be {}",
                structural_label(after),
                structural_label(alternate_assessment)
            );
        }
    }
}

fn apply_delivery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) -> StructuralAssessment {
    assert_eq!(
        state.tick().value(),
        controller.delivery.delivery_at_tick,
        "controlled gameplay event must occur at its planned world tick"
    );
    actor.report.progress.delivery_applied = true;
    actor.report.progress.operations_before_delivery = actor.report.progress.operations_completed;
    let authorization = controller
        .authorization
        .take()
        .unwrap_or_else(|| panic!("controlled delivery authorization was already consumed"));
    transfer_controlled_delivery(registries, state, authorization);
    let (compact, reinforced) = analyze_workshop_supports(registries, state, ids);
    let (after, alternate_after) = if *actor.current_support == ids.compact_support {
        (compact, reinforced)
    } else {
        (reinforced, compact)
    };
    actor.report.structure.structural_consequence =
        compact.stage() != StructuralStage::Stable || reinforced.stage() != StructuralStage::Stable;
    actor.report.structure.structural_damage_debt |= [ids.compact_support, ids.reinforced_support]
        .into_iter()
        .any(|support| {
            state
                .structures()
                .get_element(support)
                .is_some_and(|record| record.is_cracked())
        });
    let destination = if ids.delivery_support == ids.compact_support {
        "compact"
    } else {
        "reinforced"
    };
    println!(
        "  delivery: move={}mg wood into {destination} supported storage at tick={} after {} operation(s) / {}mg processed -> active={} alternate={}",
        controller.delivery.mass.milligrams(),
        state.tick().value(),
        actor.report.progress.operations_completed,
        actor.report.progress.processed_mass.milligrams(),
        structural_label(after),
        structural_label(alternate_after),
    );
    after
}

fn apply_delivery_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    controller: &mut ControlledDeliveryRuntime<'_>,
    actor: &mut ScenarioActorRuntime<'_>,
) {
    let after = apply_delivery(registries, state, ids, controller, actor);
    adapt_after_delivery(registries, state, ids, actor, after);
}

#[path = "workshop/runner.rs"]
mod runner;

pub(super) fn run_scenario(
    registries: &Registries,
    variation: ScenarioVariation,
) -> ScenarioReport {
    runner::run_scenario(registries, variation)
}

pub(super) fn run_gameplay_harness(mode: ScenarioPlanMode) {
    runner::run_gameplay_harness(mode);
}
