//! Bootstrap location admission must not create matter/terrain intersections.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::world::{ChunkCoord, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk};

fn generated_rock() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4752_4F55_4E44_504C),
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.world().block_at(at), Some(BLOCK_STATE_STONE));
    (registries, state, at)
}

fn generated_tree_trunk() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let trunk = state.world().trees().next().unwrap().root();
    assert_eq!(
        state.world().block_at(trunk),
        Some(crate::world::BlockStateId::AIR)
    );
    assert!(state.world().tree_trunk_at(trunk).is_some());
    (registries, state, trunk)
}

fn timber_stockpile(registries: &crate::registry::Registries, state: &mut AppState) -> StockpileId {
    let stockpile = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000)).unwrap();
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    stockpile
}

#[test]
fn placing_preexisting_loose_matter_inside_generated_stone_is_rejected_atomically() {
    let (registries, mut state, at) = generated_rock();
    let stockpile = timber_stockpile(&registries, &mut state);
    let before = state.clone();
    assert_eq!(
        validate_place_ground_stockpile(&state, stockpile, at).err(),
        Some(GroundStockpilePlacementError::SolidTerrainOccupied { position: at })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_prevalidated_empty_location_must_not_bury_later_added_matter() {
    let (registries, mut state, at) = generated_rock();
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let admitted = validate_place_ground_stockpile(&state, stockpile, at).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let before = state.clone();
    assert_eq!(
        admitted.commit(&mut state),
        Err(GroundStockpilePlacementCommitError::SolidTerrainOccupied { position: at })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn material_ground_placement_succeeds_in_open_generated_air_and_survives_save_load() {
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    let (registries, mut state, rock) = generated_rock();
    let at = VoxelCoord::new(rock.x(), 100, rock.z());
    assert_eq!(
        state.world().block_at(at),
        Some(crate::world::BlockStateId::AIR)
    );
    let stockpile = timber_stockpile(&registries, &mut state);
    validate_place_ground_stockpile(&state, stockpile, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}

#[test]
fn validated_ground_placement_rechecks_new_terrain_before_commit() {
    use crate::world::set_generated_block_for_test;
    let (registries, mut state, rock) = generated_rock();
    let at = VoxelCoord::new(rock.x(), 100, rock.z());
    let stockpile = timber_stockpile(&registries, &mut state);
    let admitted = validate_place_ground_stockpile(&state, stockpile, at).unwrap();
    set_generated_block_for_test(registries.world(), &mut state, at, BLOCK_STATE_STONE);
    let before = state.clone();
    assert_eq!(
        admitted.commit(&mut state),
        Err(GroundStockpilePlacementCommitError::SolidTerrainOccupied { position: at }),
    );
    assert_eq!(state, before);
}

#[test]
fn empty_built_enclosure_is_still_matter_and_cannot_be_placed_inside_stone() {
    use crate::content::STORAGE_ROUGH_TIMBER_FIELD_BOX;
    use crate::inventory::validate_build_storage_enclosure;

    let (registries, mut state, at) = generated_rock();
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    let target =
        add_solid_stockpile_for_test(&mut state, definition.maximum_stockpile_capacity()).unwrap();
    let source =
        add_solid_stockpile_for_test(&mut state, definition.maximum_stockpile_capacity()).unwrap();
    for input in definition.assembly_profile().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        source,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let record = state.inventory().get_stockpile(target).unwrap();
    assert!(record.stored_mass().is_zero());
    assert!(!record.embodied_mass().is_zero());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_place_ground_stockpile(&state, target, at).err(),
        Some(GroundStockpilePlacementError::SolidTerrainOccupied { position: at })
    );
    assert_eq!(state, before);
}

#[test]
fn stocked_ground_placement_cannot_intersect_a_living_tree_trunk() {
    let (registries, mut state, trunk) = generated_tree_trunk();
    let stockpile = timber_stockpile(&registries, &mut state);
    let before = state.clone();
    assert_eq!(
        validate_place_ground_stockpile(&state, stockpile, trunk).err(),
        Some(GroundStockpilePlacementError::StandingTreeOccupied { position: trunk })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_cannot_accept_ground_timber_inside_a_living_tree_trunk() {
    let (registries, mut state, trunk) = generated_tree_trunk();
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    // Deliberately bypass the placement validator to test saved-state
    // defense. The world owner rejects even an empty location marker here.
    let before_revision = state.logistics().revision();
    state.logistics_state_mut().apply_ground_placement(
        before_revision,
        before_revision + 1,
        stockpile,
        trunk,
    );
    let invalid_tree = Err(crate::core::state::StateValidationError::World(
        crate::world::WorldValidationError::InvalidStandingTree { root: trunk },
    ));
    assert_eq!(validate_loaded_state(&registries, &state), invalid_tree);
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), invalid_tree);
    let serialized =
        serde_json::to_vec(&crate::persistence::SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<crate::persistence::LoadedSaveEnvelope>(&serialized)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "trusted save admission must reject stationary custody intersecting a living tree"
    );
}

#[test]
fn empty_ground_marker_cannot_displace_a_living_tree_trunk() {
    let (registries, mut state, trunk) = generated_tree_trunk();
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    let before = state.clone();
    assert_eq!(
        validate_place_ground_stockpile(&state, stockpile, trunk).err(),
        Some(GroundStockpilePlacementError::StandingTreeOccupied { position: trunk })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn purely_empty_ground_location_remains_nonphysical_in_existing_rock() {
    let (registries, mut state, at) = generated_rock();
    let stockpile =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000)).unwrap();
    validate_place_ground_stockpile(&state, stockpile, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(state.world().block_at(at), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(at)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
