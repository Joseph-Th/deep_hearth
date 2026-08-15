//! Headless workshop gameplay harness over the same canonical content registries used by the game.
//!
//! The harness deliberately varies physical initial conditions and lets a small operational policy
//! react only to observed state and resolver projections. Normal runs combine a deterministic
//! experience-coverage matrix with one time-derived exploratory seed that is printed for replay.
//! `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces that set with an exact comma-separated decimal or `0x`
//! hexadecimal seed list.

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{
    EnergyStoreId, EnergySupplyError, add_energy_store, add_energy_store_with_initial_for_test,
    calculate_mass_specific_energy,
};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, add_equipment, validate_mount_equipment,
    validate_unmount_equipment,
};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile, add_stockpile,
    add_stockpile_with_storage_profile, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::maintenance::{Condition, MaintenanceBand};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::ore_processing::{
    ComminutionBottleneck, ComminutionRequest, ComminutionResolutionError, ResolvedComminution,
    resolve_comminution_process,
};
use crate::production::validate_start_process;
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, StructuralStage,
    add_structural_element, materialize_structural_element_for_test,
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
use super::equipment::{EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_JAW_CRUSHER};
use super::processes::{PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_MELT_PURE_COPPER};
use super::{
    FORM_INGOT, FORM_LOG, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

#[derive(Clone, Copy, Debug)]
struct ScenarioVariation {
    seed: u64,
    ore_copper_ppm: u32,
    batch_mass: Mass,
    thermal_mass: Mass,
    initial_crusher_condition: Condition,
    compact_support_area: Area,
    reinforced_support_area: Area,
    reinforced_background_load: Force,
    disturbance_load: Force,
    planned_batches: u8,
    disturbance_after_batch: u8,
    large_drive_batch_budget: u8,
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
        let compact_area = 1_450_u64 + a % 351;
        let reinforced_area = compact_area + 300 + b % 351;
        Self {
            seed,
            ore_copper_ppm: 450_000 + (b % 300_001) as u32,
            batch_mass: Mass::from_milligrams(8 + c % 13),
            thermal_mass: Mass::from_milligrams(6 + d % 7),
            initial_crusher_condition: condition(650_000 + (e % 330_001) as u32),
            compact_support_area: Area::from_square_millimeters(compact_area),
            reinforced_support_area: Area::from_square_millimeters(reinforced_area),
            reinforced_background_load: Force::from_millinewtons(u128::from(
                4_000_000 + f % 12_000_001,
            )),
            disturbance_load: Force::from_millinewtons(u128::from(3_000_000 + g % 45_000_001)),
            planned_batches: 4 + (a % 3) as u8,
            disturbance_after_batch: 1 + (g % 2) as u8,
            large_drive_batch_budget: 1 + (h % 3) as u8,
        }
    }
}

#[derive(Clone, Copy)]
struct WorkshopIds {
    ore_source: StockpileId,
    crushed_storage: StockpileId,
    pure_copper_source: StockpileId,
    molten_vessel: StockpileId,
    cast_storage: StockpileId,
    ore_lot: MaterialLotId,
    pure_copper_lot: MaterialLotId,
    crusher: EquipmentId,
    furnace: EquipmentId,
    mold: EquipmentId,
    small_drive: EnergyStoreId,
    large_drive: EnergyStoreId,
    electrical_buffer: EnergyStoreId,
    heat_sink: EnergyStoreId,
    compact_support: StructuralElementId,
    reinforced_support: StructuralElementId,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioReport {
    structural_consequence: bool,
    structural_damage_debt: bool,
    support_failure_blocked_production: bool,
    support_relocation: bool,
    chose_compact_support: bool,
    used_small_drive: bool,
    used_large_drive: bool,
    large_drive_exhausted: bool,
    energy_bottleneck: bool,
    throughput_bottleneck: bool,
    maintenance_warning: bool,
    maintenance_stop: bool,
    ore_frontier_visible: bool,
    completed_foundry_cycle: bool,
    completed_batches: u8,
    target_batches: u8,
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

fn exploratory_seed() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    mix64((elapsed as u64) ^ ((elapsed >> 64) as u64) ^ u64::from(std::process::id()))
}

fn scenario_seeds() -> (Vec<u64>, bool) {
    if let Ok(raw) = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS") {
        let seeds: Vec<_> = raw.split(',').filter_map(parse_seed).collect();
        assert!(
            !seeds.is_empty(),
            "DEEP_HEARTH_GAMEPLAY_SEEDS contained no valid decimal or hexadecimal seeds"
        );
        return (seeds, false);
    }
    let mut seeds = vec![0xD33F_0101, 0xD33F_0102, 0xD33F_0103, 0xD33F_0104];
    seeds.push(exploratory_seed());
    (seeds, true)
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
}

