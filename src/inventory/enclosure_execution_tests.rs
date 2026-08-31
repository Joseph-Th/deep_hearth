//! Construction, conservation, structural-load, and persistence contracts for storage enclosures.

use super::*;

use crate::content::{
    FORM_CHEST_BODY, FORM_FOOD, FORM_LOG, MATERIAL_BERRIES, MATERIAL_WOOD,
    PROCESS_SHAPE_WOOD_BOARDS, STORAGE_TIMBER_PROVISIONS_CHEST,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{AggregateMass, Area, Length, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::calculate_explicit_energy_accounting;
use crate::inventory::{
    MaterialLotId, StockpileStorageError, StorageEnclosureValidationError,
    add_solid_stockpile_for_test, deposit_lot_for_test, validate_mount_stockpile,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLoadKind, add_structural_element,
    calculate_aggregate_weight_force_ceiling, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::survival::{FoodFreshness, assess_food_freshness, initialize_player_survival};

const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const CHEST_MASS: Mass = Mass::from_milligrams(2_400_000);

fn construction_fixture(
    target_capacity: Mass,
) -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5702_1001));
    let target = add_solid_stockpile_for_test(&mut state, target_capacity)
        .unwrap_or_else(|error| panic!("preservation target fixture failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("preservation food fixture failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, CHEST_MASS)
        .unwrap_or_else(|error| panic!("preservation construction source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("preservation chest-body fixture failed: {error}"));
    (registries, state, target, source, food)
}

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("preservation fixture tick failed: {error}"));
    }
}

fn active_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("preservation support bounds failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("preservation support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("preservation support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("preservation support activation commit failed: {error}"));
    support
}

#[test]
fn built_preservation_enclosure_conserves_matter_and_only_slows_future_spoilage() {
    let (registries, mut state, target, source, food) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    advance_exact(&registries, &mut state, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(100)
    ));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("preservation matter-before audit failed: {error}"));
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("preservation energy-before audit failed: {error}"))
        .total();

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("preservation enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("preservation enclosure commit failed: {error}"));

    let target_record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("preservation target disappeared after construction"));
    assert_eq!(
        target_record
            .storage_profile()
            .preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(target_record.embodied_mass(), CHEST_MASS);
    assert_eq!(
        target_record.enclosure().map(|record| record.definition()),
        Some(STORAGE_TIMBER_PROVISIONS_CHEST)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(100)
    ));
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("preservation matter-after audit failed: {error}"));
    assert_eq!(matter_after.total(), matter_before.total());
    assert_eq!(
        matter_after.storage_infrastructure(),
        AggregateMass::from_mass(CHEST_MASS)
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("preservation energy-after audit failed: {error}"))
            .total(),
        energy_before
    );

    advance_exact(&registries, &mut state, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(150)
    ));
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("preservation final state audit failed: {error}"));
}

#[test]
fn enclosure_rejects_oversized_or_already_improved_stockpile_without_mutation() {
    let (registries, state, target, source, _) =
        construction_fixture(Mass::from_milligrams(20_000_001));
    let before = state.clone();
    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            source,
        )
        .err(),
        Some(StorageEnclosureConstructionError::TargetCapacityTooLarge {
            stockpile: target,
            capacity: Mass::from_milligrams(20_000_001),
            maximum: Mass::from_milligrams(20_000_000),
        })
    );
    assert_eq!(state, before);

    let (registries, mut state, target, source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("first enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first enclosure commit failed: {error}"));
    let after_first = state.clone();
    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            source,
        )
        .err(),
        Some(StorageEnclosureConstructionError::AlreadyEnclosed {
            stockpile: target,
            definition: STORAGE_TIMBER_PROVISIONS_CHEST,
        })
    );
    assert_eq!(state, after_first);
}

