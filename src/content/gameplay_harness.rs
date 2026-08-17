//! Headless workshop gameplay harness over the same canonical content registries used by the game.
//!
//! The harness deliberately varies physical initial conditions and lets a small operational policy
//! react only to observed state and resolver projections. Normal exercise-mode runs combine a
//! deterministic experience-coverage matrix with one fixed exploratory seed for reproducibility.
//! `DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED` replaces that exploratory seed with an exact decimal or
//! `0x` hexadecimal seed for additional uncurated coverage. Forecast event timing is exact but load
//! magnitude is only an estimate; deterministic actual regional snow is revealed on the event tick,
//! including during in-flight production. Faster machinery can therefore change how much work is
//! secured before an uncertain environment changes. `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the whole
//! matrix with an exact comma-separated decimal or `0x` hexadecimal seed list; malformed entries are
//! rejected instead of ignored. Detailed trace output is opt-in via `DEEP_HEARTH_GAMEPLAY_VERBOSE`.

use std::env;

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

use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{
    EnergyStoreId, EnergySupplyError, add_energy_store, add_energy_store_with_initial_for_test,
    calculate_mass_specific_energy,
};
use crate::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, EquipmentMaintenanceResolutionError,
    EquipmentProviderError, EquipmentSupportError, add_equipment, resolve_equipment_maintenance,
    validate_equipment_repair, validate_mount_equipment, validate_unmount_equipment,
};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile,
    add_solid_stockpile_for_test, add_stockpile, deposit_composed_lot_for_test,
    deposit_lot_for_test,
};
use crate::maintenance::{Condition, MaintenanceBand};
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
    StructuralAssessment, StructuralElementId, StructuralLifecycle, StructuralLoadKind,
    StructuralStage, add_structural_element, analyze_structure,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
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
    disturbance: ScenarioDisturbanceVariation,
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
struct ScenarioDisturbanceVariation {
    forecast_snow_load: Force,
    actual_snow_load: Force,
    disturbance_at_tick: u64,
}