fn setup_workshop(
    registries: &Registries,
    variation: ScenarioVariation,
) -> (AppState, WorkshopIds) {
    let mut state = AppState::new(WorldSeed::new(variation.seed));
    let ore_mass = variation.batch_mass.milligrams() * u64::from(variation.planned_batches);
    let ore_source = add_stockpile(&mut state, Mass::from_milligrams(ore_mass + 20))
        .unwrap_or_else(|error| panic!("gameplay harness ore stockpile failed: {error}"));
    let crushed_storage = add_stockpile(&mut state, Mass::from_milligrams(ore_mass + 20))
        .unwrap_or_else(|error| panic!("gameplay harness crushed storage failed: {error}"));
    let pure_copper_source = add_stockpile(&mut state, Mass::from_milligrams(30))
        .unwrap_or_else(|error| panic!("gameplay harness copper stockpile failed: {error}"));
    let vessel_profile =
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000))
            .unwrap_or_else(|error| {
                panic!("gameplay harness molten storage profile failed: {error}")
            });
    let molten_vessel =
        add_stockpile_with_storage_profile(&mut state, Mass::from_milligrams(30), vessel_profile)
            .unwrap_or_else(|error| panic!("gameplay harness molten vessel failed: {error}"));
    let cast_storage = add_stockpile(&mut state, Mass::from_milligrams(30))
        .unwrap_or_else(|error| panic!("gameplay harness cast storage failed: {error}"));

    let ore_lot = deposit_composed_lot_for_test(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(ore_mass),
        ROOM_TEMPERATURE,
        mixed_ore_composition(variation.ore_copper_ppm),
    )
    .unwrap_or_else(|error| panic!("gameplay harness ore seed failed: {error}"));
    let pure_copper_lot = deposit_lot_for_test(
        registries,
        &mut state,
        pure_copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        variation.thermal_mass,
        ROOM_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("gameplay harness copper seed failed: {error}"));

    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        variation.initial_crusher_condition,
    )
    .unwrap_or_else(|error| panic!("gameplay harness crusher allocation failed: {error}"));
    let furnace = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("gameplay harness furnace allocation failed: {error}"));
    let mold = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_CASTING_MOLD,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("gameplay harness mold allocation failed: {error}"));

    let small_drive = seed_energy_store(registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE, 1);
    let electrical_buffer = seed_energy_store(registries, &mut state, ENERGY_ELECTRICAL_BUFFER, 2);
    let heat_sink = add_energy_store(registries, &mut state, ENERGY_THERMAL_SINK)
        .unwrap_or_else(|error| panic!("gameplay harness thermal sink allocation failed: {error}"));

    let compact_support = active_support(registries, &mut state, 0, variation.compact_support_area);
    let reinforced_support =
        active_support(registries, &mut state, 2, variation.reinforced_support_area);
    let occupied_bay = validate_set_structural_load(
        registries,
        &state,
        reinforced_support,
        StructuralLoadKind::Permanent,
        variation.reinforced_background_load,
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
        calculate_mass_specific_energy(variation.batch_mass, comminution.specific_energy());
    let large_drive_energy = Energy::from_nanojoules(
        batch_energy.nanojoules() * u128::from(variation.large_drive_batch_budget),
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
            pure_copper_source,
            molten_vessel,
            cast_storage,
            ore_lot,
            pure_copper_lot,
            crusher,
            furnace,
            mold,
            small_drive,
            large_drive,
            electrical_buffer,
            heat_sink,
            compact_support,
            reinforced_support,
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
            available,
            requested,
            ..
        })) => {
            println!(
                "  power option {name}: unavailable, stored={}nJ required={}nJ",
                available.nanojoules(),
                requested.nanojoules()
            );
            None
        }
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
) -> Option<(CrushOption, &'static str)> {
    match (small, large) {
        (None, None) => None,
        (Some(option), None) | (None, Some(option)) => {
            if thresholds.classify(option.resolved.condition_after()) == MaintenanceBand::Critical {
                None
            } else {
                Some((option, "only viable energy source"))
            }
        }
        (Some(small), Some(large)) => {
            let current_band = thresholds.classify(small.resolved.condition_before());
            let small_after = thresholds.classify(small.resolved.condition_after());
            let large_after = thresholds.classify(large.resolved.condition_after());
            if small_after == MaintenanceBand::Critical && large_after != MaintenanceBand::Critical
            {
                Some((large, "high power avoids critical machine condition"))
            } else if current_band != MaintenanceBand::Normal
                || maintenance_band_rank(small_after) > maintenance_band_rank(current_band)
            {
                if large_after == MaintenanceBand::Critical {
                    None
                } else {
                    Some((large, "high power limits active-time wear"))
                }
            } else {
                Some((
                    small,
                    "preserve scarce high-power reserve while condition is healthy",
                ))
            }
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
) -> ComminutionBottleneck {
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
    if let Err(error) = start.commit(state) {
        panic!("gameplay harness crushing commit failed: {error}");
    }
    finish_operation(registries, state, duration);
    bottleneck
}

fn structural_assessment(
    analysis: &crate::structural::StructuralAnalysis,
    element: StructuralElementId,
) -> crate::structural::StructuralAssessment {
    analysis
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == element)
        .copied()
        .unwrap_or_else(|| panic!("gameplay harness structural assessment missing"))
}

fn relocate_crusher(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    current_support: &mut StructuralElementId,
    alternate_support: &mut StructuralElementId,
    report: &mut ScenarioReport,
) {
    let abandoned_support = *current_support;
    validate_unmount_equipment(registries, state, ids.crusher)
        .unwrap_or_else(|error| panic!("crusher recovery unmount failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("crusher recovery unmount commit failed: {error}"));
    let remount = validate_mount_equipment(registries, state, ids.crusher, *alternate_support)
        .unwrap_or_else(|error| panic!("crusher recovery remount failed: {error}"));
    let assessment = structural_assessment(remount.structural_analysis(), *alternate_support);
    assert_ne!(
        assessment.stage(),
        StructuralStage::Failed,
        "alternate workshop support must permit recovery"
    );
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
        report.structural_damage_debt = true;
        println!(
            "  recovery debt: previous bay remains {:?} cracked={} after relocation; restoring production did not repair the structure",
            abandoned.lifecycle(),
            abandoned.is_cracked(),
        );
    } else {
        println!(
            "  recovery note: previous bay retains {}mN disturbance load after relocation",
            abandoned.load(StructuralLoadKind::Snow).millinewtons(),
        );
    }
    std::mem::swap(current_support, alternate_support);
    report.support_relocation = true;
}

fn apply_disturbance_and_adapt(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    variation: ScenarioVariation,
    current_support: &mut StructuralElementId,
    alternate_support: &mut StructuralElementId,
    report: &mut ScenarioReport,
) {
    let disturbance = validate_set_structural_load(
        registries,
        state,
        *current_support,
        StructuralLoadKind::Snow,
        variation.disturbance_load,
    )
    .unwrap_or_else(|error| panic!("workshop disturbance validation failed: {error}"));
    let after = structural_assessment(disturbance.analysis(), *current_support);
    report.structural_consequence = after.stage() != StructuralStage::Stable;
    println!(
        "  disturbance: +{}mN on active bay -> {:?}/{}ppm utilization",
        variation.disturbance_load.millinewtons(),
        after.stage(),
        after.utilization_ppm()
    );
    disturbance
        .commit(state)
        .unwrap_or_else(|error| panic!("workshop disturbance commit failed: {error}"));

    if after.stage() == StructuralStage::Failed {
        let selection = [MaterialLotSelection::new(ids.ore_lot, variation.batch_mass)];
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
        report.support_failure_blocked_production = matches!(
            blocked,
            Err(ComminutionResolutionError::Equipment(
                EquipmentProviderError::StructuralSupportNotActive { .. }
            ))
        );
        println!(
            "  consequence: failed support blocks production={}",
            report.support_failure_blocked_production
        );
        relocate_crusher(
            registries,
            state,
            ids,
            current_support,
            alternate_support,
            report,
        );
        return;
    }

    if after.stage() == StructuralStage::Cracking || after.stage() == StructuralStage::Strained {
        let mut preview = state.clone();
        validate_unmount_equipment(registries, &preview, ids.crusher)
            .unwrap_or_else(|error| panic!("crusher relocation preview unmount failed: {error}"))
            .commit(&mut preview)
            .unwrap_or_else(|error| panic!("crusher relocation preview commit failed: {error}"));
        let alternate =
            validate_mount_equipment(registries, &preview, ids.crusher, *alternate_support)
                .unwrap_or_else(|error| panic!("crusher relocation preview mount failed: {error}"));
        let alternate_assessment =
            structural_assessment(alternate.structural_analysis(), *alternate_support);
        if (
            stage_rank(alternate_assessment.stage()),
            alternate_assessment.utilization_ppm(),
        ) < (stage_rank(after.stage()), after.utilization_ppm())
        {
            println!(
                "  decision: alternate bay is safer at {:?}/{}ppm; relocate before failure",
                alternate_assessment.stage(),
                alternate_assessment.utilization_ppm()
            );
            relocate_crusher(
                registries,
                state,
                ids,
                current_support,
                alternate_support,
                report,
            );
        } else {
            println!("  decision: remain on current support; alternate bay is not safer");
        }
    }
}

fn run_scenario(registries: &Registries, variation: ScenarioVariation) -> ScenarioReport {
    let (mut state, ids) = setup_workshop(registries, variation);
    let mut report = ScenarioReport {
        target_batches: variation.planned_batches,
        ..ScenarioReport::default()
    };
    println!(
        "\nSCENARIO seed=0x{:016X} ore={}ppm Cu batch={}mg crusher={}ppm target_batches={} disturbance_after={} large_drive_budget={} batch(es)",
        variation.seed,
        variation.ore_copper_ppm,
        variation.batch_mass.milligrams(),
        variation.initial_crusher_condition.parts_per_million(),
        variation.planned_batches,
        variation.disturbance_after_batch,
        variation.large_drive_batch_budget,
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
        "  support options: compact={:?}/{}ppm reinforced={:?}/{}ppm (reinforced existing load={}mN)",
        compact_assessment.stage(),
        compact_assessment.utilization_ppm(),
        reinforced_assessment.stage(),
        reinforced_assessment.utilization_ppm(),
        variation.reinforced_background_load.millinewtons(),
    );
    let compact_is_better = (
        stage_rank(compact_assessment.stage()),
        compact_assessment.utilization_ppm(),
    ) < (
        stage_rank(reinforced_assessment.stage()),
        reinforced_assessment.utilization_ppm(),
    );
    let (mut current_support, mut alternate_support, selected_mount, support_name) =
        if compact_is_better {
            report.chose_compact_support = true;
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
    assert_ne!(selected_assessment.stage(), StructuralStage::Failed);
    println!("  decision: mount crusher on {support_name}");
    selected_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected crusher mount failed: {error}"));

    let thresholds = registries
        .equipment()
        .get_equipment(EQUIPMENT_JAW_CRUSHER)
        .unwrap_or_else(|| panic!("canonical crusher definition disappeared"))
        .maintenance_thresholds();
    for batch_index in 0..variation.planned_batches {
        let current_condition = state
            .equipment()
            .get_equipment(ids.crusher)
            .map(|record| record.condition())
            .unwrap_or_else(|| panic!("crusher disappeared during gameplay harness"));
        let band = thresholds.classify(current_condition);
        if band != MaintenanceBand::Normal {
            report.maintenance_warning = true;
            println!(
                "  maintenance: condition={}ppm band={band:?}",
                current_condition.parts_per_million()
            );
        }
        if band == MaintenanceBand::Critical {
            report.maintenance_stop = true;
            println!("  decision: stop crushing; machine is already in critical condition");
            println!(
                "  maintenance frontier: no free condition reset is available; service requires a physically resolved resource/tool/labor path"
            );
            break;
        }

        let small = resolve_crush_option(
            registries,
            &state,
            ids,
            variation.batch_mass,
            "small",
            ids.small_drive,
        );
        let large = resolve_crush_option(
            registries,
            &state,
            ids,
            variation.batch_mass,
            "large",
            ids.large_drive,
        );
        if let Some(option) = &small {
            print_crush_option(option, thresholds);
        }
        if let Some(option) = &large {
            print_crush_option(option, thresholds);
        } else {
            report.large_drive_exhausted = true;
        }
        let Some((selected, reason)) = choose_crush_option(small, large, thresholds) else {
            report.maintenance_stop = true;
            println!(
                "  decision: stop crushing; remaining power choices would enter critical condition or no source can supply the batch"
            );
            println!(
                "  maintenance frontier: no free condition reset is available; service requires a physically resolved resource/tool/labor path"
            );
            break;
        };
        println!("  decision: use {} drive because {reason}", selected.name);
        if selected.store == ids.small_drive {
            report.used_small_drive = true;
        } else if selected.store == ids.large_drive {
            report.used_large_drive = true;
        }
        let bottleneck = crush_batch(
            registries,
            &mut state,
            ids,
            variation.batch_mass,
            selected,
            batch_index + 1,
        );
        report.completed_batches += 1;
        match bottleneck {
            ComminutionBottleneck::Throughput => report.throughput_bottleneck = true,
            ComminutionBottleneck::EnergyDelivery => report.energy_bottleneck = true,
            ComminutionBottleneck::Balanced => {
                report.energy_bottleneck = true;
                report.throughput_bottleneck = true;
            }
        }
        if batch_index + 1 == variation.disturbance_after_batch {
            apply_disturbance_and_adapt(
                registries,
                &mut state,
                ids,
                variation,
                &mut current_support,
                &mut alternate_support,
                &mut report,
            );
        }
    }
    let final_condition = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("crusher disappeared after gameplay harness crushing"));
    if thresholds.classify(final_condition) != MaintenanceBand::Normal {
        report.maintenance_warning = true;
    }

    let crushed_lot = stockpile_first_lot(&state, ids.crushed_storage);
    let crushed_mass = state
        .inventory()
        .get_stockpile(ids.crushed_storage)
        .map(|stockpile| stockpile.stored_mass())
        .unwrap_or_else(|| panic!("crushed storage disappeared"));
    println!(
        "  material: crushed={}mg composition={}ppm Cu / {}ppm gangue",
        crushed_mass.milligrams(),
        variation.ore_copper_ppm,
        1_000_000 - variation.ore_copper_ppm,
    );
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
    report.ore_frontier_visible = matches!(
        blocked_melt,
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::ImpureInput { .. }
        ))
    );
    println!(
        "  process frontier: crushed mixed ore cannot enter pure-copper melting={} (concentration/smelting remains the missing bridge)",
        report.ore_frontier_visible
    );

    println!("  downstream foundry: exercise existing pure-copper feedstock through melt and cast");
    let pure_selection = [MaterialLotSelection::new(
        ids.pure_copper_lot,
        variation.thermal_mass,
    )];
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
    .unwrap_or_else(|error| panic!("pure-copper melt failed: {error}"));
    let melt_duration = melt.process_resolution().duration();
    println!(
        "  melt: mass={}mg energy={}nJ power={}uW duration={}t",
        variation.thermal_mass.milligrams(),
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
    .unwrap_or_else(|error| panic!("melt start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("melt commit failed: {error}"));
    finish_operation(registries, &mut state, melt_duration);

    let molten_lot = stockpile_first_lot(&state, ids.molten_vessel);
    let molten_selection = [MaterialLotSelection::new(
        molten_lot,
        variation.thermal_mass,
    )];
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
    .unwrap_or_else(|error| panic!("pure-copper casting failed: {error}"));
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
    .unwrap_or_else(|error| panic!("casting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("casting commit failed: {error}"));
    finish_operation(registries, &mut state, cast_duration);
    report.completed_foundry_cycle = state
        .inventory()
        .get_stockpile(ids.cast_storage)
        .is_some_and(|stockpile| stockpile.stored_mass() == variation.thermal_mass);

    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    println!(
        "  report: batches={}/{} structural_change={} damage_debt={} support_block={} relocation={} small_drive={} large_drive={} large_exhausted={} energy_limit={} throughput_limit={} maintenance_warning={} maintenance_stop={} ore_frontier={} foundry_cycle={} ticks={}",
        report.completed_batches,
        variation.planned_batches,
        report.structural_consequence,
        report.structural_damage_debt,
        report.support_failure_blocked_production,
        report.support_relocation,
        report.used_small_drive,
        report.used_large_drive,
        report.large_drive_exhausted,
        report.energy_bottleneck,
        report.throughput_bottleneck,
        report.maintenance_warning,
        report.maintenance_stop,
        report.ore_frontier_visible,
        report.completed_foundry_cycle,
        state.tick().value(),
    );
    report
}

#[test]
fn gameplay_harness_agent_experience_matrix() {
    let registries = build_registries();
    assert_canonical_gameplay_content(&registries);
    let (seeds, enforce_coverage_matrix) = scenario_seeds();
    println!(
        "\n=== DEEP HEARTH WORKSHOP GAMEPLAY HARNESS: {} scenario(s), registry schema {} ===",
        seeds.len(),
        registries.schema_version().value(),
    );
    println!(
        "SETUP BOUNDARY: matter, equipment, finite energy, structural bays, and the reinforced bay's baseline load are starting conditions; every experienced decision and mutation after setup uses canonical runtime transactions."
    );
    println!(
        "LOOP SCOPE: the harness experiences workshop siting, comminution, finite power, wear, environmental structural stress, and recovery. Geological acquisition and construction authorization are outside this workshop setup; crushed mixed ore currently reaches an explicit concentration/smelting frontier before the separate pure-copper foundry path."
    );

    let reports: Vec<_> = seeds
        .into_iter()
        .map(ScenarioVariation::from_seed)
        .map(|variation| run_scenario(&registries, variation))
        .collect();

    let completed_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.completed_batches))
        .sum();
    let target_batches: u32 = reports
        .iter()
        .map(|report| u32::from(report.target_batches))
        .sum();
    println!(
        "\nEXPERIENCE SUMMARY: batches={completed_batches}/{target_batches} compact_choices={} reinforced_choices={} disruptions={} damage_debt={} relocations={} blocked_by_failure={} small_drive={} large_drive={} large_exhausted={} energy_bottlenecks={} throughput_bottlenecks={} maintenance_warnings={} maintenance_stops={} ore_frontier={} foundry_cycles={}",
        reports
            .iter()
            .filter(|report| report.chose_compact_support)
            .count(),
        reports
            .iter()
            .filter(|report| !report.chose_compact_support)
            .count(),
        reports
            .iter()
            .filter(|report| report.structural_consequence)
            .count(),
        reports
            .iter()
            .filter(|report| report.structural_damage_debt)
            .count(),
        reports
            .iter()
            .filter(|report| report.support_relocation)
            .count(),
        reports
            .iter()
            .filter(|report| report.support_failure_blocked_production)
            .count(),
        reports
            .iter()
            .filter(|report| report.used_small_drive)
            .count(),
        reports
            .iter()
            .filter(|report| report.used_large_drive)
            .count(),
        reports
            .iter()
            .filter(|report| report.large_drive_exhausted)
            .count(),
        reports
            .iter()
            .filter(|report| report.energy_bottleneck)
            .count(),
        reports
            .iter()
            .filter(|report| report.throughput_bottleneck)
            .count(),
        reports
            .iter()
            .filter(|report| report.maintenance_warning)
            .count(),
        reports
            .iter()
            .filter(|report| report.maintenance_stop)
            .count(),
        reports
            .iter()
            .filter(|report| report.ore_frontier_visible)
            .count(),
        reports
            .iter()
            .filter(|report| report.completed_foundry_cycle)
            .count(),
    );

    assert!(reports.iter().all(|report| report.completed_batches > 0));
    assert!(reports.iter().all(|report| report.ore_frontier_visible));
    assert!(reports.iter().all(|report| report.completed_foundry_cycle));
    if enforce_coverage_matrix {
        assert!(reports.iter().any(|report| report.structural_consequence));
        assert!(reports.iter().any(|report| !report.structural_consequence));
        assert!(reports.iter().any(|report| report.structural_damage_debt));
        assert!(
            reports
                .iter()
                .any(|report| report.support_failure_blocked_production)
        );
        assert!(reports.iter().any(|report| report.support_relocation));
        assert!(reports.iter().any(|report| !report.support_relocation));
        assert!(reports.iter().any(|report| {
            report.support_relocation && !report.support_failure_blocked_production
        }));
        assert!(reports.iter().any(|report| report.chose_compact_support));
        assert!(reports.iter().any(|report| !report.chose_compact_support));
        assert!(reports.iter().any(|report| report.used_small_drive));
        assert!(reports.iter().any(|report| report.used_large_drive));
        assert!(reports.iter().any(|report| report.large_drive_exhausted));
        assert!(reports.iter().any(|report| report.energy_bottleneck));
        assert!(reports.iter().any(|report| report.throughput_bottleneck));
        assert!(reports.iter().any(|report| report.maintenance_warning));
        assert!(reports.iter().any(|report| report.maintenance_stop));
        assert!(
            reports
                .iter()
                .any(|report| report.completed_batches == report.target_batches)
        );
        assert!(
            reports
                .iter()
                .any(|report| report.completed_batches < report.target_batches)
        );
    }
}
