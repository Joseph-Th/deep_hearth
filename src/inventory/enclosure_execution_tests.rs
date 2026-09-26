//! Construction, conservation, structural-load, and persistence contracts for storage enclosures.

use std::num::NonZeroU64;

use super::*;

use crate::content::{
    FORM_BOARD, FORM_BULK_CRATE_BODY, FORM_CHEST_BODY, FORM_CHIP, FORM_FOOD,
    FORM_INSULATED_PANTRY_BODY, FORM_LOG, FORM_LUMP, FORM_ROUGH_BOX_BODY, MATERIAL_BERRIES,
    MATERIAL_GRAIN, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_SHAPE_STONE_PROVISIONS_CROCK, PROCESS_SHAPE_WOOD_BOARDS,
    STORAGE_BULK_TIMBER_PROVISIONS_CRATE, STORAGE_CARVED_STONE_PROVISIONS_CROCK,
    STORAGE_INSULATED_TIMBER_PANTRY, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    STORAGE_TIMBER_PROVISIONS_CHEST, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{AggregateMass, Area, Length, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::TickSpan;
use crate::crafting::{
    ManualCraftStartRequest, plan_manual_craft_from_stockpile, validate_start_manual_craft,
};
use crate::energy::calculate_explicit_energy_accounting;
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileStorageError, StorageEnclosureValidationError,
    add_solid_stockpile_for_test, deposit_lot_for_test, validate_mount_stockpile,
};
use crate::logistics::{
    validate_allocate_ground_stockpile, validate_initialize_player_logistics,
    validate_place_ground_stockpile,
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
    let mut state = AppState::new();
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

#[test]
fn located_storage_target_rejects_remote_carried_construction_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_position = VoxelCoord::new(0, 0, 0);
    let carried = validate_initialize_player_logistics(
        &state,
        player_position,
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("remote storage logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote storage logistics commit failed: {error}"))
    .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("remote storage chest body failed: {error}"));
    let target_position = VoxelCoord::new(1, 0, 0);
    let target = validate_allocate_ground_stockpile(
        &state,
        target_position,
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("remote storage target allocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote storage target allocation commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            carried,
        )
        .err(),
        Some(
            StorageEnclosureConstructionError::LocatedTargetSourceRemote {
                target,
                target_position,
                source: carried,
                source_position: player_position,
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn located_storage_target_accepts_carried_material_at_same_voxel() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(3, 0, -2);
    let carried =
        validate_initialize_player_logistics(&state, position, Mass::from_milligrams(5_000_000))
            .unwrap_or_else(|error| panic!("local storage logistics setup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("local storage logistics commit failed: {error}"))
            .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("local storage chest body failed: {error}"));
    let target =
        validate_allocate_ground_stockpile(&state, position, Mass::from_milligrams(5_000_000))
            .unwrap_or_else(|error| panic!("local storage target allocation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("local storage target allocation commit failed: {error}")
            });

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        carried,
    )
    .unwrap_or_else(|error| panic!("same-voxel storage construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("same-voxel storage construction commit failed: {error}"));

    assert_eq!(
        state
            .inventory()
            .get_stockpile(target)
            .and_then(|record| record.enclosure())
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_TIMBER_PROVISIONS_CHEST)
    );
    assert_eq!(
        state.logistics().ground_stockpile_position(target),
        Some(position)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn storage_construction_token_rejects_world_location_change_before_commit() {
    let (registries, mut state, target, source, _) =
        construction_fixture(Mass::from_milligrams(5_000_000));
    let validated = validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("stale-location storage validation failed: {error}"));
    validate_place_ground_stockpile(&state, target, VoxelCoord::new(4, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location target placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location target placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(StorageEnclosureCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert!(
        state
            .inventory()
            .get_stockpile(target)
            .is_some_and(|record| record.enclosure().is_none())
    );
}

#[test]
fn raw_timber_in_carried_custody_becomes_a_placed_field_box_at_player_voxel() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("field-box survival setup failed: {error}"));
    let position = VoxelCoord::new(2, 0, 3);
    let carried =
        validate_initialize_player_logistics(&state, position, Mass::from_milligrams(4_000_000))
            .unwrap_or_else(|error| panic!("field-box logistics setup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("field-box logistics commit failed: {error}"))
            .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(2_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("field-box raw timber fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("field-box matter-before audit failed: {error}"))
        .total();

    let boards = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        carried,
        NonZeroU64::new(2).unwrap_or_else(|| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("field-box board planning failed: {error}"));
    let board_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::in_place(boards),
    )
    .unwrap_or_else(|error| panic!("field-box board start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("field-box board commit failed: {error}"));
    while state.production().get_job(board_job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("field-box board work failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::from_milligrams(1_600_000))
    );

    let body = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
        carried,
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!()),
    )
    .unwrap_or_else(|error| panic!("field-box body planning failed: {error}"));
    let body_job =
        validate_start_manual_craft(&registries, &state, ManualCraftStartRequest::in_place(body))
            .unwrap_or_else(|error| panic!("field-box body start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("field-box body commit failed: {error}"));
    while state.production().get_job(body_job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("field-box body work failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY))),
        Some(Mass::from_milligrams(1_600_000))
    );

    let target =
        validate_allocate_ground_stockpile(&state, position, Mass::from_milligrams(10_000_000))
            .unwrap_or_else(|error| panic!("field-box ground target allocation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("field-box ground target commit failed: {error}"));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        carried,
    )
    .unwrap_or_else(|error| panic!("field-box enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("field-box enclosure commit failed: {error}"));

    let target_record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("field-box target disappeared"));
    assert_eq!(
        target_record
            .enclosure()
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_ROUGH_TIMBER_FIELD_BOX)
    );
    assert_eq!(
        state.logistics().ground_stockpile_position(target),
        Some(position)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))),
        Some(Mass::from_milligrams(400_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("field-box matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_carried_stockpile_cannot_be_upgraded_into_stationary_storage() {
    let registries = build_registries();
    let mut state = AppState::new();
    let target = validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(0, 0, 0),
        Mass::from_milligrams(5_000_000),
    )
    .unwrap_or_else(|error| panic!("carried-storage logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("carried-storage logistics commit failed: {error}"))
    .carried_stockpile();
    let source = add_solid_stockpile_for_test(&mut state, CHEST_MASS)
        .unwrap_or_else(|error| panic!("carried-storage source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("carried-storage chest body failed: {error}"));

    assert_eq!(
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            target,
            source,
        )
        .err(),
        Some(StorageEnclosureConstructionError::PlayerCarriedTarget { stockpile: target })
    );
    assert!(
        state
            .inventory()
            .get_stockpile(target)
            .is_some_and(|record| record.enclosure().is_none())
    );
}

#[test]
fn bulk_crate_encloses_large_reserve_that_standard_chest_cannot() {
    let registries = build_registries();
    let mut state = AppState::new();
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(40_000_000))
        .unwrap_or_else(|error| panic!("bulk crate target fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(30_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("bulk crate grain reserve failed: {error}"));
    let body_mass = Mass::from_milligrams(3_200_000);
    let source = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("bulk crate construction source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("bulk crate body fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("bulk crate matter-before audit failed: {error}"))
        .total();

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
            capacity: Mass::from_milligrams(40_000_000),
            maximum: Mass::from_milligrams(20_000_000),
        })
    );

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("bulk crate construction validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("bulk crate construction commit failed: {error}"));
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("bulk crate target disappeared"));
    assert_eq!(target_record.capacity(), Mass::from_milligrams(40_000_000));
    assert_eq!(
        target_record.stored_mass(),
        Mass::from_milligrams(30_000_000)
    );
    assert_eq!(
        target_record
            .storage_profile()
            .preservation_multiplier_ppm(),
        1_500_000
    );
    assert_eq!(target_record.embodied_mass(), body_mass);
    assert_eq!(
        target_record
            .enclosure()
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_BULK_TIMBER_PROVISIONS_CRATE)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("bulk crate matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn raw_stone_can_be_shaped_into_a_timber_free_preservation_crock() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stone crock survival setup failed: {error}"));
    let raw_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("stone crock raw source failed: {error}"));
    let raw_stone = deposit_lot_for_test(
        &registries,
        &mut state,
        raw_source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("stone crock raw material failed: {error}"));
    let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("stone crock shaped destination failed: {error}"));
    let craft = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
            raw_source,
            MaterialLotSelection::new(raw_stone, Mass::from_milligrams(3_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("stone crock shaping validation failed: {error}"));
    let craft_ticks = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_PROVISIONS_CROCK)
        .map(|definition| definition.duration().value())
        .unwrap_or_else(|| panic!("stone crock shaping definition disappeared"));
    assert_eq!(craft_ticks, 180);
    craft
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone crock shaping commit failed: {error}"));
    advance_exact(&registries, &mut state, craft_ticks);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(3_000_000))
    );

    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("stone crock food target failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("stone crock food fixture failed: {error}"));
    advance_exact(&registries, &mut state, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(100)
    ));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("stone crock matter-before audit failed: {error}"))
        .total();

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_CARVED_STONE_PROVISIONS_CROCK,
        target,
        shaped,
    )
    .unwrap_or_else(|error| panic!("stone crock construction validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stone crock construction commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(600_000)),
        "stone shaping chips must remain represented after the crock body is installed"
    );
    assert_eq!(
        state.inventory().get_stockpile(target).map(|record| (
            record.storage_profile().preservation_multiplier_ppm(),
            record.embodied_mass(),
        )),
        Some((2_500_000, Mass::from_milligrams(2_400_000)))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("stone crock matter-after audit failed: {error}"))
            .total(),
        matter_before
    );

    advance_exact(&registries, &mut state, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(140)
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn insulated_pantry_slows_only_future_food_age_four_to_one() {
    let registries = build_registries();
    let mut state = AppState::new();
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("insulated pantry target fixture failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("insulated pantry food fixture failed: {error}"));
    let body_mass = Mass::from_milligrams(4_800_000);
    let source = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("insulated pantry source fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("insulated pantry body fixture failed: {error}"));

    advance_exact(&registries, &mut state, 80);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(80)
    ));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_INSULATED_TIMBER_PANTRY,
        target,
        source,
    )
    .unwrap_or_else(|error| panic!("insulated pantry construction validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("insulated pantry construction commit failed: {error}"));
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(80)
    ));

    advance_exact(&registries, &mut state, 80);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(100)
    ));
    assert_eq!(
        state.inventory().get_stockpile(target).map(|record| (
            record.storage_profile().preservation_multiplier_ppm(),
            record.embodied_mass(),
        )),
        Some((4_000_000, body_mass))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
    let mut state = AppState::new();
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
    let craft_lot = deposit_lot_for_test(
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
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_BOARDS,
            craft_source,
            MaterialLotSelection::new(craft_lot, Mass::from_milligrams(1_000_000)),
            target,
        ),
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