impl ScenarioVariation {
    fn from_seed(seed: u64) -> Self {
        let a = mix64(seed);
        let b = mix64(a);
        let c = mix64(b);
        let d = mix64(c);
        let e = mix64(d);
        let f = mix64(e);
        let g = mix64(f);
        let h = mix64(g);
        let i = mix64(h);
        let compact_area = 1_450_u64 + a % 351;
        let reinforced_area = compact_area + 300 + b % 351;
        let forecast_snow_millinewtons = 1_000_000 + g % 32_000_001;
        let actual_to_forecast_ppm = 700_000 + i % 600_001;
        let actual_snow_millinewtons =
            u128::from(forecast_snow_millinewtons) * u128::from(actual_to_forecast_ppm) / 1_000_000;
        Self {
            seed,
            ore: ScenarioOreVariation {
                ore_copper_ppm: 450_000 + (b % 300_001) as u32,
                batch_mass: Mass::from_milligrams(8 + c % 13),
                planned_batches: 4 + (a % 3) as u8,
            },
            crusher: ScenarioCrusherVariation {
                initial_crusher_condition: condition(650_000 + (e % 330_001) as u32),
                large_drive_batch_budget: 1 + (h % 3) as u8,
            },
            structure: ScenarioStructureVariation {
                compact_support_area: Area::from_square_millimeters(compact_area),
                reinforced_support_area: Area::from_square_millimeters(reinforced_area),
                reinforced_background_load: Force::from_millinewtons(u128::from(
                    4_000_000 + f % 12_000_001,
                )),
            },
            disturbance: ScenarioDisturbanceVariation {
                forecast_snow_load: Force::from_millinewtons(u128::from(
                    forecast_snow_millinewtons,
                )),
                actual_snow_load: Force::from_millinewtons(actual_snow_millinewtons),
                disturbance_at_tick: 15 + d % 26,
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

#[derive(Clone, Copy)]
struct FoundryIds {
    pure_copper_source: StockpileId,
    molten_vessel: StockpileId,
    cast_storage: StockpileId,
    pure_copper_lot: MaterialLotId,
    furnace: EquipmentId,
    mold: EquipmentId,
    electrical_buffer: EnergyStoreId,
    heat_sink: EnergyStoreId,
}

#[derive(Clone, Copy)]
struct OrePreparationProbeIds {
    ore_source: StockpileId,
    crushed_storage: StockpileId,
    ground_storage: StockpileId,
    undersize_storage: StockpileId,
    oversize_storage: StockpileId,
    ore_lot: MaterialLotId,
    crusher: EquipmentId,
    grinder: EquipmentId,
    screen: EquipmentId,
    drive: EnergyStoreId,
}

#[derive(Clone, Copy, Debug)]
struct ScenarioReport {
    seed: u64,
    structure: ScenarioStructureReport,
    choices: ScenarioChoiceReport,
    maintenance: ScenarioMaintenanceReport,
    limits: ScenarioLimitReport,
    progress: ScenarioProgressReport,
}

impl ScenarioReport {
    fn new(seed: u64, target_batches: u8) -> Self {
        Self {
            seed,
            structure: ScenarioStructureReport::default(),
            choices: ScenarioChoiceReport::default(),
            maintenance: ScenarioMaintenanceReport {
                services: 0,
                replacement_spent: Mass::ZERO,
                supply_exhausted: false,
            },
            limits: ScenarioLimitReport::default(),
            progress: ScenarioProgressReport {
                disturbance_applied: false,
                batches_before_disturbance: 0,
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
    forecast_changed_siting: bool,
    used_small_drive: bool,
    used_large_drive: bool,
    large_drive_exhausted: bool,
    forecast_power_choice: bool,
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
    disturbance_applied: bool,
    batches_before_disturbance: u8,
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

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn seed_energy_store_exact(
    registries: &Registries,
    state: &mut AppState,
    definition: crate::energy::EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    add_energy_store_with_initial_for_test(registries, state, definition, amount)
        .unwrap_or_else(|error| panic!("gameplay harness exact energy seed failed: {error}"))
}

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(value) => value,
        Err(error) => panic!("gameplay harness condition is invalid: {error}"),
    }
}

fn parse_seed(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse().ok()
    }
}

/// Fixed default for the extra uncurated exploratory probe. A constant so the normal harness run
/// is reproducible; override with `DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED` for extra coverage.
const DEFAULT_EXPLORATORY_SEED: u64 = 0xD33D_1A5E_BEEF_5EED;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameplayHarnessConfigError {
    InvalidExploratorySeed,
    EmptyScenarioSeedList,
    InvalidScenarioSeed { index: usize },
}

fn resolve_exploratory_seed(raw: Option<&str>) -> Result<u64, GameplayHarnessConfigError> {
    match raw {
        Some(text) => parse_seed(text).ok_or(GameplayHarnessConfigError::InvalidExploratorySeed),
        None => Ok(DEFAULT_EXPLORATORY_SEED),
    }
}

fn scenario_seeds_from(
    scenario_raw: Option<&str>,
    exploratory_raw: Option<&str>,
) -> Result<(Vec<u64>, bool), GameplayHarnessConfigError> {
    if let Some(raw) = scenario_raw {
        if raw.trim().is_empty() {
            return Err(GameplayHarnessConfigError::EmptyScenarioSeedList);
        }
        let mut seeds = Vec::new();
        for (index, token) in raw.split(',').enumerate() {
            let seed = parse_seed(token)
                .ok_or(GameplayHarnessConfigError::InvalidScenarioSeed { index })?;
            seeds.push(seed);
        }
        return Ok((seeds, false));
    }

    // Stable coverage seeds exercise: regional structural outage, maintenance stop, forecast-driven
    // siting, successful relocation after forecast error, and relocation without prior support
    // failure. The exploratory seed then probes one additional uncurated combination. It is a fixed
    // constant by default so the run is reproducible, overridable via
    // `DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED` for extra coverage.
    let mut seeds = vec![1, 4, 13, 41, 61];
    seeds.push(resolve_exploratory_seed(exploratory_raw)?);
    Ok((seeds, true))
}

fn scenario_seeds() -> Result<(Vec<u64>, bool), GameplayHarnessConfigError> {
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let exploratory_raw = env::var("DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED").ok();
    scenario_seeds_from(scenario_raw.as_deref(), exploratory_raw.as_deref())
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
    let geometry = crate::structural::make_test_structural_geometry(
        bounds(x),
        Length::from_micrometers(1),
        cross_section,
    );
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
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
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
    match add_energy_store_with_initial_for_test(registries, state, definition, amount) {
        Ok(store) => store,
        Err(error) => panic!("gameplay harness initial energy store failed: {error}"),
    }
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
    let ore_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(ore_mass + 20))
        .unwrap_or_else(|error| panic!("gameplay harness ore stockpile failed: {error}"));
    let crushed_storage =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(ore_mass + 20))
            .unwrap_or_else(|error| panic!("gameplay harness crushed storage failed: {error}"));
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    let maintenance_source =
        add_solid_stockpile_for_test(&mut state, maintenance_profile.replacement_mass())
            .unwrap_or_else(|error| {
                panic!("gameplay harness maintenance stockpile failed: {error}")
            });
    let maintenance_spent =
        add_solid_stockpile_for_test(&mut state, maintenance_profile.replacement_mass())
            .unwrap_or_else(|error| {
                panic!("gameplay harness spent-maintenance stockpile failed: {error}")
            });

    let ore_lot = deposit_composed_lot_for_test(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(ore_mass),
        ROOM_TEMPERATURE,
        mixed_ore_composition(variation.ore.ore_copper_ppm),
    )
    .unwrap_or_else(|error| panic!("gameplay harness ore seed failed: {error}"));
    deposit_lot_for_test(
        registries,
        &mut state,
        maintenance_source,
        maintenance_profile.replacement(),
        maintenance_profile.replacement_mass(),
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| {
        panic!("gameplay harness maintenance replacement seed failed: {error}")
    });

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

fn setup_foundry_probe(registries: &Registries, mass: Mass) -> (AppState, FoundryIds) {
    let mut state = AppState::new(WorldSeed::new(0xD33F_F001));
    let pure_copper_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(30))
        .unwrap_or_else(|error| panic!("foundry probe copper stockpile failed: {error}"));
    let vessel_profile =
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000))
            .unwrap_or_else(|error| panic!("foundry probe molten storage profile failed: {error}"));
    let molten_vessel = add_stockpile(&mut state, Mass::from_milligrams(30), vessel_profile)
        .unwrap_or_else(|error| panic!("foundry probe molten vessel failed: {error}"));
    let cast_storage = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(30))
        .unwrap_or_else(|error| panic!("foundry probe cast storage failed: {error}"));
    let pure_copper_lot = deposit_lot_for_test(
        registries,
        &mut state,
        pure_copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        mass,
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("foundry probe copper seed failed: {error}"));
    let furnace = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("foundry probe furnace allocation failed: {error}"));
    let mold = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_CASTING_MOLD,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("foundry probe mold allocation failed: {error}"));
    let electrical_buffer = seed_energy_store(registries, &mut state, ENERGY_ELECTRICAL_BUFFER, 2);
    let heat_sink = add_energy_store(registries, &mut state, ENERGY_THERMAL_SINK)
        .unwrap_or_else(|error| panic!("foundry probe thermal sink allocation failed: {error}"));
    (
        state,
        FoundryIds {
            pure_copper_source,
            molten_vessel,
            cast_storage,
            pure_copper_lot,
            furnace,
            mold,
            electrical_buffer,
            heat_sink,
        },
    )
}

