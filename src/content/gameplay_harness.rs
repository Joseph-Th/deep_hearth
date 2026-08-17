//! Headless workshop gameplay harness over the same canonical content registries used by the game.
//!
//! The harness deliberately varies physical initial conditions and player priorities, then lets a
//! small operational policy react only to observed state and resolver projections. Normal
//! exercise-mode runs combine a deterministic experience-coverage matrix with a small replayable set
//! of organic exploratory scenarios. Their physical ranges are derived from the current authored
//! content rather than copied balance constants.
//! `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces one organic scenario set from an exact decimal or
//! `0x` hexadecimal root seed. The scenario can announce a future snow
//! load and then inject the actual load at the announced tick. This is an external harness stimulus,
//! not an implemented weather or forecasting subsystem. Faster machinery can therefore change how
//! much work is secured before an uncertain external condition changes.
//! `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the whole matrix with an exact comma-separated decimal or
//! `0x` hexadecimal seed list; malformed entries are rejected instead of ignored. Detailed trace
//! output is opt-in via `DEEP_HEARTH_GAMEPLAY_VERBOSE`.

use std::env;

mod bootstrap;
mod configuration;
mod coverage;
mod probe_setup;
mod seed;

use bootstrap::{
    materialize_structure, seed_composed_lot, seed_energy_store as bootstrap_seed_energy_store,
    seed_lot,
};
use configuration::{ScenarioSeedPlan, configuration_contract_gaps, scenario_seeds};
use coverage::{coverage_gaps, scenario_contract_gaps};
use probe_setup::{setup_foundry_probe, setup_ore_preparation_probe};
use seed::mix64;

const HARNESS_MODE: &str = "exercise";

fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
}

macro_rules! println {
    ($($argument:tt)*) => {{
        if has_verbose_output() {
            std::println!($($argument)*);
        }
    }};
}

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{EnergyStoreId, EnergySupplyError, calculate_mass_specific_energy};
use crate::equipment::{
    EquipmentDefinitionId, EquipmentId, EquipmentMaintenanceRequest,
    EquipmentMaintenanceResolutionError, EquipmentProviderError, EquipmentSupportError,
    add_equipment, resolve_equipment_maintenance, validate_equipment_repair,
    validate_mount_equipment, validate_unmount_equipment,
};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile, add_stockpile,
};
use crate::maintenance::{CONDITION_PARTS_PER_MILLION, Condition, MaintenanceBand};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::ore_processing::{
    ComminutionBatchError, ComminutionBottleneck, ComminutionRequest, ComminutionResolutionError,
    ResolvedComminution, ScreeningBatchError, ScreeningProcessDefinition, ScreeningRequest,
    ScreeningResolutionError, resolve_comminution_process, resolve_screening_process,
};
use crate::production::{
    ProcessOutputRoute, ProductionAvailabilityChange, ProductionJobId, ProductionSuspensionReason,
    validate_start_process, validate_start_process_routed,
};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralAssessment, StructuralElementGeometry,
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, StructuralLoadMode,
    StructuralStage, add_structural_element, analyze_structure, calculate_weight_force_ceiling,
    validate_activate_structural_element, validate_set_structural_load,
};
use crate::thermal::{
    CastingRequest, MeltingBatchError, MeltingRequest, MeltingResolutionError,
    resolve_casting_process, resolve_melting_process,
};

