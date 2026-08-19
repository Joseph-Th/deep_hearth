//! Headless workshop gameplay harness over the same canonical content registries used by the game.
//!
//! The harness deliberately varies physical initial conditions and player priorities, then lets a
//! small operational policy react only to observed state and resolver projections. The required gate
//! runs five maintained anchor cases plus a small fresh bounded sample. The explicit report lane uses
//! a larger fresh bounded sample by default. Both print exact replay roots so any result can be
//! reproduced. Physical scenario and
//! automated-player behavior randomness are independent. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`
//! reproduces the world/scenario sample and `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` reproduces policy
//! variation. Focused gameplay probes also use one maintained anchor plus one fresh bounded physical
//! sample by default; the same variation seed reproduces that sample and
//! `DEEP_HEARTH_GAMEPLAY_SEEDS` provides an exact focused-probe sweep. Each scenario schedules a real material
//! transfer into supported storage, so ordinary inventory ownership can change structural margin while
//! production is active.
//! The controlled delivery event is hidden from the acting policy until its effects are observable.
//! `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the whole matrix with an exact comma-separated decimal or
//! `0x` hexadecimal seed list; malformed entries are rejected instead of ignored. Detailed trace
//! output is opt-in via `DEEP_HEARTH_GAMEPLAY_VERBOSE`.

use std::collections::BTreeSet;
use std::env;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

mod configuration;
mod contracts;
mod probe_setup;
mod progression_probe;
mod report;
mod scenario;
mod seed;
mod survival_probe;

use configuration::{
    MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT, ScenarioPlanMode,
    focused_probe_seeds_from, scenario_seeds_from,
};
use contracts::{assert_anchor_diversity, assert_scenario_contracts};
use deep_hearth::content::gameplay_fixture::{
    authorize_controlled_material_delivery, materialize_structure, seed_composed_lot,
    seed_energy_store as bootstrap_seed_energy_store, seed_lot,
};
use probe_setup::{setup_foundry_probe, setup_ore_preparation_probe};
use progression_probe::run_primitive_progression_probe;
use report::{
    EnergyRecoveryPreference, MaintenancePreference, PowerPreference, ScenarioPolicyVariation,
    ScenarioReport, StructuralPreference, print_content_summary, print_harness_summary,
};
use scenario::ScenarioVariation;
use seed::mix64;
use survival_probe::run_survival_provisioning_probe;

fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
}

fn fresh_exploration_root(salt: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = (now as u64) ^ ((now >> 64) as u64) ^ u64::from(process::id()) ^ salt;
    mix64(folded)
}

fn focused_probe_seeds(name: &str, maintained_seed: u64, salt: u64) -> Vec<u64> {
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let generated_variation_root = fresh_exploration_root(MAINTAINED_VARIATION_ROOT ^ salt);
    let seeds = focused_probe_seeds_from(
        scenario_raw.as_deref(),
        variation_raw.as_deref(),
        maintained_seed,
        salt,
        generated_variation_root,
    )
    .unwrap_or_else(|error| panic!("gameplay focused probe seed configuration failed: {error:?}"));
    let replay = seeds
        .iter()
        .map(|seed| format!("0x{seed:016X}"))
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "PROBE INPUT name={name} samples={} replay={replay}",
        seeds.len()
    );
    seeds
}

fn run_focused_probe(name: &str, maintained_seed: u64, salt: u64, probe: fn(&Registries, u64)) {
    let registries = build_registries();
    for seed in focused_probe_seeds(name, maintained_seed, salt) {
        probe(&registries, seed);
    }
}

macro_rules! println {
    ($($argument:tt)*) => {{
        if has_verbose_output() {
            std::println!($($argument)*);
        }
    }};
}

use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::core::quantity::{Area, Energy, Length, Mass, Temperature};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{TickSpan, WorldSeed};
use deep_hearth::energy::{
    EnergySinkError, EnergyStoreId, EnergySupplyError, calculate_mass_specific_energy,
};
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, EquipmentMaintenanceRequest,
    EquipmentMaintenanceResolutionError, EquipmentProviderError, EquipmentSupportError,
    add_equipment, resolve_equipment_maintenance, resolve_equipment_provider,
    validate_assemble_equipment, validate_equipment_repair, validate_mount_equipment,
    validate_relocate_equipment,
};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialTransferResolution, StockpileId,
    StockpileStorageProfile, add_stockpile, validate_material_transfer, validate_mount_stockpile,
};
use deep_hearth::labor::{
    ManualPowerError, ManualPowerRequest, PlayerWorkStartError, ValidatedManualPowerStart,
    validate_start_manual_power,
};
use deep_hearth::maintenance::{Condition, MaintenanceBand};
use deep_hearth::material::{CommodityKey, CompositionComponent, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::ore_processing::{
    ComminutionBatchError, ComminutionBottleneck, ComminutionRequest, ComminutionResolutionError,
    ResolvedComminution, ScreeningBatchError, ScreeningProcessDefinition, ScreeningRequest,
    ScreeningResolutionError, resolve_comminution_process, resolve_screening_process,
};
use deep_hearth::production::{
    ProcessOutputRoute, ProductionAvailabilityChange, ProductionJobId, ProductionSuspensionReason,
    validate_start_process, validate_start_process_routed,
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
    CastingRequest, MeltingBatchError, MeltingRequest, MeltingResolutionError,
    resolve_casting_process, resolve_melting_process,
};

use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_THERMAL_SINK,
};
use deep_hearth::content::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_DRY_SCREEN,
    EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
};
use deep_hearth::content::{
    FORM_INGOT, FORM_LOG, FORM_ORE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const WORKSHOP_SUPPORT_LENGTH: Length = Length::from_micrometers(2_000_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrushStopReason {
    EnergyUnavailable,
    MaintenanceCritical,
}

struct ManualRecoveryProbe {
    option: Option<ManualRecoveryOption>,
    survival_limited: bool,
    policy_declined: bool,
    equipment_unavailable: bool,
}

enum ManualRecoverySearch {
    Available {
        mass: Mass,
        option: Box<ManualRecoveryOption>,
    },
    DeclinedForSurvival,
    SurvivalLimited,
    EquipmentUnavailable,
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
    let source = add_solid_stockpile(state, capacity, "emergency power kit");
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
    bottleneck: ComminutionBottleneck,
    completed: bool,
}

struct ScenarioRuntime<'state> {
    variation: ScenarioVariation,
    delivery_authorization: &'state mut Option<MaterialTransferResolution>,
    current_support: &'state mut StructuralElementId,
    alternate_support: &'state mut StructuralElementId,
    report: &'state mut ScenarioReport,
}

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn nominal_equipment_mass_capability(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    capability: CapabilityId,
) -> Mass {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    match definition.capabilities().get_capability(capability) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(value) => panic!(
            "gameplay harness expected mass capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
        None => panic!(
            "gameplay harness equipment {} is missing authored mass capability {}",
            equipment.value(),
            capability.value()
        ),
    }
}

fn seed_energy_store_exact(
    registries: &Registries,
    state: &mut AppState,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    bootstrap_seed_energy_store(registries, state, definition, amount)
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

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(value) => value,
        Err(error) => panic!("gameplay harness condition is invalid: {error}"),
    }
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
    bootstrap_seed_energy_store(registries, state, definition, amount)
}

fn add_solid_stockpile(state: &mut AppState, capacity: Mass, context: &'static str) -> StockpileId {
    add_stockpile(state, capacity, StockpileStorageProfile::solid_only())
        .unwrap_or_else(|error| panic!("gameplay harness {context} stockpile failed: {error}"))
}

fn mixed_ore_composition(copper_ppm: u32) -> MaterialComposition {
    match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - copper_ppm),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("gameplay harness ore composition failed: {error}"),
    }
}

