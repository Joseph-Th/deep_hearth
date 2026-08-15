//! Agent-facing gameplay harness over the same canonical content registries used by the game.
//!
//! The harness deliberately varies initial conditions and lets a small policy react to observed
//! state. Normal runs combine a deterministic coverage matrix with one time-derived exploratory seed
//! that is printed for replay. `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces that set with an exact
//! comma-separated decimal or `0x` hexadecimal seed list.

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{EnergyStoreId, add_energy_store, add_energy_store_with_initial_for_test};
use crate::equipment::{
    EquipmentId, EquipmentProviderError, add_equipment, validate_mount_equipment,
};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile, add_stockpile,
    add_stockpile_with_storage_profile, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::maintenance::{Condition, MaintenanceBand};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::ore_processing::{
    ComminutionBottleneck, ComminutionRequest, ComminutionResolutionError,
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
    risky_support_area: Area,
    safe_support_area: Area,
    secondary_load: Force,
    planned_batches: u8,
    bootstrap_with_small_drive: bool,
    cautious_maintenance: bool,
}

impl ScenarioVariation {
    fn from_seed(seed: u64) -> Self {
        let a = mix64(seed);
        let b = mix64(a);
        let c = mix64(b);
        let d = mix64(c);
        let e = mix64(d);
        let f = mix64(e);
        let risky_area = 900_u64 + a % 301;
        Self {
            seed,
            ore_copper_ppm: 450_000 + (b % 300_001) as u32,
            batch_mass: Mass::from_milligrams(8 + c % 13),
            thermal_mass: Mass::from_milligrams(6 + d % 7),
            initial_crusher_condition: condition(720_000 + (e % 280_001) as u32),
            risky_support_area: Area::from_square_millimeters(risky_area),
            safe_support_area: Area::from_square_millimeters(risky_area * 2),
            secondary_load: Force::from_millinewtons(u128::from(750_000 + f % 4_250_001)),
            planned_batches: 3 + (a % 3) as u8,
            bootstrap_with_small_drive: b & 1 == 0,
            cautious_maintenance: c & 1 == 0,
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
}

#[derive(Clone, Copy, Debug, Default)]
struct ScenarioReport {
    structural_consequence: bool,
    support_failure_blocked_production: bool,
    energy_bottleneck: bool,
    throughput_bottleneck: bool,
    maintenance_warning: bool,
    mixed_ore_gate: bool,
    completed_metal_cycle: bool,
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
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
    let large_drive = seed_energy_store(registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE, 1);
    let electrical_buffer = seed_energy_store(registries, &mut state, ENERGY_ELECTRICAL_BUFFER, 2);
    let heat_sink = add_energy_store(registries, &mut state, ENERGY_THERMAL_SINK)
        .unwrap_or_else(|error| panic!("gameplay harness thermal sink allocation failed: {error}"));

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

fn crush_duration_for(
    registries: &Registries,
    state: &AppState,
    ids: WorkshopIds,
    mass: Mass,
    energy: EnergyStoreId,
) -> TickSpan {
    let selection = [MaterialLotSelection::new(ids.ore_lot, mass)];
    resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &selection,
            ids.crusher,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("gameplay harness crusher option resolution failed: {error}"))
    .process_resolution()
    .duration()
}

fn crush_batch(
    registries: &Registries,
    state: &mut AppState,
    ids: WorkshopIds,
    mass: Mass,
    energy: EnergyStoreId,
    batch_index: u8,
) -> ComminutionBottleneck {
    let selection = [MaterialLotSelection::new(ids.ore_lot, mass)];
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            ids.ore_source,
            &selection,
            ids.crusher,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("gameplay harness crushing resolution failed: {error}"));
    let condition_before = state
        .equipment()
        .get_equipment(ids.crusher)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("gameplay harness crusher disappeared"));
    println!(
        "  crush#{batch_index}: mass={}mg condition={}ppm rate={}mg/s power={}uW duration={}t constraints=[throughput:{}t energy:{}t] bottleneck={:?}",
        mass.milligrams(),
        condition_before.parts_per_million(),
        resolved.processing_rate().milligrams_per_second(),
        resolved.available_power().whole_microwatts(),
        resolved.process_resolution().duration().value(),
        resolved.throughput_duration().value(),
        resolved.energy_duration().value(),
        resolved.bottleneck(),
    );
    let duration = resolved.process_resolution().duration();
    let bottleneck = resolved.bottleneck();
    let start = validate_start_process(
        registries,
        state,
        resolved.process_resolution(),
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

fn run_scenario(registries: &Registries, variation: ScenarioVariation) -> ScenarioReport {
    let (mut state, ids) = setup_workshop(registries, variation);
    let mut report = ScenarioReport::default();
    println!(
        "\nSCENARIO seed=0x{:016X} ore={}ppm Cu batch={}mg crusher={}ppm batches={} bootstrap_small={} cautious_maintenance={}",
        variation.seed,
        variation.ore_copper_ppm,
        variation.batch_mass.milligrams(),
        variation.initial_crusher_condition.parts_per_million(),
        variation.planned_batches,
        variation.bootstrap_with_small_drive,
        variation.cautious_maintenance,
    );

    let risky_support = active_support(registries, &mut state, 0, variation.risky_support_area);
    let safe_support = active_support(registries, &mut state, 2, variation.safe_support_area);
    let risky_mount = validate_mount_equipment(registries, &state, ids.crusher, risky_support)
        .unwrap_or_else(|error| panic!("risky mount prediction failed: {error}"));
    let safe_mount = validate_mount_equipment(registries, &state, ids.crusher, safe_support)
        .unwrap_or_else(|error| panic!("safe mount prediction failed: {error}"));
    let risky_assessment = risky_mount
        .structural_analysis()
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == risky_support)
        .copied()
        .unwrap_or_else(|| panic!("risky support assessment missing"));
    let safe_assessment = safe_mount
        .structural_analysis()
        .assessments()
        .iter()
        .find(|assessment| assessment.element() == safe_support)
        .copied()
        .unwrap_or_else(|| panic!("safe support assessment missing"));
    assert!(
        (
            stage_rank(safe_assessment.stage()),
            safe_assessment.utilization_ppm()
        ) < (
            stage_rank(risky_assessment.stage()),
            risky_assessment.utilization_ppm()
        ),
        "authored doubled support stopped being the safer crusher mount"
    );
    println!(
        "  support choice: risky={:?}/{}ppm safe={:?}/{}ppm -> agent selects safer support",
        risky_assessment.stage(),
        risky_assessment.utilization_ppm(),
        safe_assessment.stage(),
        safe_assessment.utilization_ppm(),
    );

    let mut consequence_branch = state.clone();
    risky_mount
        .commit(&mut consequence_branch)
        .unwrap_or_else(|error| panic!("risky branch mount failed: {error}"));
    if risky_assessment.stage() == StructuralStage::Failed {
        report.structural_consequence = true;
        println!("  alternate branch: crusher weight alone fails the risky support");
    } else {
        let secondary = validate_set_structural_load(
            registries,
            &consequence_branch,
            risky_support,
            StructuralLoadKind::Snow,
            variation.secondary_load,
        )
        .unwrap_or_else(|error| panic!("secondary-load branch failed: {error}"));
        let after_secondary = secondary
            .analysis()
            .assessments()
            .iter()
            .find(|assessment| assessment.element() == risky_support)
            .copied()
            .unwrap_or_else(|| panic!("secondary-load assessment missing"));
        report.structural_consequence = after_secondary.stage() != risky_assessment.stage();
        println!(
            "  alternate branch: +{}mN environmental load changes {:?} -> {:?}",
            variation.secondary_load.millinewtons(),
            risky_assessment.stage(),
            after_secondary.stage(),
        );
        secondary
            .commit(&mut consequence_branch)
            .unwrap_or_else(|error| panic!("secondary-load commit failed: {error}"));
    }
    if consequence_branch
        .structures()
        .get_element(risky_support)
        .is_some_and(|record| record.lifecycle() == StructuralLifecycle::Failed)
    {
        let selection = [MaterialLotSelection::new(ids.ore_lot, variation.batch_mass)];
        let blocked = resolve_comminution_process(
            registries,
            &consequence_branch,
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
            "  alternate branch: failed support blocks crusher={}",
            report.support_failure_blocked_production
        );
    }

    safe_mount
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("safe crusher mount failed: {error}"));

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
            if variation.cautious_maintenance && batch_index > 0 {
                println!(
                    "  policy: cautious agent stops additional crushing at maintenance warning"
                );
                break;
            }
        }

        let small_duration = crush_duration_for(
            registries,
            &state,
            ids,
            variation.batch_mass,
            ids.small_drive,
        );
        let large_duration = crush_duration_for(
            registries,
            &state,
            ids,
            variation.batch_mass,
            ids.large_drive,
        );
        let selected_drive = if batch_index == 0 && variation.bootstrap_with_small_drive {
            ids.small_drive
        } else if large_duration < small_duration {
            ids.large_drive
        } else {
            ids.small_drive
        };
        println!(
            "  power choice: small={}t large={}t selected={}",
            small_duration.value(),
            large_duration.value(),
            if selected_drive == ids.small_drive {
                "small"
            } else {
                "large"
            }
        );
        let bottleneck = crush_batch(
            registries,
            &mut state,
            ids,
            variation.batch_mass,
            selected_drive,
            batch_index + 1,
        );
        match bottleneck {
            ComminutionBottleneck::Throughput => report.throughput_bottleneck = true,
            ComminutionBottleneck::EnergyDelivery => report.energy_bottleneck = true,
            ComminutionBottleneck::Balanced => {
                report.energy_bottleneck = true;
                report.throughput_bottleneck = true;
            }
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
    report.mixed_ore_gate = matches!(
        blocked_melt,
        Err(MeltingResolutionError::Batch(
            MeltingBatchError::ImpureInput { .. }
        ))
    );
    println!(
        "  metallurgy gate: mixed crushed ore rejected={}",
        report.mixed_ore_gate
    );

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
    report.completed_metal_cycle = state
        .inventory()
        .get_stockpile(ids.cast_storage)
        .is_some_and(|stockpile| stockpile.stored_mass() == variation.thermal_mass);

    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    println!(
        "  report: structural_change={} support_block={} energy_limit={} throughput_limit={} maintenance={} mixed_gate={} metal_cycle={}",
        report.structural_consequence,
        report.support_failure_blocked_production,
        report.energy_bottleneck,
        report.throughput_bottleneck,
        report.maintenance_warning,
        report.mixed_ore_gate,
        report.completed_metal_cycle,
    );
    report
}