use super::energy::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_THERMAL_SINK,
};
use super::equipment::{
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER,
};
use super::processes::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
};
use super::{
    FORM_INGOT, FORM_LOG, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

#[derive(Clone, Copy, Debug)]
struct ScenarioVariation {
    seed: u64,
    ore: ScenarioOreVariation,
    crusher: ScenarioCrusherVariation,
    structure: ScenarioStructureVariation,
    stimulus: ScenarioStimulusVariation,
    policy: ScenarioPolicyVariation,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioOreVariation {
    ore_copper_ppm: u32,
    batch_mass: Mass,
    planned_batches: u8,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioCrusherVariation {
    initial_crusher_condition: Condition,
    large_drive_batch_budget: u8,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioStructureVariation {
    compact_support_area: Area,
    reinforced_support_area: Area,
    reinforced_background_load: Force,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioStimulusVariation {
    briefed_snow_load: Force,
    actual_snow_load: Force,
    stimulus_at_tick: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PowerPreference {
    PreserveReserve,
    ProtectCondition,
    FinishSooner,
}

impl PowerPreference {
    const fn label(self) -> &'static str {
        match self {
            Self::PreserveReserve => "preserve-reserve",
            Self::ProtectCondition => "protect-condition",
            Self::FinishSooner => "finish-sooner",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ScenarioPolicyVariation {
    load_confidence_ppm: u32,
    power_preference: PowerPreference,
}

impl ScenarioVariation {
    fn from_seed(registries: &Registries, seed: u64) -> Self {
        let a = mix64(seed);
        let b = mix64(a);
        let c = mix64(b);
        let d = mix64(c);
        let e = mix64(d);
        let f = mix64(e);
        let g = mix64(f);
        let h = mix64(g);
        let i = mix64(h);
        let j = mix64(i);
        let k = mix64(j);
        let crusher_definition = registries
            .equipment()
            .get_equipment(EQUIPMENT_JAW_CRUSHER)
            .unwrap_or_else(|| panic!("canonical crusher definition disappeared"));
        let crusher_process = registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
        let maximum_batch = nominal_equipment_mass_capability(
            registries,
            EQUIPMENT_JAW_CRUSHER,
            crusher_process.max_batch_mass_capability(),
        )
        .milligrams();
        assert!(
            maximum_batch > 0,
            "canonical crusher batch limit must be nonzero"
        );
        let minimum_batch = maximum_batch.div_ceil(2);
        let batch_mass = minimum_batch + c % (maximum_batch - minimum_batch + 1);
        let planned_batches = 4 + (a % 3) as u8;

        let thresholds = crusher_definition.maintenance_thresholds();
        let warning = thresholds.warning_below().parts_per_million();
        let first_normal = warning.saturating_add(1).min(CONDITION_PARTS_PER_MILLION);
        let normal_span = CONDITION_PARTS_PER_MILLION - first_normal;
        let initial_condition = first_normal + (e % u64::from(normal_span + 1)) as u32;

        let crusher_weight =
            calculate_weight_force_ceiling(crusher_definition.mass(), registries.core().gravity());
        let structural_profile = registries
            .structural()
            .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
            .unwrap_or_else(|| panic!("canonical compression profile disappeared"));
        let strained = structural_profile.strained_at_ppm();
        let target_low = ((u64::from(strained) * 800_000) / u64::from(STRUCTURAL_PARTS_PER_MILLION))
            .max(1) as u32;
        let slightly_strained =
            ((u64::from(strained) * 1_050_000) / u64::from(STRUCTURAL_PARTS_PER_MILLION)) as u32;
        let target_high = structural_profile
            .cracking_at_ppm()
            .saturating_sub(25_000)
            .min(slightly_strained)
            .max(target_low);
        let target_span = u64::from(target_high - target_low) + 1;
        let compact_target_ppm = target_low + (b % target_span) as u32;
        let compact_area =
            support_area_for_utilization(registries, crusher_weight, compact_target_ppm);
        let reinforced_area = scale_area(compact_area, 1_150_000 + (d % 350_001) as u32);
        let reinforced_background_load = scale_force(crusher_weight, 50_000 + (f % 400_001) as u32);
        let briefed_snow_load = scale_force(crusher_weight, 30_000 + (g % 900_001) as u32);
        let actual_to_briefed_ppm = 700_000 + i % 600_001;
        let actual_snow_load = scale_force(briefed_snow_load, actual_to_briefed_ppm as u32);
        let power_preference = match j % 3 {
            0 => PowerPreference::PreserveReserve,
            1 => PowerPreference::ProtectCondition,
            2 => PowerPreference::FinishSooner,
            _ => unreachable!("modulo three must be exhaustive"),
        };
        Self {
            seed,
            ore: ScenarioOreVariation {
                ore_copper_ppm: 450_000 + (b % 300_001) as u32,
                batch_mass: Mass::from_milligrams(batch_mass),
                planned_batches,
            },
            crusher: ScenarioCrusherVariation {
                initial_crusher_condition: condition(initial_condition),
                large_drive_batch_budget: 1 + (h % u64::from(planned_batches)) as u8,
            },
            structure: ScenarioStructureVariation {
                compact_support_area: compact_area,
                reinforced_support_area: reinforced_area,
                reinforced_background_load,
            },
            stimulus: ScenarioStimulusVariation {
                briefed_snow_load,
                actual_snow_load,
                stimulus_at_tick: 0,
            },
            policy: ScenarioPolicyVariation {
                load_confidence_ppm: 450_000 + (k % 550_001) as u32,
                power_preference,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrushStopReason {
    EnergyUnavailable,
    MaintenanceCritical,
}

#[derive(Clone, Copy)]
struct WorkshopIds {
    ore_source: StockpileId,
    crushed_storage: StockpileId,
    maintenance_source: StockpileId,
    maintenance_spent: StockpileId,
    ore_lot: MaterialLotId,
    crusher: EquipmentId,
    furnace: EquipmentId,
    small_drive: EnergyStoreId,
    large_drive: EnergyStoreId,
    electrical_buffer: EnergyStoreId,
    compact_support: StructuralElementId,
    reinforced_support: StructuralElementId,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioReport {
    seed: u64,
    policy: ScenarioPolicyVariation,
    structure: ScenarioStructureReport,
    choices: ScenarioChoiceReport,
    maintenance: ScenarioMaintenanceReport,
    limits: ScenarioLimitReport,
    progress: ScenarioProgressReport,
}

impl ScenarioReport {
    fn new(seed: u64, policy: ScenarioPolicyVariation, target_batches: u8) -> Self {
        Self {
            seed,
            policy,
            structure: ScenarioStructureReport::default(),
            choices: ScenarioChoiceReport::default(),
            maintenance: ScenarioMaintenanceReport {
                services: 0,
                replacement_spent: Mass::ZERO,
                supply_exhausted: false,
            },
            limits: ScenarioLimitReport::default(),
            progress: ScenarioProgressReport {
                stimulus_applied: false,
                batches_before_stimulus: 0,
                ore_frontier_visible: false,
                completed_batches: 0,
                target_batches,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ScenarioMaintenanceReport {
    services: u8,
    replacement_spent: Mass,
    supply_exhausted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioStructureReport {
    structural_consequence: bool,
    structural_damage_debt: bool,
    support_failure_blocked_production: bool,
    support_relocation: bool,
    structural_stop: bool,
    production_suspension: bool,
    stranded_work_in_process: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioChoiceReport {
    chose_compact_support: bool,
    briefing_changed_siting: bool,
    used_small_drive: bool,
    used_large_drive: bool,
    large_drive_exhausted: bool,
    deadline_power_choice: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioLimitReport {
    energy_bottleneck: bool,
    throughput_bottleneck: bool,
    maintenance_warning: bool,
    maintenance_stop: bool,
    energy_stop: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioProgressReport {
    stimulus_applied: bool,
    batches_before_stimulus: u8,
    ore_frontier_visible: bool,
    completed_batches: u8,
    target_batches: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrushBatchOutcome {
    bottleneck: ComminutionBottleneck,
    completed: bool,
}

struct ScenarioRuntime<'state> {
    variation: ScenarioVariation,
    current_support: &'state mut StructuralElementId,
    alternate_support: &'state mut StructuralElementId,
    report: &'state mut ScenarioReport,
}

fn scale_force(load: Force, parts_per_million: u32) -> Force {
    let scaled = load
        .millinewtons()
        .checked_mul(u128::from(parts_per_million))
        .unwrap_or_else(|| panic!("gameplay harness load-confidence scaling overflowed"))
        / u128::from(STRUCTURAL_PARTS_PER_MILLION);
    Force::from_millinewtons(scaled)
}

fn divide_ceiling(numerator: u128, denominator: u128) -> u128 {
    assert!(denominator > 0, "gameplay harness divisor must be nonzero");
    if numerator == 0 {
        0
    } else {
        1 + (numerator - 1) / denominator
    }
}

fn support_area_for_utilization(
    registries: &Registries,
    carried_load: Force,
    target_utilization_ppm: u32,
) -> Area {
    assert!(target_utilization_ppm > 0);
    let profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("canonical compression profile disappeared"));
    let material = registries
        .materials()
        .get_material(MATERIAL_WOOD)
        .unwrap_or_else(|| panic!("canonical wood material disappeared"));
    let mechanical = material.properties().mechanical();
    let strength_kpa = match profile.load_mode() {
        StructuralLoadMode::Compression => mechanical.compressive_strength_kpa(),
        StructuralLoadMode::Tension => mechanical.tensile_strength_kpa(),
    };
    assert!(
        strength_kpa > 0,
        "canonical support material must have nonzero strength"
    );
    let required_capacity = divide_ceiling(
        carried_load
            .millinewtons()
            .checked_mul(u128::from(STRUCTURAL_PARTS_PER_MILLION))
            .unwrap_or_else(|| panic!("gameplay harness support-capacity scaling overflowed")),
        u128::from(target_utilization_ppm),
    );
    let area = divide_ceiling(required_capacity, u128::from(strength_kpa));
    let area = u64::try_from(area).unwrap_or_else(|_| {
        panic!("gameplay harness support area exceeds authored quantity range")
    });
    Area::from_square_millimeters(area.max(1))
}

fn scale_area(area: Area, parts_per_million: u32) -> Area {
    let scaled = divide_ceiling(
        u128::from(area.square_millimeters()) * u128::from(parts_per_million),
        u128::from(STRUCTURAL_PARTS_PER_MILLION),
    );
    let scaled = u64::try_from(scaled)
        .unwrap_or_else(|_| panic!("gameplay harness support area scaling overflowed"));
    Area::from_square_millimeters(scaled.max(1))
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
    definition: crate::energy::EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    bootstrap_seed_energy_store(registries, state, definition, amount)
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
        StructuralElementGeometry::new(bounds(x), Length::from_micrometers(1), cross_section)
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
    definition: crate::energy::EnergyStoreDefinitionId,
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
        CompositionComponent::new(MATERIAL_SLAG, 1_000_000 - copper_ppm),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("gameplay harness ore composition failed: {error}"),
    }
}

fn assert_canonical_gameplay_content(registries: &Registries) {
    for equipment in [
        EQUIPMENT_JAW_CRUSHER,
        EQUIPMENT_GRINDING_MILL,
        EQUIPMENT_DRY_SCREEN,
        EQUIPMENT_ELECTRIC_FURNACE,
        EQUIPMENT_CASTING_MOLD,
    ] {
        assert!(
            registries.equipment().get_equipment(equipment).is_some(),
            "canonical gameplay equipment {} is absent from build_registries()",
            equipment.value()
        );
    }
    for energy in [
        ENERGY_MECHANICAL_SMALL_DRIVE,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        ENERGY_ELECTRICAL_BUFFER,
        ENERGY_THERMAL_SINK,
    ] {
        assert!(
            registries.energy().get_store(energy).is_some(),
            "canonical gameplay energy definition {} is absent from build_registries()",
            energy.value()
        );
    }
    for process in [
        PROCESS_CRUSH_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
    ] {
        assert!(
            registries.production().get_process(process).is_some(),
            "canonical gameplay process {} is absent from build_registries()",
            process.value()
        );
    }
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
            .is_some()
    );
    assert!(
        registries
            .thermal()
            .get_melting(PROCESS_MELT_PURE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .thermal()
            .get_casting(PROCESS_CAST_PURE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .equipment()
            .get_equipment(EQUIPMENT_JAW_CRUSHER)
            .and_then(|definition| definition.maintenance_profile())
            .is_some(),
        "canonical jaw crusher must expose a physical maintenance service"
    );
}

fn setup_workshop(
    registries: &Registries,
    variation: ScenarioVariation,
) -> (AppState, WorkshopIds) {
    let mut state = AppState::new(WorldSeed::new(variation.seed));
    let ore_mass = variation.ore.batch_mass.milligrams() * u64::from(variation.ore.planned_batches);
    let ore_source = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(ore_mass + variation.ore.batch_mass.milligrams()),
        "ore source",
    );
    let crushed_storage = add_solid_stockpile(
        &mut state,
        Mass::from_milligrams(ore_mass + variation.ore.batch_mass.milligrams()),
        "crushed storage",
    );
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let maintenance_source = add_solid_stockpile(
        &mut state,
        maintenance_profile.replacement_mass(),
        "maintenance source",
    );
    let maintenance_spent = add_solid_stockpile(
        &mut state,
        maintenance_profile.replacement_mass(),
        "maintenance spent",
    );

    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(ore_mass),
        ROOM_TEMPERATURE,
        mixed_ore_composition(variation.ore.ore_copper_ppm),
    );
    seed_lot(
        registries,
        &mut state,
        maintenance_source,
        maintenance_profile.replacement(),
        maintenance_profile.replacement_mass(),
        ROOM_TEMPERATURE,
    );

    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        variation.crusher.initial_crusher_condition,
    )
    .unwrap_or_else(|error| panic!("gameplay harness crusher allocation failed: {error}"));
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
    let occupied_bay = validate_set_structural_load(
        registries,
        &state,
        reinforced_support,
        StructuralLoadKind::Permanent,
        variation.structure.reinforced_background_load,
    )
    .unwrap_or_else(|error| panic!("gameplay harness background support load failed: {error}"));
    occupied_bay.commit(&mut state).unwrap_or_else(|error| {
        panic!("gameplay harness background support load commit failed: {error}")
    });

    let comminution = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("canonical crusher process definition disappeared"));
    let batch_energy =
        calculate_mass_specific_energy(variation.ore.batch_mass, comminution.specific_energy());
    let small_drive_batch_budget = variation.ore.planned_batches;
    let small_drive_energy =
        Energy::from_nanojoules(batch_energy.nanojoules() * u128::from(small_drive_batch_budget));
    let small_drive = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        small_drive_energy,
    );
    let large_drive_energy = Energy::from_nanojoules(
        batch_energy.nanojoules() * u128::from(variation.crusher.large_drive_batch_budget),
    );
    let large_drive = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        large_drive_energy,
    );

    (
        state,
        WorkshopIds {
            ore_source,
            crushed_storage,
            maintenance_source,
            maintenance_spent,
            ore_lot,
            crusher,
            furnace,
            small_drive,
            large_drive,
            electrical_buffer,
            compact_support,
            reinforced_support,
        },
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
            available,
            required,
            ..
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
    println!(
        "  maintenance service: spend={}mg replacement stock condition={}ppm->{}ppm; worn material remains in spent storage",
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
        .get_stockpile(stockpile)
        .and_then(|record| record.lot_ids().next())
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
    thresholds: crate::maintenance::MaintenanceThresholds,
    preference: PowerPreference,
    current_tick: u64,
    stimulus_at_tick: u64,
    stimulus_pending: bool,
    planned_structural_outage: bool,
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
        })) => None,
        Err(error) => panic!("gameplay harness {name} drive resolution failed: {error}"),
    }
}

fn schedule_stimulus_from_current_gameplay(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    variation: &mut ScenarioVariation,
) {
    let reference_duration = resolve_crush_option(
        registries,
        state,
        ids,
        variation.ore.batch_mass,
        "small",
        ids.small_drive,
    )
    .or_else(|| {
        resolve_crush_option(
            registries,
            state,
            ids,
            variation.ore.batch_mass,
            "large",
            ids.large_drive,
        )
    })
    .map(|option| option.resolved.process_resolution().duration().value())
    .unwrap_or_else(|| panic!("gameplay harness has no powered reference batch for event timing"));
    assert!(
        reference_duration > 0,
        "nonzero gameplay batch must take at least one tick"
    );
    let work_horizon = reference_duration
        .checked_mul(u64::from(variation.ore.planned_batches))
        .unwrap_or_else(|| panic!("gameplay harness work horizon overflowed"));
    variation.stimulus.stimulus_at_tick =
        1 + mix64(variation.seed ^ 0x57A1_1EED_71A1_1EED) % work_horizon;
}

fn print_crush_option(option: &CrushOption, thresholds: crate::maintenance::MaintenanceThresholds) {
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
) -> Result<(CrushOption, &'static str, bool), CrushStopReason> {
    let CrushChoiceContext {
        thresholds,
        preference,
        current_tick,
        stimulus_at_tick,
        stimulus_pending,
        planned_structural_outage,
    } = context;
    match (small, large) {
        (None, None) => Err(CrushStopReason::EnergyUnavailable),
        (Some(option), None) | (None, Some(option)) => {
            if thresholds.classify(option.resolved.condition_after()) == MaintenanceBand::Critical {
                Err(CrushStopReason::MaintenanceCritical)
            } else {
                Ok((option, "only viable energy source", false))
            }
        }
        (Some(small), Some(large)) => {
            let small_after = thresholds.classify(small.resolved.condition_after());
            let large_after = thresholds.classify(large.resolved.condition_after());
            if small_after == MaintenanceBand::Critical && large_after == MaintenanceBand::Critical
            {
                Err(CrushStopReason::MaintenanceCritical)
            } else if small_after == MaintenanceBand::Critical {
                Ok((large, "high power avoids critical machine condition", false))
            } else if stimulus_pending
                && planned_structural_outage
                && current_tick < stimulus_at_tick
            {
                Ok((
                    large,
                    "spend high-power reserve before the announced load event may halt production",
                    true,
                ))
            } else if stimulus_pending
                && current_tick < stimulus_at_tick
                && current_tick
                    .checked_add(small.resolved.process_resolution().duration().value())
                    .is_some_and(|finish| finish >= stimulus_at_tick)
                && current_tick
                    .checked_add(large.resolved.process_resolution().duration().value())
                    .is_some_and(|finish| finish < stimulus_at_tick)
            {
                Ok((
                    large,
                    "high power completes this batch before the announced load event",
                    true,
                ))
            } else {
                match preference {
                    PowerPreference::PreserveReserve => Ok((
                        small,
                        "player priority preserves scarce high-power reserve",
                        false,
                    )),
                    PowerPreference::ProtectCondition => {
                        if large.resolved.condition_after() > small.resolved.condition_after() {
                            Ok((
                                large,
                                "player priority minimizes projected active-time wear",
                                false,
                            ))
                        } else {
                            Ok((
                                small,
                                "both power choices project the same machine condition, so preserve reserve",
                                false,
                            ))
                        }
                    }
                    PowerPreference::FinishSooner => {
                        if large.resolved.process_resolution().duration()
                            < small.resolved.process_resolution().duration()
                        {
                            Ok((
                                large,
                                "player priority minimizes projected batch completion time",
                                false,
                            ))
                        } else {
                            Ok((
                                small,
                                "both power choices finish equally soon, so preserve reserve",
                                false,
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
                        ..
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
    batch_index: u8,
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
    if !runtime.report.progress.stimulus_applied
        && started_at < runtime.variation.stimulus.stimulus_at_tick
        && runtime.variation.stimulus.stimulus_at_tick < completes_at
    {
        finish_operation(
            registries,
            state,
            TickSpan::new(runtime.variation.stimulus.stimulus_at_tick - started_at),
        );
        let assessment = apply_stimulus(registries, state, ids, &mut runtime);
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
                        remaining_active_time,
                        ..
                    } if changed_job == job => Some((reason, remaining_active_time)),
                    ProductionAvailabilityChange::Suspended { .. }
                    | ProductionAvailabilityChange::Resumed { .. } => None,
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
            adapt_after_stimulus(registries, state, ids, &mut runtime, assessment);
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
                            ..
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
                adapt_after_stimulus(registries, state, ids, &mut runtime, assessment);
            }
        }
    } else {
        assert_eq!(
            advance_job_until_completion_or_suspension(registries, state, job),
            JobAdvanceOutcome::Completed,
            "crusher production suspended without a harness structural event"
        );
    }
    CrushBatchOutcome {
        bottleneck,
        completed: true,
    }
}

fn structural_assessment(
    analysis: &crate::structural::StructuralAnalysis,
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

fn apply_snow_load(
    registries: &Registries,
    state: &mut AppState,
    support: StructuralElementId,
    load: Force,
) {
    validate_set_structural_load(registries, state, support, StructuralLoadKind::Snow, load)
        .unwrap_or_else(|error| panic!("workshop snow-load validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("workshop snow-load commit failed: {error}"));
}

fn apply_external_snow_load(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    load: Force,
) -> (StructuralAssessment, StructuralAssessment) {
    let mut supports = [ids.compact_support, ids.reinforced_support];
    supports.sort();
    for support in supports {
        apply_snow_load(registries, state, support, load);
    }
    let analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        state.structures(),
    )
    .unwrap_or_else(|error| panic!("workshop external snow-load analysis failed: {error}"));
    (
        structural_assessment(&analysis, ids.compact_support),
        structural_assessment(&analysis, ids.reinforced_support),
    )
}

fn preview_snow_load_after_mount(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mount: &crate::equipment::ValidatedEquipmentSupportChange,
    mounted_support: StructuralElementId,
    load: Force,
) -> StructuralAssessment {
    let mut preview = state.clone();
    mount
        .clone()
        .commit(&mut preview)
        .unwrap_or_else(|error| panic!("workshop planned-load mount preview failed: {error}"));
    let (compact, reinforced) = apply_external_snow_load(registries, &mut preview, ids, load);
    if mounted_support == ids.compact_support {
        compact
    } else {
        reinforced
    }
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
    let mut preview = state.clone();
    validate_unmount_equipment(registries, &preview, ids.crusher)
        .unwrap_or_else(|error| panic!("crusher recovery preview unmount failed: {error}"))
        .commit(&mut preview)
        .unwrap_or_else(|error| panic!("crusher recovery preview unmount commit failed: {error}"));
    let remount_preview = match validate_mount_equipment(
        registries,
        &preview,
        ids.crusher,
        *alternate_support,
    ) {
        Ok(remount) => remount,
        Err(EquipmentSupportError::TargetNotActive { lifecycle, .. }) => {
            println!(
                "  recovery blocked: alternate bay is {lifecycle:?} after the same external snow load"
            );
            return CrusherRelocationOutcome::Blocked;
        }
        Err(error) => panic!("crusher recovery preview remount failed: {error}"),
    };
    let preview_assessment =
        structural_assessment(remount_preview.structural_analysis(), *alternate_support);
    if preview_assessment.stage() == StructuralStage::Failed {
        println!(
            "  recovery blocked: mounting the crusher on the alternate bay would fail it at {}ppm utilization",
            preview_assessment.utilization_ppm()
        );
        return CrusherRelocationOutcome::Blocked;
    }

    let abandoned_support = *current_support;
    validate_unmount_equipment(registries, state, ids.crusher)
        .unwrap_or_else(|error| panic!("crusher recovery unmount failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("crusher recovery unmount commit failed: {error}"));
    let remount = validate_mount_equipment(registries, state, ids.crusher, *alternate_support)
        .unwrap_or_else(|error| panic!("crusher recovery remount failed: {error}"));
    let assessment = structural_assessment(remount.structural_analysis(), *alternate_support);
    debug_assert_ne!(assessment.stage(), StructuralStage::Failed);
    remount
        .commit(state)
        .unwrap_or_else(|error| panic!("crusher recovery remount commit failed: {error}"));
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
            "  recovery note: previous bay remains exposed to the same {}mN external snow load after relocation",
            abandoned.load(StructuralLoadKind::Snow).millinewtons(),
        );
    }
    std::mem::swap(current_support, alternate_support);
    report.structure.support_relocation = true;
    CrusherRelocationOutcome::Relocated
}

fn adapt_after_stimulus(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    runtime: &mut ScenarioRuntime<'_>,
    after: StructuralAssessment,
) {
    if after.stage() == StructuralStage::Failed {
        let suspended_wip = state
            .production()
            .get_equipment_occupant(ids.crusher)
            .filter(|job| job.is_suspended());
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
        let has_remaining_batch = state
            .inventory()
            .get_lot(ids.ore_lot)
            .is_some_and(|lot| lot.mass() >= runtime.variation.ore.batch_mass);
        if has_remaining_batch {
            let selection = [MaterialLotSelection::new(
                ids.ore_lot,
                runtime.variation.ore.batch_mass,
            )];
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
                    EquipmentProviderError::StructuralSupportNotActive { .. }
                ))
            );
            println!(
                "  consequence: failed support blocks the next production batch={}",
                runtime.report.structure.support_failure_blocked_production
            );
        } else {
            if suspended_wip.is_none() {
                println!(
                    "  consequence: support failed after the work order was already complete; recovery still leaves structural damage debt"
                );
            } else {
                println!(
                    "  queue state: no untouched batch remains behind the suspended work-in-process"
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
        let mut preview = state.clone();
        validate_unmount_equipment(registries, &preview, ids.crusher)
            .unwrap_or_else(|error| panic!("crusher relocation preview unmount failed: {error}"))
            .commit(&mut preview)
            .unwrap_or_else(|error| panic!("crusher relocation preview commit failed: {error}"));
        let alternate = match validate_mount_equipment(
            registries,
            &preview,
            ids.crusher,
            *runtime.alternate_support,
        ) {
            Ok(alternate) => alternate,
            Err(EquipmentSupportError::TargetNotActive { lifecycle, .. }) => {
                println!(
                    "  decision: remain on current support; alternate bay is {lifecycle:?} after the external snow load"
                );
                return;
            }
            Err(error) => panic!("crusher relocation preview mount failed: {error}"),
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

fn apply_stimulus(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    runtime: &mut ScenarioRuntime<'_>,
) -> StructuralAssessment {
    assert_eq!(
        state.tick().value(),
        runtime.variation.stimulus.stimulus_at_tick,
        "gameplay harness external load must occur at its announced world tick"
    );
    runtime.report.progress.stimulus_applied = true;
    runtime.report.progress.batches_before_stimulus = runtime.report.progress.completed_batches;
    let (compact, reinforced) = apply_external_snow_load(
        registries,
        state,
        ids,
        runtime.variation.stimulus.actual_snow_load,
    );
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
    println!(
        "  stimulus: external snow load arrives at tick={} after {} completed batch(es); briefed={}mN/bay actual={}mN/bay -> active={} alternate={}",
        state.tick().value(),
        runtime.report.progress.completed_batches,
        runtime.variation.stimulus.briefed_snow_load.millinewtons(),
        runtime.variation.stimulus.actual_snow_load.millinewtons(),
        structural_label(after),
        structural_label(alternate_after),
    );
    after
}

fn apply_stimulus_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    mut runtime: ScenarioRuntime<'_>,
) {
    let after = apply_stimulus(registries, state, ids, &mut runtime);
    adapt_after_stimulus(registries, state, ids, &mut runtime, after);
}

fn run_scenario(registries: &Registries, mut variation: ScenarioVariation) -> ScenarioReport {
    let (mut state, ids) = setup_workshop(registries, variation);
    schedule_stimulus_from_current_gameplay(registries, &state, ids, &mut variation);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness initial matter accounting failed: {error}")
        })
        .total();
    let mut report = ScenarioReport::new(
        variation.seed,
        variation.policy,
        variation.ore.planned_batches,
    );
    let small_drive_batch_budget = variation.ore.planned_batches;
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let planned_snow_load = scale_force(
        variation.stimulus.briefed_snow_load,
        variation.policy.load_confidence_ppm,
    );
    println!(
        "\nSCENARIO seed=0x{:016X} ore={}ppm Cu batch={}mg crusher={}ppm target_batches={} stimulus=[tick:{} briefed_snow:{}mN/bay confidence:{}ppm planned_snow:{}mN/bay] policy=[power:{}] work_reserve=[small:{} batch(es), high-power:{} batch(es)] maintenance=[replacement:{}mg target:{}ppm]",
        variation.seed,
        variation.ore.ore_copper_ppm,
        variation.ore.batch_mass.milligrams(),
        variation
            .crusher
            .initial_crusher_condition
            .parts_per_million(),
        variation.ore.planned_batches,
        variation.stimulus.stimulus_at_tick,
        variation.stimulus.briefed_snow_load.millinewtons(),
        variation.policy.load_confidence_ppm,
        planned_snow_load.millinewtons(),
        variation.policy.power_preference.label(),
        small_drive_batch_budget,
        variation.crusher.large_drive_batch_budget,
        maintenance_profile.replacement_mass().milligrams(),
        maintenance_profile.restored_condition().parts_per_million(),
    );
    println!(
        "  objective: complete the ore work order without operating in critical condition; choose among resolver-projected power and siting options according to this player's priorities"
    );
    println!(
        "  stimulus boundary: the announced snow load and event tick are harness inputs, not an implemented weather or forecasting system; the actual load is hidden from the acting policy until injection"
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
    let compact_planned = preview_snow_load_after_mount(
        registries,
        &state,
        ids,
        &compact_mount,
        ids.compact_support,
        planned_snow_load,
    );
    let reinforced_planned = preview_snow_load_after_mount(
        registries,
        &state,
        ids,
        &reinforced_mount,
        ids.reinforced_support,
        planned_snow_load,
    );
    println!(
        "  support options: compact now={} planned_load={}; reinforced now={} planned_load={} (reinforced existing load={}mN)",
        structural_label(compact_assessment),
        structural_label(compact_planned),
        structural_label(reinforced_assessment),
        structural_label(reinforced_planned),
        variation
            .structure
            .reinforced_background_load
            .millinewtons(),
    );
    let compact_is_better_now = (
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    let compact_is_better = (
        stage_rank(compact_planned.stage()),
        compact_planned.utilization_ppm(),
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_planned.stage()),
        reinforced_planned.utilization_ppm(),
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    report.choices.briefing_changed_siting = compact_is_better != compact_is_better_now;
    let (mut current_support, mut alternate_support, selected_mount, support_name) =
        if compact_is_better {
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
    let selected_assessment = if compact_is_better {
        compact_assessment
    } else {
        reinforced_assessment
    };
    let planned_structural_outage = compact_planned.stage() == StructuralStage::Failed
        && reinforced_planned.stage() == StructuralStage::Failed;
    assert_ne!(selected_assessment.stage(), StructuralStage::Failed);
    if report.choices.briefing_changed_siting {
        println!(
            "  decision: mount crusher on {support_name}; the announced-load plan changes the choice from the best present-only margin"
        );
    } else {
        println!(
            "  decision: mount crusher on {support_name}; it has the best margin under this player's announced-load plan"
        );
    }
    if planned_structural_outage {
        println!(
            "  risk: neither siting option survives the load this player planned around; production before the announced event has extra value"
        );
    }
    selected_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected crusher mount failed: {error}"));

    let thresholds = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"))
        .maintenance_thresholds();
    'batches: for batch_index in 0..variation.ore.planned_batches {
        if report.structure.structural_stop {
            println!(
                "  decision: stop crushing; the external structural load left no support that can carry the machine"
            );
            break;
        }
        let (selected, reason, deadline_driven) = loop {
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
                        break 'batches;
                    }
                }
            }

            let small = resolve_crush_option(
                registries,
                &state,
                ids,
                variation.ore.batch_mass,
                "small",
                ids.small_drive,
            );
            let large = resolve_crush_option(
                registries,
                &state,
                ids,
                variation.ore.batch_mass,
                "large",
                ids.large_drive,
            );
            if let Some(option) = &small {
                print_crush_option(option, thresholds);
            }
            if let Some(option) = &large {
                print_crush_option(option, thresholds);
            } else if !report.choices.large_drive_exhausted {
                report.choices.large_drive_exhausted = true;
                println!("  power reserve: high-power drive can no longer supply a full batch");
            }
            match choose_crush_option(
                small,
                large,
                CrushChoiceContext {
                    thresholds,
                    preference: variation.policy.power_preference,
                    current_tick: state.tick().value(),
                    stimulus_at_tick: variation.stimulus.stimulus_at_tick,
                    stimulus_pending: !report.progress.stimulus_applied,
                    planned_structural_outage,
                },
            ) {
                Ok(choice) => break choice,
                Err(CrushStopReason::EnergyUnavailable) => {
                    report.limits.energy_stop = true;
                    println!(
                        "  decision: stop crushing; no stored mechanical source can supply another batch"
                    );
                    println!(
                        "  energy frontier: stored work is exhausted and no generation/recharge path is present in this workshop setup"
                    );
                    break 'batches;
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
                            break 'batches;
                        }
                    }
                }
            }
        };
        report.choices.deadline_power_choice |= deadline_driven;
        println!("  decision: use {} drive because {reason}", selected.name);
        if selected.store == ids.small_drive {
            report.choices.used_small_drive = true;
        } else if selected.store == ids.large_drive {
            report.choices.used_large_drive = true;
        }
        let outcome = crush_batch(
            registries,
            &mut state,
            ids,
            variation.ore.batch_mass,
            selected,
            batch_index + 1,
            ScenarioRuntime {
                variation,
                current_support: &mut current_support,
                alternate_support: &mut alternate_support,
                report: &mut report,
            },
        );
        if outcome.completed {
            report.progress.completed_batches += 1;
        }
        match outcome.bottleneck {
            ComminutionBottleneck::Throughput => report.limits.throughput_bottleneck = true,
            ComminutionBottleneck::EnergyDelivery => report.limits.energy_bottleneck = true,
            ComminutionBottleneck::Balanced => {
                report.limits.energy_bottleneck = true;
                report.limits.throughput_bottleneck = true;
            }
        }
        if !outcome.completed {
            break;
        }
        if !report.progress.stimulus_applied
            && state.tick().value() >= variation.stimulus.stimulus_at_tick
        {
            apply_stimulus_and_adapt(
                registries,
                &mut state,
                ids,
                ScenarioRuntime {
                    variation,
                    current_support: &mut current_support,
                    alternate_support: &mut alternate_support,
                    report: &mut report,
                },
            );
        }
    }
    if !report.progress.stimulus_applied {
        let current_tick = state.tick().value();
        if current_tick < variation.stimulus.stimulus_at_tick {
            println!(
                "  timeline: work pauses at tick={current_tick}; advance to announced load event at tick={}",
                variation.stimulus.stimulus_at_tick
            );
            finish_operation(
                registries,
                &mut state,
                TickSpan::new(variation.stimulus.stimulus_at_tick - current_tick),
            );
        }
        apply_stimulus_and_adapt(
            registries,
            &mut state,
            ids,
            ScenarioRuntime {
                variation,
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
                MeltingBatchError::ImpureInput { .. }
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
    println!(
        "  outcome: batches={}/{} before_stimulus={} briefing_changed_siting={} deadline_power={} suspended={} stranded_wip={} final_condition={}ppm/{:?} maintenance=[services:{} spent:{}mg remaining:{}mg] mechanical_reserve=[small:{}nJ high-power:{}nJ] active_support={:?}/cracked:{} ticks={}",
        report.progress.completed_batches,
        variation.ore.planned_batches,
        report.progress.batches_before_stimulus,
        report.choices.briefing_changed_siting,
        report.choices.deadline_power_choice,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        final_condition.parts_per_million(),
        thresholds.classify(final_condition),
        report.maintenance.services,
        report.maintenance.replacement_spent.milligrams(),
        maintenance_remaining.milligrams(),
        small_remaining.nanojoules(),
        large_remaining.nanojoules(),
        active_support.lifecycle(),
        active_support.is_cracked(),
        state.tick().value(),
    );
    println!(
        "  report: structural_change={} damage_debt={} support_block={} relocation={} structural_stop={} production_suspension={} stranded_wip={} small_drive={} large_drive={} large_exhausted={} energy_limit={} throughput_limit={} maintenance_warning={} maintenance_services={} maintenance_supply_exhausted={} maintenance_stop={} energy_stop={} ore_frontier={}",
        report.structure.structural_consequence,
        report.structure.structural_damage_debt,
        report.structure.support_failure_blocked_production,
        report.structure.support_relocation,
        report.structure.structural_stop,
        report.structure.production_suspension,
        report.structure.stranded_work_in_process,
        report.choices.used_small_drive,
        report.choices.used_large_drive,
        report.choices.large_drive_exhausted,
        report.limits.energy_bottleneck,
        report.limits.throughput_bottleneck,
        report.limits.maintenance_warning,
        report.maintenance.services,
        report.maintenance.supply_exhausted,
        report.limits.maintenance_stop,
        report.limits.energy_stop,
        report.progress.ore_frontier_visible,
    );
    report
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
    Mass::from_milligrams(1 + mix64(seed ^ 0xF0A1_DA7A) % maximum.min(12))
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
    let unit_count = 1 + mix64(seed ^ 0x0AE5_1A5E) % maximum_units.min(3);
    let batch_mass = Mass::from_milligrams(representable_unit * unit_count);
    let copper_ppm = 300_000 + (mix64(seed ^ 0xC0FF_EE11) % 400_001) as u32;
    (batch_mass, copper_ppm)
}

fn run_foundry_capability_probe(registries: &Registries, seed: u64) -> Vec<&'static str> {
    let mass = foundry_probe_mass(registries, seed);
    let (mut state, ids) = setup_foundry_probe(registries, mass);
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
    [(
        "foundry cast output preserves input mass",
        cast_mass_is_conserved,
    )]
    .into_iter()
    .filter_map(|(name, observed)| (!observed).then_some(name))
    .collect()
}

fn run_ore_preparation_capability_probe(registries: &Registries, seed: u64) -> Vec<&'static str> {
    let (batch_mass, copper_ppm) = ore_preparation_probe_parameters(registries, seed);
    let (mut state, ids) = setup_ore_preparation_probe(registries, batch_mass, copper_ppm);
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
            ..
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
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange { .. },
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
    let (fine_energy, final_grinder_projection, oversize_profile_is_preserved) =
        if screened_oversize_mass.is_zero() {
            println!(
                "  regrind oversize: skipped because the authored screen produced no oversize"
            );
            (Energy::ZERO, grinder_condition, true)
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
    let undersize_stockpile = state
        .inventory()
        .get_stockpile(ids.undersize_storage)
        .unwrap_or_else(|| panic!("ore preparation undersize storage disappeared"));
    let composition_preserved = undersize_stockpile.lot_ids().all(|lot| {
        state
            .inventory()
            .get_lot(lot)
            .is_some_and(|lot| lot.composition() == &output_composition)
    });
    let final_distribution_is_fine = undersize_stockpile.lot_ids().all(|lot| {
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

    requirements
        .into_iter()
        .filter_map(|(name, observed)| (!observed).then_some(name))
        .collect()
}

/// Runs the maintained headless workshop exercise and fails with named contract gaps.
///
/// This entry point exists only with the `test-gameplay` feature so the dedicated integration target
/// can exercise gameplay behavior without compiling every crate unit-test body into the same binary.
pub fn run_gameplay_harness() {
    let registries = build_registries();
    assert_canonical_gameplay_content(&registries);
    let ScenarioSeedPlan {
        seeds,
        coverage_seed_count,
        variation_seed,
    } = scenario_seeds()
        .unwrap_or_else(|error| panic!("gameplay harness configuration failed: {error:?}"));
    let replay_seeds = seeds
        .iter()
        .map(|seed| format!("0x{seed:016X}"))
        .collect::<Vec<_>>()
        .join(",");
    let variation_label = variation_seed
        .map(|seed| format!("0x{seed:016X}"))
        .unwrap_or_else(|| "custom-seed-list".to_owned());
    std::println!(
        "HARNESS INPUT maintained={} organic={} variation_seed={} replay={replay_seeds}",
        coverage_seed_count,
        seeds.len().saturating_sub(coverage_seed_count),
        variation_label,
    );
    let probe_seed = seeds
        .iter()
        .copied()
        .fold(0xD33F_C01D_5EED_u64, |combined, seed| {
            mix64(combined ^ seed)
        });
    println!(
        "\n=== DEEP HEARTH WORKSHOP GAMEPLAY HARNESS: {} scenario(s), registry schema {} ===",
        seeds.len(),
        registries.schema_version().value(),
    );
    println!(
        "SETUP BOUNDARY: matter, equipment, finite energy, structural bays, and the reinforced bay's baseline load are starting conditions; every experienced decision and mutation after setup uses canonical runtime transactions."
    );
    println!(
        "WORKSHOP FANTASY: turn a constrained, failure-prone physical workshop into reliable production by reading structural margin, power reserve, machine condition, material state, and an announced external load event."
    );
    println!(
        "LOOP SCOPE: the scenario matrix experiences varied player priorities, imperfect confidence in an announced external snow load, comminution, finite stored work, power-versus-time tradeoffs, wear, finite replacement-stock maintenance, exact-tick load injection, persistent structural damage, production suspension, and recovery. Deep Hearth does not yet implement weather/forecast ownership; geological acquisition and construction authorization also remain outside this workshop setup. Separate ore-preparation and foundry probes validate existing downstream capabilities without pretending the mixed-ore chain is complete."
    );

    let reports: Vec<_> = seeds
        .into_iter()
        .map(|seed| ScenarioVariation::from_seed(&registries, seed))
        .map(|variation| run_scenario(&registries, variation))
        .collect();

    let completed_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.completed_batches))
        .sum();
    let target_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.target_batches))
        .sum();
    let batches_before_stimulus: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.batches_before_stimulus))
        .sum();
    let completed_orders = reports
        .iter()
        .filter(|report| report.progress.completed_batches == report.progress.target_batches)
        .count();
    let recovered_work_in_process = reports
        .iter()
        .filter(|report| {
            report.structure.production_suspension && !report.structure.stranded_work_in_process
        })
        .count();
    let maintenance_services: u32 = reports
        .iter()
        .map(|report| u32::from(report.maintenance.services))
        .sum();
    let ore_preparation_gaps = run_ore_preparation_capability_probe(&registries, probe_seed);
    let foundry_gaps = run_foundry_capability_probe(&registries, probe_seed);

    let mut gaps = scenario_contract_gaps(&reports);
    gaps.extend(
        ore_preparation_gaps
            .into_iter()
            .map(|gap| format!("ore preparation: {gap}")),
    );
    gaps.extend(
        foundry_gaps
            .into_iter()
            .map(|gap| format!("foundry: {gap}")),
    );
    if coverage_seed_count > 0 {
        gaps.extend(
            coverage_gaps(&reports[..coverage_seed_count])
                .into_iter()
                .map(|gap| format!("coverage: {gap}")),
        );
    }
    assert!(
        gaps.is_empty(),
        "gameplay exercise failures:\n- {}",
        gaps.join("\n- ")
    );

    std::println!(
        "HARNESS PASS mode={HARNESS_MODE} scenarios={} orders={completed_orders}/{} batches={completed_batches}/{target_batches} pre_stimulus={batches_before_stimulus} stops=[structural:{} maintenance:{} energy:{}] material=[ore_prep:pass foundry:pass mixed_ore_bridge:blocked]",
        reports.len(),
        reports.len(),
        reports
            .iter()
            .filter(|report| report.structure.structural_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.maintenance_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.energy_stop)
            .count(),
    );
    std::println!(
        "SYSTEMS policy=[reserve:{} condition:{} speed:{}] control=[briefing_siting:{} deadline_power:{}] recovery=[relocations:{} resumed_wip:{recovered_work_in_process} stranded_wip:{} maintenance_services:{maintenance_services}] pressure=[structural:{} maintenance_warning:{}] bottlenecks=[energy_delivery:{} throughput:{}]",
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::PreserveReserve)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::ProtectCondition)
            .count(),
        reports
            .iter()
            .filter(|report| report.policy.power_preference == PowerPreference::FinishSooner)
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.briefing_changed_siting)
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.deadline_power_choice)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.support_relocation)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.stranded_work_in_process)
            .count(),
        reports
            .iter()
            .filter(|report| report.structure.structural_consequence)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.maintenance_warning)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.energy_bottleneck)
            .count(),
        reports
            .iter()
            .filter(|report| report.limits.throughput_bottleneck)
            .count(),
    );
    std::println!(
        "SCOPE exercised=[canonical comminution,power choice,wear,maintenance,structural siting,failure recovery] bootstrap=[matter,stored energy,equipment,constructed bays,baseline load] external=[announced snow stimulus] deferred=[resource acquisition,energy generation,weather ownership,concentration/smelting bridge]"
    );
}

#[doc(hidden)]
#[must_use]
pub fn gameplay_harness_configuration_contract_gaps() -> Vec<&'static str> {
    configuration_contract_gaps()
}