#[test]
fn enclosure_rejects_existing_contents_outside_completed_profile_without_mutation() {
    let (registries, mut state, target, source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    let hot_temperature = Temperature::from_millikelvin(340_000);
    let hot_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(50_000),
        hot_temperature,
    )
    .unwrap_or_else(|error| panic!("hot provisions fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            source,
        )
        .err(),
        Some(
            StorageEnclosureConstructionError::TargetContentsIncompatible {
                lot: hot_lot,
                error: StockpileStorageError::TemperatureExceedsMaximum {
                    stockpile: target,
                    temperature: hot_temperature,
                    maximum: Temperature::from_millikelvin(333_150),
                },
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn enclosure_allows_incompatible_construction_lot_when_target_source_consumes_it_fully() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5702_1008));
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("self-enclosure target fixture failed: {error}"));
    let hot_temperature = Temperature::from_millikelvin(340_000);
    let construction_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        hot_temperature,
    )
    .unwrap_or_else(|error| panic!("self-enclosure construction lot failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("self-enclosure matter-before audit failed: {error}"))
        .total();

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        target,
    )
    .unwrap_or_else(|error| panic!("self-enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("self-enclosure commit failed: {error}"));

    let record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("self-enclosure target disappeared"));
    assert_eq!(record.stored_mass(), Mass::ZERO);
    assert_eq!(record.embodied_mass(), CHEST_MASS);
    assert!(state.inventory().get_lot(construction_lot).is_none());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("self-enclosure matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn enclosure_rejects_storage_profile_change_while_inbound_output_is_reserved() {
    let (registries, mut state, target, enclosure_source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    let craft_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("reserved-enclosure craft source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        craft_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("reserved-enclosure craft input failed: {error}"));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("reserved-enclosure survival setup failed: {error}"));
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_BOARDS, craft_source, target),
    )
    .unwrap_or_else(|error| panic!("reserved-enclosure craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reserved-enclosure craft commit failed: {error}"));

    assert_eq!(
        state
            .inventory()
            .get_stockpile(target)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(1_000_000))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();

    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            enclosure_source,
        )
        .err(),
        Some(
            StorageEnclosureConstructionError::TargetHasReservedInbound {
                stockpile: target,
                reserved: Mass::from_milligrams(1_000_000),
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn mounting_enclosed_stockpile_loads_contents_and_enclosure_body() {
    let (registries, mut state, target, source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("supported enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported enclosure commit failed: {error}"));
    let support = active_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, target, support)
        .unwrap_or_else(|error| panic!("enclosed stockpile mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("enclosed stockpile mount commit failed: {error}"));

    let supported_mass = CHEST_MASS
        .checked_add(Mass::from_milligrams(100_000))
        .unwrap_or_else(|| unreachable!("bounded preservation fixture mass cannot overflow"));
    let expected = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(supported_mass),
        registries.core().gravity(),
    )
    .unwrap_or_else(|| panic!("enclosed stockpile expected weight overflowed"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("supported enclosure state audit failed: {error}"));
}

#[test]
fn load_replays_storage_definition_profile_and_embodied_matter() {
    let (registries, mut state, target, source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("persistence enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("persistence enclosure commit failed: {error}"));

    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("enclosure serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded.clone())
        .unwrap_or_else(|error| panic!("enclosure decode failed: {error}"));
    assert_eq!(
        decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("enclosure round-trip load failed: {error}")),
        state
    );

    let target_key = target.value().to_string();
    let mut obsolete_mass = encoded.clone();
    obsolete_mass["state"]["systems"]["inventory"]["stockpiles"][&target_key]["enclosure"]["embodied_mass"] =
        serde_json::json!(2_300_000_u64);
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(obsolete_mass).is_err());

    let mut overflowed_traces = encoded.clone();
    let traces =
        overflowed_traces["state"]["systems"]["inventory"]["stockpiles"][&target_key]["enclosure"]
            ["embodied_material"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("storage enclosure lost embodied trace array"));
    let mut duplicate = traces
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("storage enclosure lost embodied material"));
    traces[0]["mass"] = serde_json::json!(u64::MAX);
    duplicate["mass"] = serde_json::json!(u64::MAX);
    traces.push(duplicate);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(overflowed_traces)
        .unwrap_or_else(|error| panic!("enclosure trace-overflow decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::StorageEnclosure(
                StorageEnclosureValidationError::EmbodiedTraceMassOverflow { stockpile: target }
            )
        ))
    );

    let mut forged_profile = encoded;
    forged_profile["state"]["systems"]["inventory"]["stockpiles"][&target_key]["storage_profile"]
        ["preservation_multiplier_ppm"] = serde_json::json!(3_000_000_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged_profile)
        .unwrap_or_else(|error| panic!("forged enclosure profile decode failed: {error}"));
    assert!(matches!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::StorageEnclosure(
            StorageEnclosureValidationError::StorageProfileMismatch { stockpile, .. }
        ))) if stockpile == target
    ));
}