fn setup_workshop(
    registries: &Registries,
    variation: ScenarioVariation,
) -> (AppState, WorkshopIds, Option<MaterialTransferResolution>) {
    let mut state = AppState::new(WorldSeed::new(variation.world_seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("workshop survival initialization failed: {error}"));
    let ore_mass = variation.ore.order_mass;
    let ore_source = add_solid_stockpile(&mut state, ore_mass, "ore source");
    let crushed_storage = add_solid_stockpile(&mut state, ore_mass, "crushed storage");
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
    let maintenance_source =
        add_solid_stockpile(&mut state, maintenance_capacity, "maintenance source");
    let maintenance_spent =
        add_solid_stockpile(&mut state, maintenance_capacity, "maintenance spent");

    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        ore_mass,
        ROOM_TEMPERATURE,
        mixed_ore_composition(variation.ore.ore_copper_ppm),
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

    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        variation.crusher.initial_crusher_condition,
    )
    .unwrap_or_else(|error| panic!("gameplay harness crusher allocation failed: {error}"));
    let hand_crank = assemble_workshop_hand_crank(registries, &mut state);
    let furnace = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("gameplay harness furnace allocation failed: {error}"));
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
    let background_storage = add_solid_stockpile(
        &mut state,
        variation.structure.reinforced_background_mass,
        "reinforced bay background storage",
    );
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

    let delivery_source = add_solid_stockpile(
        &mut state,
        variation.delivery.mass,
        "controlled delivery source",
    );
    let delivery_destination = add_solid_stockpile(
        &mut state,
        variation.delivery.mass,
        "controlled delivery destination",
    );
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
    let small_drive = seed_energy_store_exact(
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
    let large_drive = seed_energy_store_exact(
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

fn current_crusher_batch_limit(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
) -> Mass {
    let definition = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
    let provider = resolve_equipment_provider(registries, state, ids.crusher)
        .unwrap_or_else(|error| panic!("gameplay crusher provider resolution failed: {error}"));
    match provider.get_capability(definition.max_batch_mass_capability()) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(value) => panic!(
            "crusher maximum-batch capability changed to {:?}",
            value.kind()
        ),
        None => panic!("crusher lost its maximum-batch capability"),
    }
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
    let outcome = validate_equipment_repair(registries, state, resolution)
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

fn stockpile_first_lot(state: &AppState, stockpile: StockpileId) -> MaterialLotId {
    state
        .inventory()
        .lot_ids(stockpile)
        .next()
        .unwrap_or_else(|| panic!("gameplay harness expected output lot is missing"))
}

struct CrushOption {
    name: &'static str,
    store: EnergyStoreId,
    stored_before: Energy,
    resolved: ResolvedComminution,
}

#[derive(Clone, Copy)]
struct CrushChoiceContext {
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
    preference: PowerPreference,
}

fn resolve_crush_option(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    name: &'static str,
    store: EnergyStoreId,
) -> Option<CrushOption> {
    let stored_before = state
        .energy()
        .get_store(store)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("gameplay harness {name} drive disappeared"));
    let selection = [MaterialLotSelection::new(ids.ore_lot, mass)];
    match resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &selection,
            ids.crusher,
            store,
        ),
    ) {
        Ok(resolved) => Some(CrushOption {
            name,
            store,
            stored_before,
            resolved,
        }),
        Err(ComminutionResolutionError::Energy(EnergySupplyError::InsufficientEnergy {
            ..
        }))
        | Err(ComminutionResolutionError::BatchMassExceeded { .. }) => None,
        Err(error) => panic!("gameplay harness {name} drive resolution failed: {error}"),
    }
}

fn resolve_crush_options(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
) -> (Option<CrushOption>, Option<CrushOption>) {
    (
        resolve_crush_option(registries, state, ids, mass, "small", ids.small_drive),
        resolve_crush_option(registries, state, ids, mass, "large", ids.large_drive),
    )
}

fn largest_resolvable_crush_batch(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    desired: Mass,
) -> Option<(Mass, Option<CrushOption>, Option<CrushOption>)> {
    if desired.is_zero() {
        return None;
    }
    let options = resolve_crush_options(registries, state, ids, desired);
    if options.0.is_some() || options.1.is_some() {
        return Some((desired, options.0, options.1));
    }

    let mut low = 1_u64;
    let mut high = desired.milligrams().saturating_sub(1);
    let mut best = None;
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mass = Mass::from_milligrams(midpoint);
        let options = resolve_crush_options(registries, state, ids, mass);
        if options.0.is_some() || options.1.is_some() {
            best = Some((mass, options.0, options.1));
            low = midpoint + 1;
        } else {
            high = midpoint.saturating_sub(1);
        }
    }
    best
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
    runtime: &mut ScenarioRuntime<'_>,
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

    let event_tick = runtime.variation.delivery.delivery_at_tick;
    let mut event_assessment = None;
    if !runtime.report.progress.delivery_applied
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
        event_assessment = Some(apply_delivery(registries, state, ids, runtime));
    }
    if state.tick().value() < completes_at {
        finish_operation(
            registries,
            state,
            TickSpan::new(completes_at - state.tick().value()),
        );
    }
    if !runtime.report.progress.delivery_applied && state.tick().value() == event_tick {
        event_assessment = Some(apply_delivery(registries, state, ids, runtime));
    }
    if let Some(assessment) = event_assessment {
        adapt_after_delivery(registries, state, ids, runtime, assessment);
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
    runtime.report.choices.manual_recharges = runtime
        .report
        .choices
        .manual_recharges
        .checked_add(1)
        .unwrap_or_else(|| panic!("manual-recovery count overflowed"));
    runtime.report.resources.manually_generated_energy = runtime
        .report
        .resources
        .manually_generated_energy
        .checked_add(option.energy)
        .unwrap_or_else(|| panic!("manual-recovery generated-energy accounting overflowed"));
    runtime.report.resources.manual_power_ticks = runtime
        .report
        .resources
        .manual_power_ticks
        .checked_add(duration)
        .unwrap_or_else(|| panic!("manual-recovery duration accounting overflowed"));
    runtime.report.resources.manual_power_metabolic_energy = runtime
        .report
        .resources
        .manual_power_metabolic_energy
        .checked_add(budget.metabolic_energy())
        .unwrap_or_else(|| panic!("manual-recovery metabolic accounting overflowed"));
    runtime.report.resources.manual_power_hydration = runtime
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
    let mut equipment_unavailable = false;
    for (name, store) in [("small", ids.small_drive), ("large", ids.large_drive)] {
        match manual_recovery_option(registries, state, ids, mass, name, store) {
            Ok(Some(option)) => options.push(option),
            Ok(None) => {}
            Err(ManualPowerError::Work(
                PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
                | PlayerWorkStartError::InsufficientHydration { .. },
            )) => survival_limited = true,
            Err(ManualPowerError::EnergySink(EnergySinkError::InsufficientCapacity { .. })) => {}
            Err(ManualPowerError::ZeroEquipmentPower { .. }) => equipment_unavailable = true,
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
        equipment_unavailable,
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
        };
    }

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
        };
    }

    let minimum_probe =
        probe_manual_recovery_option(registries, state, ids, Mass::from_milligrams(1), preference);
    if minimum_probe.policy_declined || desired_probe.policy_declined {
        ManualRecoverySearch::DeclinedForSurvival
    } else if minimum_probe.survival_limited || desired_probe.survival_limited {
        ManualRecoverySearch::SurvivalLimited
    } else if minimum_probe.equipment_unavailable || desired_probe.equipment_unavailable {
        ManualRecoverySearch::EquipmentUnavailable
    } else {
        ManualRecoverySearch::SurvivalLimited
    }
}

fn schedule_controlled_delivery_event(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    variation: &mut ScenarioVariation,
) {
    let reference_duration =
        largest_resolvable_crush_batch(registries, state, ids, variation.ore.nominal_batch_mass)
            .and_then(|(_mass, small, large)| small.or(large))
            .map(|option| option.resolved.process_resolution().duration().value())
            .unwrap_or_else(|| {
                panic!("gameplay harness has no powered reference operation for delivery timing")
            });
    assert!(
        reference_duration > 0,
        "nonzero gameplay batch must take at least one tick"
    );
    let nominal_batch_count = variation
        .ore
        .order_mass
        .milligrams()
        .div_ceil(variation.ore.nominal_batch_mass.milligrams());
    let work_horizon = reference_duration
        .checked_mul(nominal_batch_count)
        .unwrap_or_else(|| panic!("gameplay harness work horizon overflowed"));
    variation.delivery.delivery_at_tick =
        1 + mix64(variation.world_seed ^ 0x57A1_1EED_71A1_1EED) % work_horizon;
}

fn print_crush_option(
    option: &CrushOption,
    thresholds: deep_hearth::maintenance::MaintenanceThresholds,
) {
    let stored_after = option
        .stored_before
        .checked_sub(option.resolved.required_energy())
        .unwrap_or_else(|| panic!("validated crush option overdraws its energy store"));
    println!(
        "  power option {}: duration={}t bottleneck={:?} energy={}nJ reserve={}nJ->{}nJ wear={}ppm->{}ppm ({:?})",
        option.name,
        option.resolved.process_resolution().duration().value(),
        option.resolved.bottleneck(),
        option.resolved.required_energy().nanojoules(),
        option.stored_before.nanojoules(),
        stored_after.nanojoules(),
        option.resolved.condition_before().parts_per_million(),
        option.resolved.condition_after().parts_per_million(),
        thresholds.classify(option.resolved.condition_after()),
    );
}