#[test]
fn gameplay_harness_agent_experience_matrix() {
    let registries = build_registries();
    assert_canonical_gameplay_content(&registries);
    let (seeds, enforce_coverage_matrix) = scenario_seeds();
    println!(
        "\n=== DEEP HEARTH AGENT GAMEPLAY HARNESS: {} scenario(s), registry schema {} ===",
        seeds.len(),
        registries.schema_version().value(),
    );
    println!(
        "SETUP BOUNDARY: scenario matter and initial stored energy are seeded starting conditions; all decisions and mutations after setup use canonical game registries and runtime transactions."
    );

    let reports: Vec<_> = seeds
        .into_iter()
        .map(ScenarioVariation::from_seed)
        .map(|variation| run_scenario(&registries, variation))
        .collect();

    assert!(reports.iter().all(|report| report.mixed_ore_gate));
    assert!(reports.iter().all(|report| report.completed_metal_cycle));
    if enforce_coverage_matrix {
        assert!(reports.iter().any(|report| report.structural_consequence));
        assert!(
            reports
                .iter()
                .any(|report| report.support_failure_blocked_production)
        );
        assert!(reports.iter().any(|report| report.energy_bottleneck));
        assert!(reports.iter().any(|report| report.throughput_bottleneck));
        assert!(reports.iter().any(|report| report.maintenance_warning));
    }
}