fn setup_ore_preparation_probe(registries: &Registries) -> (AppState, OrePreparationProbeIds) {
    let mut state = AppState::new(WorldSeed::new(0xD33F_0A11));
    let batch_mass = Mass::from_milligrams(10);
    let ore_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("ore preparation source failed: {error}"));
    let crushed_storage = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("ore preparation crushed storage failed: {error}"));
    let ground_storage = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("ore preparation ground storage failed: {error}"));
    let undersize_storage = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("ore preparation undersize storage failed: {error}"));
    let oversize_storage = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("ore preparation oversize storage failed: {error}"));
    let ore_lot = deposit_composed_lot_for_test(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        mixed_ore_composition(500_000),
    )
    .unwrap_or_else(|error| panic!("ore preparation material seed failed: {error}"));
    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation crusher failed: {error}"));
    let grinder = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_GRINDING_MILL,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinder failed: {error}"));
    let screen = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_DRY_SCREEN,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation screen failed: {error}"));
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
    let oversize_mass = Mass::from_milligrams(5);
    let required_work =
        calculate_mass_specific_energy(batch_mass, crusher_definition.specific_energy())
            .checked_add(calculate_mass_specific_energy(
                batch_mass,
                grinder_definition.specific_energy(),
            ))
            .and_then(|work| {
                work.checked_add(calculate_mass_specific_energy(
                    batch_mass,
                    screen_definition.specific_energy(),
                ))
            })
            .and_then(|work| {
                work.checked_add(calculate_mass_specific_energy(
                    oversize_mass,
                    fine_grind_definition.specific_energy(),
                ))
            })
            .unwrap_or_else(|| panic!("ore preparation work budget overflowed"));
    let drive = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        required_work,
    );
    (
        state,
        OrePreparationProbeIds {
            ore_source,
            crushed_storage,
            ground_storage,
            undersize_storage,
            oversize_storage,
            ore_lot,
            crusher,
            grinder,
            screen,
            drive,
        },
    )
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