fn choose_crush_option(
    small: Option<CrushOption>,
    large: Option<CrushOption>,
    context: CrushChoiceContext,
) -> Result<(CrushOption, &'static str, PowerChoiceBasis), CrushStopReason> {
    let CrushChoiceContext {
        thresholds,
        preference,
    } = context;
    match (small, large) {
        (None, None) => Err(CrushStopReason::EnergyUnavailable),
        (Some(option), None) | (None, Some(option)) => {
            if thresholds.classify(option.resolved.condition_after()) == MaintenanceBand::Critical {
                Err(CrushStopReason::MaintenanceCritical)
            } else {
                Ok((
                    option,
                    "only viable energy source",
                    PowerChoiceBasis::SingleSource,
                ))
            }
        }
        (Some(small), Some(large)) => {
            let small_after = thresholds.classify(small.resolved.condition_after());
            let large_after = thresholds.classify(large.resolved.condition_after());
            if small_after == MaintenanceBand::Critical && large_after == MaintenanceBand::Critical
            {
                Err(CrushStopReason::MaintenanceCritical)
            } else {
                match preference {
                    PowerPreference::PreserveReserve => Ok((
                        small,
                        "player priority preserves scarce high-power reserve",
                        PowerChoiceBasis::Policy,
                    )),
                    PowerPreference::FinishSooner => {
                        if large.resolved.process_resolution().duration()
                            < small.resolved.process_resolution().duration()
                        {
                            Ok((
                                large,
                                "player priority minimizes projected batch completion time",
                                PowerChoiceBasis::Policy,
                            ))
                        } else {
                            Ok((
                                small,
                                "both power choices finish equally soon, so preserve reserve",
                                PowerChoiceBasis::Policy,
                            ))
                        }
                    }
                }
            }
        }
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
    loop {
        let Some(record) = state.production().get_job(job) else {
            return JobAdvanceOutcome::Completed;
        };
        if record.is_suspended() {
            return JobAdvanceOutcome::Suspended;
        }
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
}

fn crush_batch(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    mass: Mass,
    option: CrushOption,
    batch_index: u16,
    mut runtime: ScenarioRuntime<'_>,
) -> CrushBatchOutcome {
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
    if !runtime.report.progress.delivery_applied
        && started_at < runtime.variation.delivery.delivery_at_tick
        && runtime.variation.delivery.delivery_at_tick < completes_at
    {
        finish_operation(
            registries,
            state,
            TickSpan::new(runtime.variation.delivery.delivery_at_tick - started_at),
        );
        let assessment = apply_delivery(registries, state, ids, &mut runtime);
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
                    ProductionAvailabilityChange::Suspended {
                        job: _job,
                        reason: _reason,
                        suspended_at: _suspended_at,
                        remaining_active_time: _remaining_active_time,
                    } => None,
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
            runtime.report.structure.production_suspension = true;
            println!(
                "  interruption: crush#{batch_index} suspends with {} active tick(s) remaining; consumed matter and work stay owned as work-in-process",
                suspension.1.value()
            );
            adapt_after_delivery(registries, state, ids, &mut runtime, assessment);
            if runtime.report.structure.structural_stop {
                runtime.report.structure.stranded_work_in_process = true;
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
                adapt_after_delivery(registries, state, ids, &mut runtime, assessment);
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
    report: &mut ScenarioReport,
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
        report.structure.structural_damage_debt = true;
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
    report.structure.support_relocation = true;
    CrusherRelocationOutcome::Relocated
}

fn adapt_after_delivery(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    runtime: &mut ScenarioRuntime<'_>,
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
                    .min(runtime.variation.ore.nominal_batch_mass.milligrams()),
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
            runtime.report.structure.support_failure_blocked_production = matches!(
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
                runtime.report.structure.support_failure_blocked_production
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
            runtime.current_support,
            runtime.alternate_support,
            runtime.report,
        ) == CrusherRelocationOutcome::Blocked
        {
            runtime.report.structure.structural_stop = true;
            println!(
                "  structural frontier: no surviving bay can carry the crusher, so new production remains blocked"
            );
        }
        return;
    }

    if after.stage() == StructuralStage::Cracking || after.stage() == StructuralStage::Strained {
        if runtime.variation.policy.structural_preference
            == StructuralPreference::MoveOnlyForFailure
        {
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
            *runtime.alternate_support,
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
            structural_assessment(alternate.structural_analysis(), *runtime.alternate_support);
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
                runtime.current_support,
                runtime.alternate_support,
                runtime.report,
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
    runtime: &mut ScenarioRuntime<'_>,
) -> StructuralAssessment {
    assert_eq!(
        state.tick().value(),
        runtime.variation.delivery.delivery_at_tick,
        "controlled gameplay event must occur at its planned world tick"
    );
    runtime.report.progress.delivery_applied = true;
    runtime.report.progress.operations_before_delivery =
        runtime.report.progress.operations_completed;
    let authorization = runtime
        .delivery_authorization
        .take()
        .unwrap_or_else(|| panic!("controlled delivery authorization was already consumed"));
    transfer_controlled_delivery(registries, state, authorization);
    let (compact, reinforced) = analyze_workshop_supports(registries, state, ids);
    let (after, alternate_after) = if *runtime.current_support == ids.compact_support {
        (compact, reinforced)
    } else {
        (reinforced, compact)
    };
    runtime.report.structure.structural_consequence =
        compact.stage() != StructuralStage::Stable || reinforced.stage() != StructuralStage::Stable;
    runtime.report.structure.structural_damage_debt |=
        [ids.compact_support, ids.reinforced_support]
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
        runtime.variation.delivery.mass.milligrams(),
        state.tick().value(),
        runtime.report.progress.operations_completed,
        runtime.report.progress.processed_mass.milligrams(),
        structural_label(after),
        structural_label(alternate_after),
    );
    after
}

fn apply_delivery_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    mut runtime: ScenarioRuntime<'_>,
) {
    let after = apply_delivery(registries, state, ids, &mut runtime);
    adapt_after_delivery(registries, state, ids, &mut runtime, after);
}

fn run_scenario(registries: &Registries, mut variation: ScenarioVariation) -> ScenarioReport {
    let (mut state, ids, mut delivery_authorization) = setup_workshop(registries, variation);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness initial matter accounting failed: {error}")
        })
        .total();
    let crusher_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
    let thresholds = crusher_definition.maintenance_thresholds();
    let initial_band = thresholds.classify(variation.crusher.initial_crusher_condition);
    let mut report = ScenarioReport::new(variation, initial_band);
    if initial_band == MaintenanceBand::Critical {
        report.limits.maintenance_warning = true;
        println!(
            "  initial maintenance gate: crusher begins at {}ppm/{initial_band:?}; service before planning powered work",
            variation
                .crusher
                .initial_crusher_condition
                .parts_per_million(),
        );
        if service_crusher(registries, &mut state, ids, &mut report)
            == MaintenanceAttempt::SupplyExhausted
        {
            report.limits.maintenance_stop = true;
            println!(
                "  initial maintenance gate: no replacement stock is available; the work order cannot start"
            );
        }
    }
    schedule_controlled_delivery_event(registries, &state, ids, &mut variation);
    let maintenance_profile = crusher_definition
        .maintenance_profile()
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let delivery_target = if variation.delivery.destination_is_compact {
        "compact"
    } else {
        "reinforced"
    };
    println!(
        "\nSCENARIO world=0x{:016X} behavior=0x{:016X} ore={}ppm Cu order={}mg nominal_batch={}mg crusher={}ppm controller_event=[tick:{} mass:{}mg target:{} actor_visibility:hidden] policy=[power:{} recovery:{} maintenance:{} structure:{}] stored_work=[small:{}+{}ppm nominal-batches, high-power:{}+{}ppm nominal-batches] maintenance=[units:{} replacement:{}mg target:{}ppm]",
        variation.world_seed,
        variation.behavior_seed,
        variation.ore.ore_copper_ppm,
        variation.ore.order_mass.milligrams(),
        variation.ore.nominal_batch_mass.milligrams(),
        variation
            .crusher
            .initial_crusher_condition
            .parts_per_million(),
        variation.delivery.delivery_at_tick,
        variation.delivery.mass.milligrams(),
        delivery_target,
        variation.policy.power_preference.label(),
        variation.policy.energy_recovery_preference.label(),
        variation.policy.maintenance_preference.label(),
        variation.policy.structural_preference.label(),
        variation.crusher.small_drive_batch_budget,
        variation.crusher.small_drive_partial_batch_ppm,
        variation.crusher.large_drive_batch_budget,
        variation.crusher.large_drive_partial_batch_ppm,
        variation.crusher.maintenance_replacement_units,
        maintenance_profile.replacement_mass().milligrams(),
        maintenance_profile.restored_condition().parts_per_million(),
    );
    println!(
        "  objective: complete the ore work order using observable workshop state; react to the controlled delivery only after it occurs"
    );

    let compact_mount =
        validate_mount_equipment(registries, &state, ids.crusher, ids.compact_support)
            .unwrap_or_else(|error| panic!("compact bay mount prediction failed: {error}"));
    let reinforced_mount =
        validate_mount_equipment(registries, &state, ids.crusher, ids.reinforced_support)
            .unwrap_or_else(|error| panic!("reinforced bay mount prediction failed: {error}"));
    let compact_assessment =
        structural_assessment(compact_mount.structural_analysis(), ids.compact_support);
    let reinforced_assessment = structural_assessment(
        reinforced_mount.structural_analysis(),
        ids.reinforced_support,
    );
    println!(
        "  support options: compact={}; reinforced={} (reinforced stored cargo={}mg)",
        structural_label(compact_assessment),
        structural_label(reinforced_assessment),
        variation.structure.reinforced_background_mass.milligrams(),
    );
    assert_ne!(
        compact_assessment.stage(),
        StructuralStage::Failed,
        "gameplay scenario must offer a legal compact crusher siting option"
    );
    assert_ne!(
        reinforced_assessment.stage(),
        StructuralStage::Failed,
        "gameplay scenario must offer a legal reinforced crusher siting option"
    );
    let compact_is_better = (
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    let choose_compact = compact_is_better;
    let (mut current_support, mut alternate_support, selected_mount, support_name) =
        if choose_compact {
            report.choices.chose_compact_support = true;
            (
                ids.compact_support,
                ids.reinforced_support,
                compact_mount,
                "compact clear bay",
            )
        } else {
            (
                ids.reinforced_support,
                ids.compact_support,
                reinforced_mount,
                "reinforced occupied bay",
            )
        };
    let reason = "player chooses the best currently observable structural margin";
    println!("  decision: mount crusher on {support_name}; {reason}");
    selected_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected crusher mount failed: {error}"));

    'work_order: while report.progress.processed_mass < report.progress.target_mass {
        if report.structure.structural_stop {
            println!(
                "  decision: stop crushing; the delivered stored-matter load left no support that can carry the machine"
            );
            break;
        }
        if report.limits.maintenance_stop {
            println!(
                "  decision: stop crushing; the crusher is critical and replacement stock is unavailable"
            );
            break;
        }
        if !report.progress.delivery_applied {
            assert!(
                state.tick().value() <= variation.delivery.delivery_at_tick,
                "controlled event tick was passed without being applied"
            );
            if state.tick().value() == variation.delivery.delivery_at_tick {
                apply_delivery_and_adapt(
                    registries,
                    &mut state,
                    ids,
                    ScenarioRuntime {
                        variation,
                        delivery_authorization: &mut delivery_authorization,
                        current_support: &mut current_support,
                        alternate_support: &mut alternate_support,
                        report: &mut report,
                    },
                );
                continue;
            }
        }

        let (batch_mass, selected, reason, choice_basis, adaptive_batch) = loop {
            let current_condition = state
                .equipment()
                .get_equipment(ids.crusher)
                .map(|record| record.condition())
                .unwrap_or_else(|| panic!("crusher disappeared during gameplay harness"));
            let band = thresholds.classify(current_condition);
            if band != MaintenanceBand::Normal && !report.limits.maintenance_warning {
                report.limits.maintenance_warning = true;
                println!(
                    "  maintenance transition: condition={}ppm band={band:?}",
                    current_condition.parts_per_million()
                );
            }
            if band == MaintenanceBand::Warning
                && variation.policy.maintenance_preference
                    == MaintenancePreference::ServiceAtWarning
                && !report.maintenance.supply_exhausted
            {
                println!(
                    "  decision: service crusher in warning condition because player policy favors preventive maintenance"
                );
                match service_crusher(registries, &mut state, ids, &mut report) {
                    MaintenanceAttempt::Serviced => continue,
                    MaintenanceAttempt::SupplyExhausted => {
                        println!(
                            "  maintenance policy: preventive service is unavailable; continue legal work until condition or another constraint forces a stop"
                        );
                    }
                }
            }
            if band == MaintenanceBand::Critical {
                println!(
                    "  decision: service crusher before more work because current condition is critical"
                );
                match service_crusher(registries, &mut state, ids, &mut report) {
                    MaintenanceAttempt::Serviced => continue,
                    MaintenanceAttempt::SupplyExhausted => {
                        report.limits.maintenance_stop = true;
                        println!(
                            "  decision: stop crushing; replacement stock is exhausted and the crusher remains critical"
                        );
                        break 'work_order;
                    }
                }
            }

            let remaining = report
                .progress
                .target_mass
                .checked_sub(report.progress.processed_mass)
                .unwrap_or_else(|| panic!("workshop processed mass exceeded its work order"));
            let planned_mass = Mass::from_milligrams(
                remaining
                    .milligrams()
                    .min(variation.ore.nominal_batch_mass.milligrams()),
            );
            let condition_limit = current_crusher_batch_limit(registries, &state, ids);
            if condition_limit.is_zero() {
                report.limits.maintenance_stop = true;
                println!(
                    "  decision: stop crushing; current crusher condition leaves no usable batch capacity"
                );
                break 'work_order;
            }
            let desired_mass =
                Mass::from_milligrams(planned_mass.milligrams().min(condition_limit.milligrams()));
            let Some((resolved_mass, small, large)) =
                largest_resolvable_crush_batch(registries, &state, ids, desired_mass)
            else {
                match largest_manual_recovery(
                    registries,
                    &state,
                    ids,
                    desired_mass,
                    variation.policy.energy_recovery_preference,
                ) {
                    ManualRecoverySearch::Available { mass, option } => {
                        if mass < desired_mass {
                            println!(
                                "  manual recovery adapts the next operation from {}mg to {}mg because a larger charging commitment is not currently survivable",
                                desired_mass.milligrams(),
                                mass.milligrams(),
                            );
                        }
                        execute_manual_recovery(
                            registries,
                            &mut state,
                            ids,
                            *option,
                            &mut ScenarioRuntime {
                                variation,
                                delivery_authorization: &mut delivery_authorization,
                                current_support: &mut current_support,
                                alternate_support: &mut alternate_support,
                                report: &mut report,
                            },
                        );
                        if report.structure.structural_stop {
                            break 'work_order;
                        }
                        continue;
                    }
                    ManualRecoverySearch::DeclinedForSurvival => {
                        report.limits.manual_recovery_declined = true;
                        println!(
                            "  manual recovery declined: even the smallest useful charging commitment would cross the player's hunger or thirst warning reserve"
                        );
                    }
                    ManualRecoverySearch::SurvivalLimited => {
                        report.limits.manual_recovery_survival_limited = true;
                        println!(
                            "  manual recovery unavailable: the player lacks enough physiological reserve for another useful charging commitment"
                        );
                    }
                    ManualRecoverySearch::EquipmentUnavailable => {
                        println!(
                            "  manual recovery unavailable: hand-crank condition has reduced usable power to zero"
                        );
                    }
                }
                if report.structure.structural_stop {
                    break 'work_order;
                }
                report.limits.energy_stop = true;
                let reason = if report.limits.manual_recovery_declined {
                    "player preserves survival reserve"
                } else if report.limits.manual_recovery_survival_limited {
                    "player lacks the physiological reserve to generate the missing work"
                } else {
                    "stored work is insufficient and the manual fallback cannot supply the deficit"
                };
                println!("  decision: stop crushing; {reason}");
                break 'work_order;
            };
            let adaptive_batch = resolved_mass < planned_mass;
            if adaptive_batch {
                println!(
                    "  adaptive batching: planned={}mg -> executable={}mg from current condition and stored-work constraints",
                    planned_mass.milligrams(),
                    resolved_mass.milligrams(),
                );
            }
            if let Some(option) = &small {
                print_crush_option(option, thresholds);
            }
            if let Some(option) = &large {
                print_crush_option(option, thresholds);
            } else if !report.choices.large_drive_exhausted {
                report.choices.large_drive_exhausted = true;
                println!(
                    "  power reserve: high-power drive cannot supply the current planned mass"
                );
            }
            match choose_crush_option(
                small,
                large,
                CrushChoiceContext {
                    thresholds,
                    preference: variation.policy.power_preference,
                },
            ) {
                Ok((selected, reason, choice_basis)) => {
                    break (
                        resolved_mass,
                        selected,
                        reason,
                        choice_basis,
                        adaptive_batch,
                    );
                }
                Err(CrushStopReason::EnergyUnavailable) => {
                    unreachable!("largest resolvable batch returned without a viable energy option")
                }
                Err(CrushStopReason::MaintenanceCritical) => {
                    println!(
                        "  decision: service crusher because every available power choice would enter critical condition"
                    );
                    match service_crusher(registries, &mut state, ids, &mut report) {
                        MaintenanceAttempt::Serviced => continue,
                        MaintenanceAttempt::SupplyExhausted => {
                            report.limits.maintenance_stop = true;
                            println!(
                                "  decision: stop crushing; replacement stock is exhausted and every power choice would enter critical condition"
                            );
                            break 'work_order;
                        }
                    }
                }
            }
        };
        match choice_basis {
            PowerChoiceBasis::Policy => report.choices.policy_power_choices += 1,
            PowerChoiceBasis::SingleSource => report.choices.single_source_power_choices += 1,
        }
        println!("  decision: use {} drive because {reason}", selected.name);
        if selected.store == ids.small_drive {
            report.choices.small_drive_batches += 1;
        } else if selected.store == ids.large_drive {
            report.choices.large_drive_batches += 1;
        }
        let outcome = crush_batch(
            registries,
            &mut state,
            ids,
            batch_mass,
            selected,
            report.progress.operations_completed + 1,
            ScenarioRuntime {
                variation,
                delivery_authorization: &mut delivery_authorization,
                current_support: &mut current_support,
                alternate_support: &mut alternate_support,
                report: &mut report,
            },
        );
        if outcome.completed {
            report.progress.processed_mass = report
                .progress
                .processed_mass
                .checked_add(batch_mass)
                .unwrap_or_else(|| panic!("workshop processed-mass accounting overflowed"));
            report.progress.operations_completed = report
                .progress
                .operations_completed
                .checked_add(1)
                .unwrap_or_else(|| panic!("workshop operation count overflowed"));
            if adaptive_batch {
                report.progress.adaptive_batch_operations = report
                    .progress
                    .adaptive_batch_operations
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("adaptive-batch count overflowed"));
            }
        }
        match outcome.bottleneck {
            ComminutionBottleneck::Throughput => {
                report.limits.throughput_bottleneck_batches += 1;
            }
            ComminutionBottleneck::EnergyDelivery => {
                report.limits.energy_bottleneck_batches += 1;
            }
            ComminutionBottleneck::Balanced => {
                report.limits.balanced_bottleneck_batches += 1;
            }
        }
        if !outcome.completed {
            break;
        }
        if !report.progress.delivery_applied
            && state.tick().value() >= variation.delivery.delivery_at_tick
        {
            apply_delivery_and_adapt(
                registries,
                &mut state,
                ids,
                ScenarioRuntime {
                    variation,
                    delivery_authorization: &mut delivery_authorization,
                    current_support: &mut current_support,
                    alternate_support: &mut alternate_support,
                    report: &mut report,
                },
            );
        }
    }
    if !report.progress.delivery_applied {
        let current_tick = state.tick().value();
        if current_tick < variation.delivery.delivery_at_tick {
            println!(
                "  timeline: scenario controller advances from tick={current_tick} to controlled delivery at tick={}",
                variation.delivery.delivery_at_tick
            );
            finish_operation(
                registries,
                &mut state,
                TickSpan::new(variation.delivery.delivery_at_tick - current_tick),
            );
        }
        apply_delivery_and_adapt(
            registries,
            &mut state,
            ids,
            ScenarioRuntime {
                variation,
                delivery_authorization: &mut delivery_authorization,
                current_support: &mut current_support,
                alternate_support: &mut alternate_support,
                report: &mut report,
            },
        );
    }
    let final_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared after gameplay harness crushing"));
    if thresholds.classify(final_condition) != MaintenanceBand::Normal {
        report.limits.maintenance_warning = true;
    }
    let final_hand_crank_condition = state
        .equipment()
        .get_equipment(ids.hand_crank)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("workshop hand crank disappeared"));

    let crushed_mass = state
        .inventory()
        .get_stockpile(ids.crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("crushed storage disappeared"));
    if crushed_mass.is_zero() {
        println!(
            "  material: no crushed output was produced; selected ore remains in source storage or conserved work-in-process"
        );
        println!(
            "  process frontier: mixed-ore melt rejection is not probed because this scenario produced no crushed lot"
        );
    } else {
        let crushed_lot = stockpile_first_lot(&state, ids.crushed_storage);
        let crushed_record = state
            .inventory()
            .get_lot(crushed_lot)
            .unwrap_or_else(|| panic!("crushed output lot disappeared"));
        let particle_distribution = crushed_record
            .particle_size_distribution()
            .unwrap_or_else(|| panic!("canonical crushed ore lost particle-size state"));
        let particle_envelope = crushed_record
            .particle_size()
            .unwrap_or_else(|| panic!("canonical crushed ore lost particle-size envelope"));
        let contained_copper_floor = crushed_record
            .composition()
            .constituent_mass_floor(crushed_mass, MATERIAL_COPPER);
        println!(
            "  material: crushed={}mg composition={}ppm Cu / {}ppm gangue contained_copper_floor={}mg particle_classes={} envelope={}..={}um",
            crushed_mass.milligrams(),
            variation.ore.ore_copper_ppm,
            1_000_000 - variation.ore.ore_copper_ppm,
            contained_copper_floor.milligrams(),
            particle_distribution.classes().len(),
            particle_envelope.minimum_diameter().micrometers(),
            particle_envelope.maximum_diameter().micrometers(),
        );
        println!(
            "  value state: ore grade changes conserved contained copper, but it cannot yet change a downstream production choice because concentration/smelting is unresolved"
        );
        if particle_distribution.classes().len() == 1 {
            println!(
                "  preparation state: crusher output is one unresolved size class; a screen cut through that class cannot claim a fabricated yield"
            );
        }
        let mixed_selection = [MaterialLotSelection::new(
            crushed_lot,
            Mass::from_milligrams(1),
        )];
        let blocked_melt = resolve_melting_process(
            registries,
            &state,
            MeltingRequest::new(
                PROCESS_MELT_PURE_COPPER,
                ids.crushed_storage,
                &mixed_selection,
                ids.furnace,
                ids.electrical_buffer,
            ),
        );
        report.progress.ore_frontier_visible = matches!(
            blocked_melt,
            Err(MeltingResolutionError::Batch(
                MeltingBatchError::ImpureInput {
                    commodity: _commodity,
                }
            ))
        );
        println!(
            "  process frontier: crushed mixed ore cannot enter pure-copper melting={} (concentration/smelting remains the missing bridge)",
            report.progress.ore_frontier_visible
        );
    }

    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(initial_matter),
        "gameplay workshop must conserve matter across production, relocation, and maintenance"
    );
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    let small_remaining = state
        .energy()
        .get_store(ids.small_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("small mechanical drive disappeared"));
    let large_remaining = state
        .energy()
        .get_store(ids.large_drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("large mechanical drive disappeared"));
    let active_support = state
        .structures()
        .get_element(current_support)
        .unwrap_or_else(|| panic!("active workshop support disappeared"));
    let maintenance_remaining = state
        .inventory()
        .get_stockpile(ids.maintenance_source)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("maintenance replacement stockpile disappeared"));
    let survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("workshop player survival state disappeared"));
    let physiology = registries.survival().physiology();
    let metabolic_energy_spent = physiology
        .maximum_metabolic_energy()
        .checked_sub(survival.metabolic_energy())
        .unwrap_or_else(|| panic!("workshop metabolic reserve exceeded authored maximum"));
    let hydration_spent = physiology
        .maximum_hydration()
        .checked_sub(survival.hydration())
        .unwrap_or_else(|| panic!("workshop hydration reserve exceeded authored maximum"));
    report.resources.final_condition_ppm = final_condition.parts_per_million();
    report.resources.small_drive_remaining = small_remaining;
    report.resources.large_drive_remaining = large_remaining;
    report.resources.maintenance_stock_remaining = maintenance_remaining;
    report.resources.elapsed_ticks = state.tick().value();
    report.resources.metabolic_energy_spent = metabolic_energy_spent;
    report.resources.hydration_spent = hydration_spent;
    report.resources.final_vitality_ppm = survival.vitality().parts_per_million();
    report.resources.final_hand_crank_condition_ppm =
        final_hand_crank_condition.parts_per_million();
    println!(
        "  outcome: ore={}/{}mg operations={} adaptive={} before_event={} choices=[policy:{} single-source:{} manual-recharges:{}] suspended={} stranded_wip={} equipment=[crusher:{}ppm/{:?} crank:{}ppm] maintenance=[services:{} spent:{}mg remaining:{}mg] mechanical_reserve=[small:{}nJ high-power:{}nJ] manual_generation=[energy:{}nJ ticks:{} body:{}nJ/{}uL] survival=[total-energy:-{}nJ total-hydration:-{}uL vitality:{}ppm] active_support={:?}/cracked:{} ticks={}",
        report.progress.processed_mass.milligrams(),
        report.progress.target_mass.milligrams(),
        report.progress.operations_completed,
        report.progress.adaptive_batch_operations,
        report.progress.operations_before_delivery,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.choices.manual_recharges,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        final_condition.parts_per_million(),
        thresholds.classify(final_condition),
        final_hand_crank_condition.parts_per_million(),
        report.maintenance.services,
        report.maintenance.replacement_spent.milligrams(),
        maintenance_remaining.milligrams(),
        small_remaining.nanojoules(),
        large_remaining.nanojoules(),
        report.resources.manually_generated_energy.nanojoules(),
        report.resources.manual_power_ticks,
        report.resources.manual_power_metabolic_energy.nanojoules(),
        report.resources.manual_power_hydration.microliters(),
        metabolic_energy_spent.nanojoules(),
        hydration_spent.microliters(),
        survival.vitality().parts_per_million(),
        active_support.lifecycle(),
        active_support.is_cracked(),
        state.tick().value(),
    );
    println!(
        "  report: structural_change={} damage_debt={} support_block={} relocation={} structural_stop={} production_suspension={} stranded_wip={} machine_ops=[small:{} large:{}] manual_recharges={} power_choices=[policy:{} single-source:{}] bottlenecks=[energy:{} throughput:{} balanced:{}] maintenance_warning={} maintenance_services={} maintenance_supply_exhausted={} stops=[maintenance:{} energy:{} recovery_declined:{} recovery_survival_limited:{}] ore_frontier={}",
        report.structure.structural_consequence,
        report.structure.structural_damage_debt,
        report.structure.support_failure_blocked_production,
        report.structure.support_relocation,
        report.structure.structural_stop,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        report.choices.small_drive_batches,
        report.choices.large_drive_batches,
        report.choices.manual_recharges,
        report.choices.policy_power_choices,
        report.choices.single_source_power_choices,
        report.limits.energy_bottleneck_batches,
        report.limits.throughput_bottleneck_batches,
        report.limits.balanced_bottleneck_batches,
        report.limits.maintenance_warning,
        report.maintenance.services,
        report.maintenance.supply_exhausted,
        report.limits.maintenance_stop,
        report.limits.energy_stop,
        report.limits.manual_recovery_declined,
        report.limits.manual_recovery_survival_limited,
        report.progress.ore_frontier_visible,
    );
    report
}

fn agency_probe_policies() -> [(&'static str, ScenarioPolicyVariation); 4] {
    [
        (
            "conservative",
            ScenarioPolicyVariation {
                power_preference: PowerPreference::PreserveReserve,
                energy_recovery_preference: EnergyRecoveryPreference::ProtectSurvival,
                maintenance_preference: MaintenancePreference::ServiceAtWarning,
                structural_preference: StructuralPreference::PreserveMargin,
            },
        ),
        (
            "throughput-reactive",
            ScenarioPolicyVariation {
                power_preference: PowerPreference::FinishSooner,
                energy_recovery_preference: EnergyRecoveryPreference::SpendSurvivalReserve,
                maintenance_preference: MaintenancePreference::ServiceAtCritical,
                structural_preference: StructuralPreference::MoveOnlyForFailure,
            },
        ),
        (
            "equipment-care",
            ScenarioPolicyVariation {
                power_preference: PowerPreference::FinishSooner,
                energy_recovery_preference: EnergyRecoveryPreference::ProtectSurvival,
                maintenance_preference: MaintenancePreference::ServiceAtWarning,
                structural_preference: StructuralPreference::PreserveMargin,
            },
        ),
        (
            "reserve-reactive",
            ScenarioPolicyVariation {
                power_preference: PowerPreference::PreserveReserve,
                energy_recovery_preference: EnergyRecoveryPreference::SpendSurvivalReserve,
                maintenance_preference: MaintenancePreference::ServiceAtCritical,
                structural_preference: StructuralPreference::MoveOnlyForFailure,
            },
        ),
    ]
}

fn run_agency_probe(registries: &Registries, world_seeds: &[u64]) {
    let policies = agency_probe_policies();
    let mut worlds_with_distinct_paths = 0_usize;
    let mut worlds_with_work_difference = 0_usize;
    for &world_seed in world_seeds.iter().take(2) {
        let mut reports = Vec::with_capacity(policies.len());
        for (index, (label, policy)) in policies.into_iter().enumerate() {
            let behavior_seed = mix64(
                world_seed
                    ^ 0xA63E_4E43_5900_0000
                    ^ u64::try_from(index + 1)
                        .unwrap_or_else(|_| unreachable!("agency policy index fits u64")),
            );
            let mut variation =
                ScenarioVariation::from_seeds(registries, world_seed, behavior_seed, None);
            variation.policy = policy;
            let report = run_scenario(registries, variation);
            reports.push((label, report));
        }

        let processed_min = reports
            .iter()
            .map(|(_, report)| report.progress.processed_mass.milligrams())
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let processed_max = reports
            .iter()
            .map(|(_, report)| report.progress.processed_mass.milligrams())
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let high_power_min = reports
            .iter()
            .map(|(_, report)| report.choices.large_drive_batches)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let high_power_max = reports
            .iter()
            .map(|(_, report)| report.choices.large_drive_batches)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let service_min = reports
            .iter()
            .map(|(_, report)| report.maintenance.services)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let service_max = reports
            .iter()
            .map(|(_, report)| report.maintenance.services)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let elapsed_min = reports
            .iter()
            .map(|(_, report)| report.resources.elapsed_ticks)
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let elapsed_max = reports
            .iter()
            .map(|(_, report)| report.resources.elapsed_ticks)
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let survival_energy_min = reports
            .iter()
            .map(|(_, report)| report.resources.metabolic_energy_spent.nanojoules())
            .min()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let survival_energy_max = reports
            .iter()
            .map(|(_, report)| report.resources.metabolic_energy_spent.nanojoules())
            .max()
            .unwrap_or_else(|| unreachable!("agency probe policy set is nonempty"));
        let signatures = reports
            .iter()
            .map(|(_, report)| {
                format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    report.progress.processed_mass.milligrams(),
                    report.progress.operations_completed,
                    report.progress.adaptive_batch_operations,
                    report.choices.small_drive_batches,
                    report.choices.large_drive_batches,
                    report.choices.manual_recharges,
                    report.maintenance.services,
                    u8::from(report.structure.support_relocation),
                    u8::from(report.structure.production_suspension),
                    u8::from(report.structure.stranded_work_in_process),
                    u8::from(report.structure.structural_stop),
                    report.choices.policy_power_choices,
                    report.choices.single_source_power_choices,
                    report.resources.final_condition_ppm,
                    report.resources.small_drive_remaining.nanojoules(),
                    report.resources.large_drive_remaining.nanojoules(),
                    report.resources.maintenance_stock_remaining.milligrams(),
                    report.resources.elapsed_ticks,
                    report.resources.metabolic_energy_spent.nanojoules(),
                    report.resources.manual_power_metabolic_energy.nanojoules(),
                )
            })
            .collect::<BTreeSet<_>>();
        if signatures.len() > 1 {
            worlds_with_distinct_paths += 1;
        }
        if processed_min != processed_max {
            worlds_with_work_difference += 1;
        }
        let policy_paths = reports
            .iter()
            .map(|(label, report)| {
                format!(
                    "{label}:ore{}/{}-ops{}-adapt{}-hi{}-manual{}-maint{}-reloc{}-susp{}-choices[p:{} f:{}]-t{}-body{}-manualbody{}-c{}-lo{}-hi{}",
                    report.progress.processed_mass.milligrams(),
                    report.progress.target_mass.milligrams(),
                    report.progress.operations_completed,
                    report.progress.adaptive_batch_operations,
                    report.choices.large_drive_batches,
                    report.choices.manual_recharges,
                    report.maintenance.services,
                    u8::from(report.structure.support_relocation),
                    u8::from(report.structure.production_suspension),
                    report.choices.policy_power_choices,
                    report.choices.single_source_power_choices,
                    report.resources.elapsed_ticks,
                    report.resources.metabolic_energy_spent.nanojoules(),
                    report.resources.manual_power_metabolic_energy.nanojoules(),
                    report.resources.final_condition_ppm,
                    report.resources.small_drive_remaining.nanojoules(),
                    report.resources.large_drive_remaining.nanojoules(),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        std::println!(
            "AGENCY world=0x{world_seed:016X} variants={} unique_paths={} processed={}..{}mg high-power-ops={}..{} services={}..{} elapsed={}..{}t survival-energy={}..{}nJ paths=[{}]",
            reports.len(),
            signatures.len(),
            processed_min,
            processed_max,
            high_power_min,
            high_power_max,
            service_min,
            service_max,
            elapsed_min,
            elapsed_max,
            survival_energy_min,
            survival_energy_max,
            policy_paths,
        );
    }
    std::println!(
        "AGENCY SUMMARY worlds={} distinct_paths={} processed-work-differences={} basis=matched-world-policy-counterfactual",
        world_seeds.len().min(2),
        worlds_with_distinct_paths,
        worlds_with_work_difference,
    );
}

fn foundry_probe_mass(registries: &Registries, seed: u64) -> Mass {
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical melting definition disappeared"));
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("canonical casting definition disappeared"));
    let melt_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_ELECTRIC_FURNACE,
        melting.max_batch_mass_capability(),
    );
    let cast_maximum = nominal_equipment_mass_capability(
        registries,
        EQUIPMENT_CASTING_MOLD,
        casting.max_batch_mass_capability(),
    );
    let maximum = melt_maximum.milligrams().min(cast_maximum.milligrams());
    assert!(maximum > 0, "foundry probe requires a nonzero legal batch");
    let minimum = maximum.div_ceil(2);
    Mass::from_milligrams(minimum + mix64(seed ^ 0xF0A1_DA7A) % (maximum - minimum + 1))
}

fn ore_preparation_probe_parameters(registries: &Registries, seed: u64) -> (Mass, u32) {
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
    (batch_mass, copper_ppm)
}

fn run_foundry_capability_probe(registries: &Registries, seed: u64) {
    let mass = foundry_probe_mass(registries, seed);
    let (mut state, ids) = setup_foundry_probe(registries, seed, mass);
    println!(
        "\nDOWNSTREAM CAPABILITY PROBE: pure-copper melt/cast is validated separately from the ore workshop loop; this is not presented as ore-to-metal progression"
    );
    let pure_selection = [MaterialLotSelection::new(ids.pure_copper_lot, mass)];
    let melt = resolve_melting_process(
        registries,
        &state,
        MeltingRequest::new(
            PROCESS_MELT_PURE_COPPER,
            ids.pure_copper_source,
            &pure_selection,
            ids.furnace,
            ids.electrical_buffer,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry probe pure-copper melt failed: {error}"));
    let melt_duration = melt.process_resolution().duration();
    println!(
        "  melt: mass={}mg energy={}nJ power={}uW duration={}t",
        mass.milligrams(),
        melt.required_energy().nanojoules(),
        melt.transfer_power().whole_microwatts(),
        melt_duration.value(),
    );
    validate_start_process(
        registries,
        &state,
        melt.process_resolution(),
        ids.pure_copper_source,
        ids.molten_vessel,
    )
    .unwrap_or_else(|error| panic!("foundry probe melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe melt commit failed: {error}"));
    finish_operation(registries, &mut state, melt_duration);

    let molten_lot = stockpile_first_lot(&state, ids.molten_vessel);
    let molten_selection = [MaterialLotSelection::new(molten_lot, mass)];
    let casting = resolve_casting_process(
        registries,
        &state,
        CastingRequest::new(
            PROCESS_CAST_PURE_COPPER,
            ids.molten_vessel,
            &molten_selection,
            ids.mold,
            ids.heat_sink,
        ),
    )
    .unwrap_or_else(|error| panic!("foundry probe pure-copper casting failed: {error}"));
    let cast_duration = casting.process_resolution().duration();
    println!(
        "  cast: released={}nJ power={}uW duration={}t",
        casting.released_energy().nanojoules(),
        casting.transfer_power().whole_microwatts(),
        cast_duration.value(),
    );
    validate_start_process(
        registries,
        &state,
        casting.process_resolution(),
        ids.molten_vessel,
        ids.cast_storage,
    )
    .unwrap_or_else(|error| panic!("foundry probe casting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("foundry probe casting commit failed: {error}"));
    finish_operation(registries, &mut state, cast_duration);
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    let cast_mass_is_conserved = state
        .inventory()
        .get_stockpile(ids.cast_storage)
        .is_some_and(|stockpile| stockpile.stored_mass() == mass);
    assert!(
        cast_mass_is_conserved,
        "foundry capability probe did not conserve cast output mass"
    );
    std::println!(
        "FOUNDRY seed=0x{seed:016X} batch={}mg melt={}t cast={}t matter=conserved",
        mass.milligrams(),
        melt_duration.value(),
        cast_duration.value(),
    );
}

fn run_ore_preparation_capability_probe(registries: &Registries, seed: u64) {
    let (batch_mass, copper_ppm) = ore_preparation_probe_parameters(registries, seed);
    let (mut state, ids) = setup_ore_preparation_probe(registries, seed, batch_mass, copper_ppm);
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
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation initial matter accounting failed: {error}"))
        .total();
    let initial_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared"));

    println!(
        "\nORE PREPARATION CAPABILITY PROBE: batch={}mg ore={}ppm Cu; batch size is derived from authored equipment limits and screen representability",
        batch_mass.milligrams(),
        copper_ppm,
    );
    let crush_selection = [MaterialLotSelection::new(ids.ore_lot, batch_mass)];
    let crushed = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &crush_selection,
            ids.crusher,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical crushing probe resolution failed: {error}"));
    let crush_duration = crushed.process_resolution().duration();
    println!(
        "  crush: energy={}nJ rate={}mg/s duration={}t bottleneck={:?}",
        crushed.required_energy().nanojoules(),
        crushed.processing_rate().milligrams_per_second(),
        crush_duration.value(),
        crushed.bottleneck(),
    );
    let crush_energy = crushed.required_energy();
    let crusher_condition = crushed.condition_after();
    validate_start_process(
        registries,
        &state,
        crushed.process_resolution(),
        ids.ore_source,
        ids.crushed_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation crushing start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation crushing commit failed: {error}"));
    finish_operation(registries, &mut state, crush_duration);
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));

    let crushed_lot = stockpile_first_lot(&state, ids.crushed_storage);
    let crushed_distribution = state
        .inventory()
        .get_lot(crushed_lot)
        .and_then(|lot| lot.particle_size_distribution())
        .unwrap_or_else(|| panic!("canonical crushing output lost particle-size state"));
    let crusher_output_matches_authoring =
        crushed_distribution == crusher_definition.output_particle_size_distribution();
    let direct_screen_selection = [MaterialLotSelection::new(crushed_lot, batch_mass)];
    let direct_screen = resolve_screening_process(
        registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ids.crushed_storage,
            &direct_screen_selection,
            ids.screen,
            ids.drive,
        ),
    );
    let direct_screen_status = match &direct_screen {
        Ok(_) => "available",
        Err(ScreeningResolutionError::Batch(ScreeningBatchError::UnresolvedParticleClass {
            aperture: _aperture,
            class: _class,
        })) => "requires-classification",
        Err(error) => panic!("direct-screen route failed unexpectedly: {error}"),
    };
    let direct_fine_grind = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            ids.crushed_storage,
            &direct_screen_selection,
            ids.grinder,
            ids.drive,
        ),
    );
    let direct_fine_grind_status = match &direct_fine_grind {
        Ok(_) => "available",
        Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange {
                required: _required,
                found: _found,
            },
        )) => "outside-authored-feed-range",
        Err(error) => panic!("direct fine-grind route failed unexpectedly: {error}"),
    };
    println!(
        "  route discovery: crusher->screen={direct_screen_status} crusher->fine_regrind={direct_fine_grind_status}; these are observations, not fixed harness requirements"
    );

    let grind_selection = [MaterialLotSelection::new(crushed_lot, batch_mass)];
    let ground = resolve_comminution_process(
        registries,
        &state,
        ComminutionRequest::new(
            PROCESS_GRIND_CRUSHED_ORE,
            ids.crushed_storage,
            &grind_selection,
            ids.grinder,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical grinding probe resolution failed: {error}"));
    let grind_duration = ground.process_resolution().duration();
    println!(
        "  grind: energy={}nJ rate={}mg/s duration={}t bottleneck={:?}",
        ground.required_energy().nanojoules(),
        ground.processing_rate().milligrams_per_second(),
        grind_duration.value(),
        ground.bottleneck(),
    );
    let grind_energy = ground.required_energy();
    let grinder_condition = ground.condition_after();
    validate_start_process(
        registries,
        &state,
        ground.process_resolution(),
        ids.crushed_storage,
        ids.ground_storage,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinding start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation grinding commit failed: {error}"));
    finish_operation(registries, &mut state, grind_duration);
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    assert_eq!(
        state
            .equipment()
            .get_equipment(ids.grinder)
            .map(|equipment| equipment.condition()),
        Some(grinder_condition)
    );

    let ground_lot = stockpile_first_lot(&state, ids.ground_storage);
    let ground_distribution = state
        .inventory()
        .get_lot(ground_lot)
        .and_then(|lot| lot.particle_size_distribution())
        .cloned()
        .unwrap_or_else(|| panic!("canonical grinding output lost particle-size state"));
    let ground_classes = ground_distribution.classes();
    let grinding_matches_authoring =
        &ground_distribution == grinder_definition.output_particle_size_distribution();
    let grinding_resolved_screen_cut = ground_classes.iter().all(|class| {
        class.range().maximum_diameter() <= screen_definition.aperture()
            || class.range().minimum_diameter() > screen_definition.aperture()
    });

    let screen_selection = [MaterialLotSelection::new(ground_lot, batch_mass)];
    let screened = resolve_screening_process(
        registries,
        &state,
        ScreeningRequest::new(
            PROCESS_SCREEN_CRUSHED_ORE,
            ids.ground_storage,
            &screen_selection,
            ids.screen,
            ids.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("canonical screening probe resolution failed: {error}"));
    let screen_duration = screened.process_resolution().duration();
    println!(
        "  screen: undersize={}mg oversize={}mg energy={}nJ rate={}mg/s duration={}t bottleneck={:?}",
        screened.undersize_mass().milligrams(),
        screened.oversize_mass().milligrams(),
        screened.required_energy().nanojoules(),
        screened.processing_rate().milligrams_per_second(),
        screen_duration.value(),
        screened.bottleneck(),
    );
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
    validate_start_process_routed(
        registries,
        &state,
        screened.process_resolution(),
        ids.ground_storage,
        &routes,
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_operation(registries, &mut state, screen_duration);
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));

    let output_composition = mixed_ore_composition(copper_ppm);
    let fine_output_fits_undersize = fine_grind_definition
        .output_particle_size_distribution()
        .classes()
        .iter()
        .all(|class| class.range().maximum_diameter() <= screen_definition.aperture());
    let (fine_energy, fine_duration_ticks, final_grinder_projection, oversize_profile_is_preserved) =
        if screened_oversize_mass.is_zero() {
            println!(
                "  regrind oversize: skipped because the authored screen produced no oversize"
            );
            (Energy::ZERO, 0, grinder_condition, true)
        } else {
            let oversize_lot = stockpile_first_lot(&state, ids.oversize_storage);
            let oversize_before_regrind = state
                .inventory()
                .get_lot(oversize_lot)
                .unwrap_or_else(|| panic!("ore preparation oversize lot disappeared"));
            let oversize_profile_is_preserved = oversize_before_regrind.composition()
                == &output_composition
                && oversize_before_regrind
                    .particle_size_distribution()
                    .is_some_and(|distribution| {
                        distribution.classes().iter().all(|class| {
                            class.range().minimum_diameter() > screen_definition.aperture()
                                && ground_classes.contains(class)
                        })
                    });
            let fine_selection = [MaterialLotSelection::new(
                oversize_lot,
                screened_oversize_mass,
            )];
            let fine_ground = resolve_comminution_process(
                registries,
                &state,
                ComminutionRequest::new(
                    PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
                    ids.oversize_storage,
                    &fine_selection,
                    ids.grinder,
                    ids.drive,
                ),
            )
            .unwrap_or_else(|error| {
                panic!("canonical fine-grinding probe resolution failed: {error}")
            });
            let fine_duration = fine_ground.process_resolution().duration();
            println!(
                "  regrind oversize: mass={}mg energy={}nJ rate={}mg/s duration={}t bottleneck={:?}",
                screened_oversize_mass.milligrams(),
                fine_ground.required_energy().nanojoules(),
                fine_ground.processing_rate().milligrams_per_second(),
                fine_duration.value(),
                fine_ground.bottleneck(),
            );
            let fine_energy = fine_ground.required_energy();
            let final_grinder_projection = fine_ground.condition_after();
            validate_start_process(
                registries,
                &state,
                fine_ground.process_resolution(),
                ids.oversize_storage,
                ids.undersize_storage,
            )
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("ore preparation fine-grinding commit failed: {error}"));
            finish_operation(registries, &mut state, fine_duration);
            assert_eq!(validate_loaded_state(registries, &state), Ok(()));
            (
                fine_energy,
                fine_duration.value(),
                final_grinder_projection,
                oversize_profile_is_preserved,
            )
        };

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
    let composition_preserved = state.inventory().lot_ids(ids.undersize_storage).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .is_some_and(|lot| lot.composition() == &output_composition)
    });
    let final_distribution_is_fine = state.inventory().lot_ids(ids.undersize_storage).all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .and_then(|lot| lot.particle_size_distribution())
            .is_some_and(|distribution| {
                distribution
                    .classes()
                    .iter()
                    .all(|class| class.range().maximum_diameter() <= screen_definition.aperture())
            })
    });
    let consumed_energy = crush_energy
        .checked_add(grind_energy)
        .and_then(|energy| energy.checked_add(screen_energy))
        .and_then(|energy| energy.checked_add(fine_energy))
        .unwrap_or_else(|| panic!("ore preparation consumed energy overflowed"));

    let requirements = [
        (
            "ore preparation conserves matter",
            final_matter == initial_matter,
        ),
        (
            "ore preparation consumes exactly resolved work energy",
            initial_energy.checked_sub(consumed_energy) == Some(final_energy),
        ),
        (
            "crusher condition matches resolved wear",
            final_crusher_condition == crusher_condition,
        ),
        (
            "grinder condition matches resolved wear",
            final_grinder_condition == final_grinder_projection,
        ),
        (
            "screen condition matches resolved wear",
            final_screen_condition == screen_condition,
        ),
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
            "ore preparation preserves composition",
            composition_preserved,
        ),
        (
            "final product satisfies the fine size range",
            final_distribution_is_fine,
        ),
        (
            "all prepared mass finishes in undersize storage",
            undersize_mass == batch_mass,
        ),
        (
            "oversize storage is empty after regrind",
            oversize_mass == Mass::ZERO,
        ),
    ];

    for (name, observed) in requirements {
        assert!(
            observed,
            "ore-preparation capability contract failed: {name}"
        );
    }
    std::println!(
        "ORE_PREP seed=0x{seed:016X} batch={}mg copper={}ppm stages=[crush:{}t grind:{}t screen:{}t regrind:{}t] matter=conserved energy=resolved",
        batch_mass.milligrams(),
        copper_ppm,
        crush_duration.value(),
        grind_duration.value(),
        screen_duration.value(),
        fine_duration_ticks,
    );
}