fn maintenance_band_rank(band: MaintenanceBand) -> u8 {
    match band {
        MaintenanceBand::Normal => 0,
        MaintenanceBand::Warning => 1,
        MaintenanceBand::Critical => 2,
    }
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
    thresholds: crate::maintenance::MaintenanceThresholds,
    current_tick: u64,
    disturbance_at_tick: u64,
    disturbance_pending: bool,
    forecasted_structural_outage: bool,
) -> Result<(CrushOption, &'static str, bool), CrushStopReason> {
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
            let current_band = thresholds.classify(small.resolved.condition_before());
            let small_after = thresholds.classify(small.resolved.condition_after());
            let large_after = thresholds.classify(large.resolved.condition_after());
            if small_after == MaintenanceBand::Critical && large_after == MaintenanceBand::Critical
            {
                Err(CrushStopReason::MaintenanceCritical)
            } else if small_after == MaintenanceBand::Critical {
                Ok((large, "high power avoids critical machine condition", false))
            } else if disturbance_pending
                && forecasted_structural_outage
                && current_tick < disturbance_at_tick
            {
                Ok((
                    large,
                    "spend high-power reserve while the forecast still permits production",
                    true,
                ))
            } else if disturbance_pending
                && current_tick < disturbance_at_tick
                && current_tick
                    .checked_add(small.resolved.process_resolution().duration().value())
                    .is_some_and(|finish| finish >= disturbance_at_tick)
                && current_tick
                    .checked_add(large.resolved.process_resolution().duration().value())
                    .is_some_and(|finish| finish < disturbance_at_tick)
            {
                Ok((
                    large,
                    "high power completes this batch before the forecast structural disturbance",
                    true,
                ))
            } else if current_band != MaintenanceBand::Normal
                || maintenance_band_rank(small_after) > maintenance_band_rank(current_band)
            {
                Ok((large, "high power limits active-time wear", false))
            } else {
                Ok((
                    small,
                    "preserve scarce high-power reserve while condition is healthy",
                    false,
                ))
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
    if !runtime.report.progress.disturbance_applied
        && started_at < runtime.variation.disturbance.disturbance_at_tick
        && runtime.variation.disturbance.disturbance_at_tick < completes_at
    {
        finish_operation(
            registries,
            state,
            TickSpan::new(runtime.variation.disturbance.disturbance_at_tick - started_at),
        );
        let assessment = apply_disturbance(registries, state, ids, &mut runtime);
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
            adapt_after_disturbance(registries, state, ids, &mut runtime, assessment);
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
                adapt_after_disturbance(registries, state, ids, &mut runtime, assessment);
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

fn apply_regional_snow(
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
    .unwrap_or_else(|error| panic!("workshop regional snow analysis failed: {error}"));
    (
        structural_assessment(&analysis, ids.compact_support),
        structural_assessment(&analysis, ids.reinforced_support),
    )
}

fn preview_regional_snow_after_mount(
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
        .unwrap_or_else(|error| panic!("workshop forecast mount preview failed: {error}"));
    let (compact, reinforced) = apply_regional_snow(registries, &mut preview, ids, load);
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
                "  recovery blocked: alternate bay is {lifecycle:?} after the same regional weather event"
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
            "  recovery note: previous bay remains exposed to the same regional {}mN snow load after relocation",
            abandoned.load(StructuralLoadKind::Snow).millinewtons(),
        );
    }
    std::mem::swap(current_support, alternate_support);
    report.structure.support_relocation = true;
    CrusherRelocationOutcome::Relocated
}

fn adapt_after_disturbance(
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
                    "  decision: remain on current support; alternate bay is {lifecycle:?} after the regional snow load"
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

fn apply_disturbance(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    runtime: &mut ScenarioRuntime<'_>,
) -> StructuralAssessment {
    assert_eq!(
        state.tick().value(),
        runtime.variation.disturbance.disturbance_at_tick,
        "gameplay harness environmental load must occur at its forecast world tick"
    );
    runtime.report.progress.disturbance_applied = true;
    runtime.report.progress.batches_before_disturbance = runtime.report.progress.completed_batches;
    let (compact, reinforced) = apply_regional_snow(
        registries,
        state,
        ids,
        runtime.variation.disturbance.actual_snow_load,
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
        "  disturbance: snow arrives at tick={} after {} completed batch(es); forecast={}mN/bay actual={}mN/bay -> active={} alternate={}",
        state.tick().value(),
        runtime.report.progress.completed_batches,
        runtime
            .variation
            .disturbance
            .forecast_snow_load
            .millinewtons(),
        runtime
            .variation
            .disturbance
            .actual_snow_load
            .millinewtons(),
        structural_label(after),
        structural_label(alternate_after),
    );
    after
}

fn apply_disturbance_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    mut runtime: ScenarioRuntime<'_>,
) {
    let after = apply_disturbance(registries, state, ids, &mut runtime);
    adapt_after_disturbance(registries, state, ids, &mut runtime, after);
}

fn run_scenario(registries: &Registries, variation: ScenarioVariation) -> ScenarioReport {
    let (mut state, ids) = setup_workshop(registries, variation);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| {
            panic!("gameplay harness initial matter accounting failed: {error}")
        })
        .total();
    let mut report = ScenarioReport::new(variation.seed, variation.ore.planned_batches);
    let small_drive_batch_budget = variation.ore.planned_batches;
    let maintenance_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("canonical crusher maintenance profile disappeared"));
    println!(
        "\nSCENARIO seed=0x{:016X} ore={}ppm Cu batch={}mg crusher={}ppm target_batches={} forecast=[tick:{} snow:{}mN/bay] work_reserve=[small:{} batch(es), high-power:{} batch(es)] maintenance=[replacement:{}mg target:{}ppm]",
        variation.seed,
        variation.ore.ore_copper_ppm,
        variation.ore.batch_mass.milligrams(),
        variation
            .crusher
            .initial_crusher_condition
            .parts_per_million(),
        variation.ore.planned_batches,
        variation.disturbance.disturbance_at_tick,
        variation.disturbance.forecast_snow_load.millinewtons(),
        small_drive_batch_budget,
        variation.crusher.large_drive_batch_budget,
        maintenance_profile.replacement_mass().milligrams(),
        maintenance_profile.restored_condition().parts_per_million(),
    );
    println!(
        "  objective: complete the ore work order without operating in critical condition; use scarce high power and finite replacement stock where time, wear, or the forecast makes them worth spending"
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
    let compact_forecast = preview_regional_snow_after_mount(
        registries,
        &state,
        ids,
        &compact_mount,
        ids.compact_support,
        variation.disturbance.forecast_snow_load,
    );
    let reinforced_forecast = preview_regional_snow_after_mount(
        registries,
        &state,
        ids,
        &reinforced_mount,
        ids.reinforced_support,
        variation.disturbance.forecast_snow_load,
    );
    println!(
        "  support options: compact now={} forecast={}; reinforced now={} forecast={} (reinforced existing load={}mN)",
        structural_label(compact_assessment),
        structural_label(compact_forecast),
        structural_label(reinforced_assessment),
        structural_label(reinforced_forecast),
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
        stage_rank(compact_forecast.stage()),
        compact_forecast.utilization_ppm(),
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_forecast.stage()),
        reinforced_forecast.utilization_ppm(),
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    report.choices.forecast_changed_siting = compact_is_better != compact_is_better_now;
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
    let forecasted_structural_outage = compact_forecast.stage() == StructuralStage::Failed
        && reinforced_forecast.stage() == StructuralStage::Failed;
    assert_ne!(selected_assessment.stage(), StructuralStage::Failed);
    if report.choices.forecast_changed_siting {
        println!(
            "  decision: mount crusher on {support_name}; the forecast changes the choice from the best present-only margin"
        );
    } else {
        println!(
            "  decision: mount crusher on {support_name}; it has the best forecast-adjusted margin"
        );
    }
    if forecasted_structural_outage {
        println!(
            "  risk: neither siting option avoids the forecasted structural outage; production before the event has extra value"
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
                "  decision: stop crushing; the regional structural outage left no support that can carry the machine"
            );
            break;
        }
        let (selected, reason, forecast_driven) = loop {
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
                thresholds,
                state.tick().value(),
                variation.disturbance.disturbance_at_tick,
                !report.progress.disturbance_applied,
                forecasted_structural_outage,
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
        report.choices.forecast_power_choice |= forecast_driven;
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
        if !report.progress.disturbance_applied
            && state.tick().value() >= variation.disturbance.disturbance_at_tick
        {
            apply_disturbance_and_adapt(
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
    if !report.progress.disturbance_applied {
        let current_tick = state.tick().value();
        if current_tick < variation.disturbance.disturbance_at_tick {
            println!(
                "  timeline: work pauses at tick={current_tick}; advance to forecast disturbance at tick={}",
                variation.disturbance.disturbance_at_tick
            );
            finish_operation(
                registries,
                &mut state,
                TickSpan::new(variation.disturbance.disturbance_at_tick - current_tick),
            );
        }
        apply_disturbance_and_adapt(
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

    let crushed_lot = stockpile_first_lot(&state, ids.crushed_storage);
    let crushed_mass = state
        .inventory()
        .get_stockpile(ids.crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("crushed storage disappeared"));
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
        "  outcome: batches={}/{} before_disturbance={} forecast_siting={} forecast_power={} suspended={} stranded_wip={} final_condition={}ppm/{:?} maintenance=[services:{} spent:{}mg remaining:{}mg] mechanical_reserve=[small:{}nJ high-power:{}nJ] active_support={:?}/cracked:{} ticks={}",
        report.progress.completed_batches,
        variation.ore.planned_batches,
        report.progress.batches_before_disturbance,
        report.choices.forecast_changed_siting,
        report.choices.forecast_power_choice,
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

fn run_foundry_capability_probe(registries: &Registries) -> Vec<&'static str> {
    let mass = Mass::from_milligrams(10);
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

fn run_ore_preparation_capability_probe(registries: &Registries) -> Vec<&'static str> {
    let (mut state, ids) = setup_ore_preparation_probe(registries);
    let batch_mass = Mass::from_milligrams(10);
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("ore preparation initial matter accounting failed: {error}"))
        .total();
    let initial_energy = state
        .energy()
        .get_store(ids.drive)
        .map(|store| store.stored())
        .unwrap_or_else(|| panic!("ore preparation drive disappeared"));

    println!(
        "\nORE PREPARATION CAPABILITY PROBE: canonical crushing, grinding, and dry screening share finite mechanical work and preserve mixed-ore matter without claiming concentration"
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
    let crusher_output_is_conservative = crushed_distribution.classes().len() == 1;
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
    let grinder_is_required = matches!(
        direct_screen,
        Err(ScreeningResolutionError::Batch(
            ScreeningBatchError::UnresolvedParticleClass { .. }
        ))
    );
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
    let fine_grind_requires_screen_oversize = matches!(
        direct_fine_grind,
        Err(ComminutionResolutionError::Batch(
            ComminutionBatchError::InputParticleSizeOutsideOperatingRange { .. }
        ))
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
        .unwrap_or_else(|| panic!("canonical grinding output lost particle-size state"));
    let ground_classes = ground_distribution.classes();
    let grinding_resolved_screen_cut = ground_classes.len() == 2
        && ground_classes[0].range().maximum_diameter() == Length::from_micrometers(2_000)
        && ground_classes[1].range().minimum_diameter() == Length::from_micrometers(2_001);

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
    validate_start_process_routed(
        registries,
        &state,
        screened.process_resolution(),
        ids.ground_storage,
        &[
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                ids.undersize_storage,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                ids.oversize_storage,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("ore preparation screening start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ore preparation screening commit failed: {error}"));
    finish_operation(registries, &mut state, screen_duration);
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));

    let oversize_lot = stockpile_first_lot(&state, ids.oversize_storage);
    let oversize_before_regrind = state
        .inventory()
        .get_lot(oversize_lot)
        .unwrap_or_else(|| panic!("ore preparation oversize lot disappeared"));
    let oversize_profile_is_preserved = oversize_before_regrind.composition()
        == &mixed_ore_composition(500_000)
        && oversize_before_regrind
            .particle_size()
            .is_some_and(|range| {
                range.minimum_diameter() == Length::from_micrometers(2_001)
                    && range.maximum_diameter() == Length::from_micrometers(4_000)
            });
    let fine_selection = [MaterialLotSelection::new(
        oversize_lot,
        Mass::from_milligrams(5),
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
    .unwrap_or_else(|error| panic!("canonical fine-grinding probe resolution failed: {error}"));
    let fine_duration = fine_ground.process_resolution().duration();
    println!(
        "  regrind oversize: mass=5mg energy={}nJ rate={}mg/s duration={}t bottleneck={:?}",
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
    let output_composition = mixed_ore_composition(500_000);
    let undersize_lot = stockpile_first_lot(&state, ids.undersize_storage);
    let composition_preserved = state
        .inventory()
        .get_lot(undersize_lot)
        .is_some_and(|lot| lot.composition() == &output_composition);
    let final_distribution_is_fine = state
        .inventory()
        .get_lot(undersize_lot)
        .and_then(|lot| lot.particle_size())
        .is_some_and(|range| {
            range.minimum_diameter() == Length::from_micrometers(500)
                && range.maximum_diameter() == Length::from_micrometers(2_000)
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
            "ore preparation exhausts its finite work budget",
            final_energy == Energy::ZERO,
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
            "crusher output remains conservative",
            crusher_output_is_conservative,
        ),
        (
            "grinding is required before the screen cut",
            grinder_is_required,
        ),
        (
            "fine regrind rejects unscreened crusher feed",
            fine_grind_requires_screen_oversize,
        ),
        (
            "grinding resolves the authored screen cut",
            grinding_resolved_screen_cut,
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
            undersize_mass == Mass::from_milligrams(10),
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

fn coverage_gaps(reports: &[ScenarioReport]) -> Vec<&'static str> {
    let requirements = [
        (
            "structural consequence",
            reports
                .iter()
                .any(|report| report.structure.structural_consequence),
        ),
        (
            "stable structure",
            reports
                .iter()
                .any(|report| !report.structure.structural_consequence),
        ),
        (
            "persistent structural damage",
            reports
                .iter()
                .any(|report| report.structure.structural_damage_debt),
        ),
        (
            "forecast-changed siting",
            reports
                .iter()
                .any(|report| report.choices.forecast_changed_siting),
        ),
        (
            "structural stop",
            reports
                .iter()
                .any(|report| report.structure.structural_stop),
        ),
        (
            "production suspension",
            reports
                .iter()
                .any(|report| report.structure.production_suspension),
        ),
        (
            "stranded work in process",
            reports
                .iter()
                .any(|report| report.structure.stranded_work_in_process),
        ),
        (
            "recovered suspended work",
            reports.iter().any(|report| {
                report.structure.production_suspension && !report.structure.stranded_work_in_process
            }),
        ),
        (
            "production blocked by support failure",
            reports
                .iter()
                .any(|report| report.structure.support_failure_blocked_production),
        ),
        (
            "support relocation",
            reports
                .iter()
                .any(|report| report.structure.support_relocation),
        ),
        (
            "proactive relocation",
            reports.iter().any(|report| {
                report.structure.support_relocation
                    && !report.structure.support_failure_blocked_production
            }),
        ),
        (
            "forecast-driven power choice",
            reports
                .iter()
                .any(|report| report.choices.forecast_power_choice),
        ),
        (
            "energy bottleneck",
            reports.iter().any(|report| report.limits.energy_bottleneck),
        ),
        (
            "throughput bottleneck",
            reports
                .iter()
                .any(|report| report.limits.throughput_bottleneck),
        ),
        (
            "maintenance warning",
            reports
                .iter()
                .any(|report| report.limits.maintenance_warning),
        ),
        (
            "maintenance service",
            reports.iter().any(|report| report.maintenance.services > 0),
        ),
        (
            "scenario not requiring maintenance",
            reports
                .iter()
                .any(|report| report.maintenance.services == 0),
        ),
        (
            "maintenance service restores productive capacity",
            reports.iter().any(|report| {
                report.maintenance.services > 0
                    && report.progress.completed_batches == report.progress.target_batches
            }),
        ),
        (
            "completed work order",
            reports
                .iter()
                .any(|report| report.progress.completed_batches == report.progress.target_batches),
        ),
        (
            "incomplete work order",
            reports
                .iter()
                .any(|report| report.progress.completed_batches < report.progress.target_batches),
        ),
    ];

    requirements
        .into_iter()
        .filter_map(|(name, observed)| (!observed).then_some(name))
        .collect()
}

fn scenario_contract_gaps(reports: &[ScenarioReport]) -> Vec<String> {
    let mut gaps = Vec::new();
    for report in reports {
        if report.progress.completed_batches == 0 {
            gaps.push(format!(
                "seed 0x{:016X}: completed no production batch",
                report.seed
            ));
        }
        if !report.progress.disturbance_applied {
            gaps.push(format!(
                "seed 0x{:016X}: never reached its environmental disturbance",
                report.seed
            ));
        }
        if !report.progress.ore_frontier_visible {
            gaps.push(format!(
                "seed 0x{:016X}: did not expose the mixed-ore processing frontier",
                report.seed
            ));
        }
    }
    gaps
}

#[test]
fn gameplay_harness_agent_experience_matrix() {
    let registries = build_registries();
    assert_canonical_gameplay_content(&registries);
    let (seeds, enforce_coverage_matrix) = scenario_seeds()
        .unwrap_or_else(|error| panic!("gameplay harness configuration failed: {error:?}"));
    println!(
        "\n=== DEEP HEARTH WORKSHOP GAMEPLAY HARNESS: {} scenario(s), registry schema {} ===",
        seeds.len(),
        registries.schema_version().value(),
    );
    println!(
        "SETUP BOUNDARY: matter, equipment, finite energy, structural bays, and the reinforced bay's baseline load are starting conditions; every experienced decision and mutation after setup uses canonical runtime transactions."
    );
    println!(
        "WORKSHOP FANTASY: turn a constrained, failure-prone physical workshop into reliable production by reading structural margin, power reserve, machine condition, material state, and an approaching environmental load."
    );
    println!(
        "LOOP SCOPE: the scenario matrix experiences forecast-aware siting under imperfect load estimates, comminution, finite stored work, power-versus-time tradeoffs, wear, finite replacement-stock maintenance, exact-tick regional weather, persistent structural damage, production suspension, and recovery. Geological acquisition and construction authorization remain outside this workshop setup; separate ore-preparation and foundry probes validate existing downstream capabilities without pretending the mixed-ore chain is complete."
    );

    let reports: Vec<_> = seeds
        .into_iter()
        .map(ScenarioVariation::from_seed)
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
    let batches_before_disturbance: u32 = reports
        .iter()
        .map(|report| u32::from(report.progress.batches_before_disturbance))
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
    let ore_preparation_gaps = run_ore_preparation_capability_probe(&registries);
    let foundry_gaps = run_foundry_capability_probe(&registries);

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
    if enforce_coverage_matrix {
        gaps.extend(
            coverage_gaps(&reports)
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
        "HARNESS PASS mode={HARNESS_MODE} scenarios={} orders={completed_orders}/{} batches={completed_batches}/{target_batches} pre_disturbance={batches_before_disturbance} stops=[structural:{} maintenance:{} energy:{}] material=[ore_prep:pass foundry:pass mixed_ore_bridge:blocked]",
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
        "SYSTEMS control=[forecast_siting:{} forecast_power:{}] recovery=[relocations:{} resumed_wip:{recovered_work_in_process} stranded_wip:{} maintenance_services:{maintenance_services}] pressure=[structural:{} maintenance_warning:{}] bottlenecks=[energy_delivery:{} throughput:{}]",
        reports
            .iter()
            .filter(|report| report.choices.forecast_changed_siting)
            .count(),
        reports
            .iter()
            .filter(|report| report.choices.forecast_power_choice)
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
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_EXPLORATORY_SEED, GameplayHarnessConfigError, parse_seed, resolve_exploratory_seed,
        scenario_seeds_from,
    };

    #[test]
    fn parse_seed_accepts_decimal_and_hexadecimal() {
        assert_eq!(parse_seed("  42  "), Some(42));
        assert_eq!(parse_seed("0x2A"), Some(42));
        assert_eq!(parse_seed("0X2a"), Some(42));
        assert_eq!(parse_seed("18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_seed("0xFFFFFFFFFFFFFFFF"), Some(u64::MAX));
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed("not-a-seed"), None);
        assert_eq!(parse_seed("0x"), None);
    }

    #[test]
    fn exploratory_seed_uses_fixed_default_without_override() {
        assert_eq!(resolve_exploratory_seed(None), Ok(DEFAULT_EXPLORATORY_SEED));
    }

    #[test]
    fn exploratory_seed_accepts_explicit_override() {
        assert_eq!(resolve_exploratory_seed(Some("0xBAD")), Ok(0xBAD));
        assert_eq!(resolve_exploratory_seed(Some("2997")), Ok(2997));
    }

    #[test]
    fn exploratory_seed_rejects_invalid_override() {
        assert_eq!(
            resolve_exploratory_seed(Some("nope")),
            Err(GameplayHarnessConfigError::InvalidExploratorySeed)
        );
    }

    #[test]
    fn explicit_seed_list_rejects_invalid_entries_instead_of_dropping_them() {
        assert_eq!(
            scenario_seeds_from(Some("1,nope,4"), None),
            Err(GameplayHarnessConfigError::InvalidScenarioSeed { index: 1 })
        );
        assert_eq!(
            scenario_seeds_from(Some(""), None),
            Err(GameplayHarnessConfigError::EmptyScenarioSeedList)
        );
    }

    #[test]
    fn explicit_seed_list_is_exact_and_disables_default_matrix_contract() {
        assert_eq!(
            scenario_seeds_from(Some("1, 0x2A,3"), Some("ignored")),
            Ok((vec![1, 42, 3], false))
        );
    }
}