/// Runs the headless workshop scenario matrix with optional exploratory capability output.
fn run_gameplay_harness(mode: ScenarioPlanMode, include_probes: bool) {
    let registries = build_registries();
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let behavior_raw = env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok();
    let mode_salt = match mode {
        ScenarioPlanMode::Gate => 0x4741_5445_5EED_2026,
        ScenarioPlanMode::Explore => 0x4558_504C_5EED_2026,
    };
    let exploration_world_root = fresh_exploration_root(MAINTAINED_VARIATION_ROOT ^ mode_salt);
    let exploration_behavior_root = fresh_exploration_root(
        MAINTAINED_BEHAVIOR_ROOT ^ mode_salt.rotate_left(17) ^ 0xB3A4_7102_5EED_2026,
    );
    let plan = scenario_seeds_from(
        mode,
        scenario_raw.as_deref(),
        variation_raw.as_deref(),
        behavior_raw.as_deref(),
        exploration_world_root,
        exploration_behavior_root,
    )
    .unwrap_or_else(|error| panic!("gameplay harness configuration failed: {error:?}"));
    std::println!(
        "HARNESS INPUT plan={} anchors={} variation={} custom={} world_root={} behavior_root={} replay={}",
        plan.source_label(),
        plan.anchor_seed_count(),
        plan.variation_seed_count(),
        plan.custom_seed_count(),
        plan.variation_label(),
        plan.behavior_label(),
        plan.replay_label(),
    );
    print_content_summary(&registries, include_probes || has_verbose_output());
    std::println!(
        "PLAYER LOOP early=[survive->shape-tools->mine->reinforce->store-work->mechanize] workshop=[site-machine->process-total-mass->adapt-batch-to-condition+stored-work->choose-power->hand-charge-or-protect-survival->react-to-world-load->maintain-or-relocate->iterate] utility=[survival-reserve,machine-condition,structural-margin,stored-work,time] frontier=[industrial acquisition,power generation,mixed-ore concentration/smelting]"
    );
    let probe_seed = plan
        .cases()
        .iter()
        .fold(0xD33F_C01D_5EED_u64, |combined, case| {
            mix64(combined ^ case.world_seed ^ case.behavior_seed.rotate_left(17))
        });
    println!(
        "\n=== DEEP HEARTH WORKSHOP GAMEPLAY HARNESS: {} scenario(s), registry schema {} ===",
        plan.cases().len(),
        registries.schema_version().value(),
    );
    println!(
        "SETUP BOUNDARY: starting matter, equipment, finite energy, structural bays, background stored cargo, and one single-use delivery authorization are arranged before the actor starts. The controlled event later consumes that authorization through canonical inventory validation/commit; the actor receives no future event tick or target."
    );
    println!(
        "WORKSHOP FANTASY: turn a constrained physical workshop into reliable production by reading structural margin, uneven stored work, machine condition, material state, and personal survival reserve. Use residual work instead of discarding it, fall back to direct labor when worth the bodily cost, and recover when the world changes."
    );
    println!(
        "LOOP SCOPE: each workshop has a total ore work order rather than a required fixed batch count. Fresh cases vary uneven finite stored work, replacement stock, condition, support state, and player priorities. The actor uses canonical projections to resize operations, choose power, decide whether manual generation is survivable, and react after one hidden preauthorized supported-stockpile event changes the world. No logistics scheduler or industrial acquisition path is implied. Separate probes exercise reachable primitive progression, survival provisioning, ore preparation, and current downstream foundry capabilities."
    );

    let anchor_seed_count = plan.anchor_seed_count();
    let reports: Vec<_> = plan
        .cases()
        .iter()
        .copied()
        .enumerate()
        .map(|(index, case)| {
            ScenarioVariation::from_seeds(
                &registries,
                case.world_seed,
                case.behavior_seed,
                (index < anchor_seed_count).then_some(index),
            )
        })
        .map(|variation| run_scenario(&registries, variation))
        .collect();
    assert_scenario_contracts(&reports);
    if plan.anchor_seed_count() > 0 {
        assert_anchor_diversity(&reports[..plan.anchor_seed_count()]);
    }
    if include_probes {
        let mut agency_worlds = Vec::new();
        if plan.anchor_seed_count() > 0 {
            agency_worlds.push(plan.cases()[plan.anchor_seed_count() - 1].world_seed);
        }
        let variation_world = plan
            .cases()
            .iter()
            .skip(plan.anchor_seed_count())
            .filter(|case| !agency_worlds.contains(&case.world_seed))
            .min_by_key(|case| {
                let variation = ScenarioVariation::from_seeds(
                    &registries,
                    case.world_seed,
                    case.behavior_seed,
                    None,
                );
                let stored_work_ppm = (u64::from(variation.crusher.small_drive_batch_budget)
                    + u64::from(variation.crusher.large_drive_batch_budget))
                    * 1_000_000
                    + u64::from(variation.crusher.small_drive_partial_batch_ppm)
                    + u64::from(variation.crusher.large_drive_partial_batch_ppm);
                let order_batches = variation
                    .ore
                    .order_mass
                    .milligrams()
                    .div_ceil(variation.ore.nominal_batch_mass.milligrams());
                stored_work_ppm
                    .saturating_mul(1_000_000)
                    .checked_div(order_batches.saturating_mul(1_000_000))
                    .unwrap_or(u64::MAX)
            })
            .map(|case| case.world_seed);
        if let Some(variation_world) = variation_world {
            agency_worlds.push(variation_world);
        }
        if agency_worlds.is_empty() {
            agency_worlds.extend(plan.cases().iter().map(|case| case.world_seed).take(2));
        }
        run_agency_probe(&registries, &agency_worlds);
        run_survival_provisioning_probe(&registries, probe_seed ^ 0x5355_5256_4956_414C);
        run_primitive_progression_probe(&registries, probe_seed ^ 0x5052_4F47_5245_5353);
        run_ore_preparation_capability_probe(&registries, probe_seed);
        run_foundry_capability_probe(&registries, probe_seed);
    }
    let evidence_mode = match mode {
        ScenarioPlanMode::Gate => "controlled",
        ScenarioPlanMode::Explore => "exploratory",
    };
    print_harness_summary(evidence_mode, &reports, include_probes);
}

#[test]
fn gameplay_harness_gate() {
    run_gameplay_harness(ScenarioPlanMode::Gate, false);
}

#[test]
fn gameplay_ore_preparation_probe() {
    run_focused_probe(
        "ore-preparation",
        0xD33F_C01D_0A11,
        0x0AE5_1A5E_5052_4F42,
        run_ore_preparation_capability_probe,
    );
}

#[test]
fn gameplay_primitive_progression_probe() {
    run_focused_probe(
        "primitive-progression",
        0xD33F_C01D_5052,
        0x5052_4F47_5052_4F42,
        run_primitive_progression_probe,
    );
}

#[test]
fn gameplay_foundry_probe() {
    run_focused_probe(
        "foundry",
        0xD33F_C01D_F001,
        0xF0A1_DA7A_5052_4F42,
        run_foundry_capability_probe,
    );
}

#[test]
fn gameplay_survival_provisioning_probe() {
    run_focused_probe(
        "survival-provisioning",
        0xD33F_C01D_5A70,
        0x5355_5256_5052_4F42,
        run_survival_provisioning_probe,
    );
}

#[test]
#[ignore = "exploratory gameplay report"]
fn gameplay_harness_exploratory_report() {
    run_gameplay_harness(ScenarioPlanMode::Explore, true);
}
