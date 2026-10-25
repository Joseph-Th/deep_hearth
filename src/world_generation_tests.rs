//! Contracts for deterministic fresh-world wilderness opening generation.

use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, BLOCK_STATE_STONE,
    FLUID_WATER, FORM_BOARD, FORM_FOOD, FORM_LOG, FORM_LUMP, FORM_ROUGH_BOX_BODY, MATERIAL_BERRIES,
    MATERIAL_CLAY, MATERIAL_STONE, MATERIAL_WILD_NUTS, MATERIAL_WOOD,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::state::{AppState, StateValidationError, apply_clock_advance};
use crate::core::time::{ControlTick, SimulationTick};
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, validate_build_storage_enclosure,
    validate_place_player_storage_at,
};
use crate::logistics::{
    LogisticsValidationError, add_traversable_edge_for_test, validate_allocate_ground_stockpile,
    validate_place_ground_stockpile,
};
use crate::material::{CommodityKey, MaterialComposition};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};
use crate::surface::{
    SurfaceResourceSpec, available_adjacent_surface_resources, validate_create_surface_resources,
};
use crate::survival::{available_adjacent_drink_sources, available_local_drink_sources};
use crate::world::{
    BlockStateId, ChunkCoord, ChunkGenerationCommitError, ChunkGenerationError,
    WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test, validate_generate_chunk,
    validate_generate_player_neighborhood,
};

use super::*;

fn generated_profile(state: &AppState) -> Vec<(CommodityKey, Mass, VoxelCoord)> {
    state
        .surface()
        .resources()
        .map(|record| {
            (
                record.commodity(),
                record.remaining_mass(),
                record.position(),
            )
        })
        .collect()
}

fn plan_full_panel_at(
    registries: &Registries,
    state: &mut AppState,
    head: VoxelCoord,
) -> crate::structural::StructuralElementId {
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(
            head,
            VoxelCoord::new(head.x() + 1, head.y() + 1, head.z() + 1),
        )
        .unwrap(),
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap();
    assert!(geometry.covers_horizontal_footprint());
    add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        false,
    )
    .unwrap()
}

fn materialize_full_panel_at(registries: &Registries, state: &mut AppState, head: VoxelCoord) {
    let member = plan_full_panel_at(registries, state, head);
    materialize_structural_element_for_test(registries, state, member, FORM_BOARD);
}

fn place_completed_field_box_at(
    registries: &Registries,
    state: &mut AppState,
    at: VoxelCoord,
) -> crate::inventory::StockpileId {
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        registries,
        state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        definition.assembly_profile().input_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_player_storage_at(registries, state, STORAGE_ROUGH_TIMBER_FIELD_BOX, at)
        .unwrap()
        .commit(state)
        .unwrap()
}

#[test]
fn terrain_generation_cannot_bury_a_completed_stationary_storage_enclosure() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let enclosure = place_completed_field_box_at(&registries, &mut state, at);
    assert_eq!(
        state.logistics().stationary_stockpile_position(enclosure),
        Some(at)
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::containing(at),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel: at }),
        "generated solid rock cannot occupy the voxel of material-backed stationary storage"
    );
    assert_eq!(state, before);
}

#[test]
fn pending_chunk_generation_rejects_inventory_only_enclosure_construction() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    let target =
        add_solid_stockpile_for_test(&mut state, definition.maximum_stockpile_capacity()).unwrap();
    validate_place_ground_stockpile(&state, target, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
        definition.assembly_profile().input_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let planned = validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    let inventory_before = state.inventory().revision();
    let logistics_before = state.logistics().revision();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        target,
        target,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.logistics().revision(), logistics_before);
    let before = state.clone();
    assert_eq!(
        planned.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleInventoryRevision {
            expected: inventory_before,
            actual: state.inventory().revision(),
        }),
        "material-backed enclosure construction cannot slip past an older generation plan"
    );
    assert_eq!(state, before, "rejected generation must be atomic");
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::containing(at),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel: at })
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn player_chunk_demand_rejects_changed_inventory_before_publishing_any_chunk() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(3, 3),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let planned = validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap();
    let expected = state.inventory().revision();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let before = state.clone();
    assert_eq!(
        planned.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleInventoryRevision {
            expected,
            actual: state.inventory().revision(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn terrain_generation_reports_oldest_local_enclosure_not_spatially_first_or_remote() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let first = VoxelCoord::new(15, WORLD_MIN_Y, 15);
    let later = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let outside = VoxelCoord::new(0, WORLD_MIN_Y, 32);
    let first_id = place_completed_field_box_at(&registries, &mut state, first);
    let later_id = place_completed_field_box_at(&registries, &mut state, later);
    let _remote_id = place_completed_field_box_at(&registries, &mut state, outside);
    assert!(first_id < later_id);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel: first }),
        "chunk-local indexing must keep the oldest obstructing enclosure's identity order"
    );
    assert_eq!(state, before);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &restored,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel: first })
    );
}

#[test]
fn neighborhood_generation_rejects_burying_a_stationary_enclosure_atomically() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(3, 3),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    place_completed_field_box_at(&registries, &mut state, at);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &state).err(),
        Some(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel: at })
    );
    assert_eq!(state, before);
}

#[test]
fn terrain_generation_preserves_completed_enclosure_in_open_air() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, 100, 5);
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(6, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let enclosure = place_completed_field_box_at(&registries, &mut state, at);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.world().block_at(at), Some(BlockStateId::AIR));
    assert_eq!(
        state.logistics().stationary_stockpile_position(enclosure),
        Some(at)
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn trusted_load_rejects_stationary_enclosure_embedded_in_generated_stone() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // Author the enclosure in clear terrain, then forge a saved solid overlap
    // through the test-only world mutation rather than ordinary placement.
    set_generated_block_for_test(registries.world(), &mut state, at, BlockStateId::AIR);
    place_completed_field_box_at(&registries, &mut state, at);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    set_generated_block_for_test(registries.world(), &mut state, at, BLOCK_STATE_STONE);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Err(StateValidationError::Logistics(
            LogisticsValidationError::StationaryEnclosureIntersectsSolidTerrain { voxel: at }
        )),
        "trusted loading cannot accept a completed field box buried in solid generated rock"
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn trusted_load_enclosure_overlap_chooses_chunk_first_then_identity_after_reconstruction() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let mut state = AppState::new();
    let later_chunk = VoxelCoord::new(33, WORLD_MIN_Y, 0);
    let early_chunk_late_id = VoxelCoord::new(1, WORLD_MIN_Y, 0);
    let early_chunk_last_id = VoxelCoord::new(2, WORLD_MIN_Y, 0);
    for chunk in [ChunkCoord::new(0, 0), ChunkCoord::new(2, 0)] {
        validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        crate::world::first_buried_stationary_enclosure_in_world(&registries, &state),
        None,
        "generated terrain without located enclosures cannot contain burial"
    );
    for position in [later_chunk, early_chunk_late_id, early_chunk_last_id] {
        set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
        place_completed_field_box_at(&registries, &mut state, position);
    }
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    for position in [later_chunk, early_chunk_late_id, early_chunk_last_id] {
        set_generated_block_for_test(registries.world(), &mut state, position, BLOCK_STATE_STONE);
    }
    assert_eq!(
        crate::world::first_buried_stationary_enclosure_in_world(&registries, &state),
        Some(early_chunk_late_id),
        "earliest generated chunk wins over an older identity in a later chunk"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Err(StateValidationError::Logistics(
            LogisticsValidationError::StationaryEnclosureIntersectsSolidTerrain {
                voxel: early_chunk_late_id
            }
        ))
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "trusted load must reject the same physically buried enclosure"
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        early_chunk_late_id,
        BlockStateId::AIR,
    );
    assert_eq!(
        crate::world::first_buried_stationary_enclosure_in_world(&registries, &state),
        Some(early_chunk_last_id),
        "a cleared younger conflict reveals the next enclosure"
    );
}

#[test]
fn terrain_generation_cannot_embed_existing_full_footprint_structural_matter() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    materialize_full_panel_at(&registries, &mut state, at);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::containing(at),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::StructureBuriedByTerrain { voxel: at }),
        "the terrain generator must not bury an already embodied full-footprint panel in stone"
    );
    assert_eq!(state, before);
}

#[test]
fn chunk_terrain_collision_checks_only_local_structures_in_stable_id_order() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let mut state = AppState::new();
    let remote = VoxelCoord::new(33, WORLD_MIN_Y, 1);
    let local_first = VoxelCoord::new(2, WORLD_MIN_Y, 1);
    let local_second = VoxelCoord::new(1, WORLD_MIN_Y, 1);
    materialize_full_panel_at(&registries, &mut state, remote);
    materialize_full_panel_at(&registries, &mut state, local_first);
    materialize_full_panel_at(&registries, &mut state, local_second);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let snapshot = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::StructureBuriedByTerrain { voxel: local_first }),
        "the earliest local member ID wins; an older remote member cannot interfere"
    );
    assert_eq!(state, snapshot);
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(2, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::StructureBuriedByTerrain { voxel: remote }),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn trusted_load_rejects_embodied_timber_inside_existing_generated_stone() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    // Forged saves can bypass ordinary construction admission. Build the
    // member in clear generated air, then use the test-only terrain mutation
    // to synthesize the invalid overlap that trusted loading must reject.
    set_generated_block_for_test(registries.world(), &mut state, at, BlockStateId::AIR);
    materialize_full_panel_at(&registries, &mut state, at);
    set_generated_block_for_test(registries.world(), &mut state, at, BLOCK_STATE_STONE);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Err(StateValidationError::StructureEmbeddedInSolidTerrain { voxel: at }),
        "a saved material-backed panel embedded in generated stone must not be trusted"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn empty_structural_blueprint_inside_generated_stone_is_not_physical_matter() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    let member = plan_full_panel_at(&registries, &mut state, at);
    assert!(
        state
            .structures()
            .get_element(member)
            .unwrap()
            .embodied_material()
            .is_empty()
    );
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
}

#[test]
fn neighborhood_generation_rejects_burying_existing_structural_matter_atomically() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let mut state = AppState::new();
    materialize_full_panel_at(&registries, &mut state, at);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(3, 3),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, 100, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &state).err(),
        Some(ChunkGenerationError::StructureBuriedByTerrain { voxel: at })
    );
    assert_eq!(state, before);
}

#[test]
fn terrain_generation_preserves_embodied_construction_above_the_generated_ground() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let at = VoxelCoord::new(5, 100, 5);
    let mut state = AppState::new();
    materialize_full_panel_at(&registries, &mut state, at);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(at),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state.world().block_at(at),
        Some(crate::world::BlockStateId::AIR)
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn terrain_backed_wilderness_generation_selects_only_standable_opening_layout() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let horizontal = VoxelCoord::new(8, 0, 8);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(horizontal),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("terrain-backed wilderness chunk validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("terrain-backed wilderness chunk commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(horizontal.x(), y, horizontal.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap_or_else(|| panic!("terrain-backed wilderness spawn column has no solid surface"));
    let spawn = VoxelCoord::new(horizontal.x(), surface_y + 1, horizontal.z());
    let layout = generated_opening_layout_matching(seed, spawn, |position| {
        state
            .world()
            .has_standing_clearance(registries.world(), position)
    })
    .unwrap_or_else(|| panic!("terrain-backed wilderness fixture has no standable opening layout"));

    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("terrain-backed wilderness validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("terrain-backed wilderness commit failed: {error}"));

    assert!(
        layout.positions().into_iter().all(|position| state
            .world()
            .has_standing_clearance(registries.world(), position)),
        "every authored opening interaction voxel must remain physically standable in generated terrain"
    );
    assert!(
        layout
            .edge_pairs()
            .into_iter()
            .all(|(from, to)| state.logistics().has_traversal_edge(from, to)),
        "terrain-aware opening generation must persist exactly the selected traversable topology"
    );
}

#[test]
fn opening_generation_never_places_residents_inside_embodied_full_footprint_panel() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let horizontal = VoxelCoord::new(8, 0, 8);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(horizontal),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let spawn = VoxelCoord::new(8, surface_y + 1, 8);
    let layout = generated_opening_layout_matching(seed, spawn, |position| {
        state
            .world()
            .has_standing_clearance(registries.world(), position)
    })
    .unwrap();
    let den = layout.material_branch;
    let head = VoxelCoord::new(den.x(), den.y() + 1, den.z());
    materialize_full_panel_at(&registries, &mut state, head);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let before = state.clone();
    match validate_generate_initial_wilderness(&registries, &state, seed, spawn) {
        Ok(plan) => {
            plan.commit(&mut state).unwrap();
            assert!(
                !crate::kobolds::opening_kobold_occupies_voxel(&state, head),
                "an opening resident must not overlap the embodied full-footprint panel"
            );
            assert_eq!(
                crate::core::state::validate_loaded_state(&registries, &state),
                Ok(())
            );
        }
        Err(InitialWildernessGenerationError::NoTraversableTerrainLayout) => {
            assert_eq!(state, before);
        }
        Err(error) => panic!("unexpected opening generation failure: {error}"),
    }
}

#[test]
fn opening_generation_commit_rejects_construction_added_after_clearance_admission() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let horizontal = VoxelCoord::new(8, 0, 8);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(horizontal),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let spawn = VoxelCoord::new(8, surface_y + 1, 8);
    let layout = generated_opening_layout_matching(seed, spawn, |position| {
        state
            .world()
            .has_standing_clearance(registries.world(), position)
    })
    .unwrap();
    let planned = validate_generate_initial_wilderness(&registries, &state, seed, spawn).unwrap();
    let expected = state.structures().revision();
    let den = layout.material_branch;
    materialize_full_panel_at(
        &registries,
        &mut state,
        VoxelCoord::new(den.x(), den.y() + 1, den.z()),
    );
    let actual = state.structures().revision();
    let before = state.clone();
    assert_eq!(
        planned.commit(&mut state),
        Err(InitialWildernessGenerationCommitError::StaleStructureRevision { expected, actual }),
        "wilderness creation must reject a stale structural clearance snapshot"
    );
    assert_eq!(state, before);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn terrain_backed_wilderness_generation_rejects_spawn_without_standable_layout() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let horizontal = VoxelCoord::new(8, 0, 8);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(horizontal),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("terrain rejection chunk validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("terrain rejection chunk commit failed: {error}"));
    let before = state.clone();
    let unsupported_spawn = VoxelCoord::new(horizontal.x(), 100, horizontal.z());

    assert_eq!(
        validate_generate_initial_wilderness(&registries, &state, seed, unsupported_spawn).err(),
        Some(InitialWildernessGenerationError::NoTraversableTerrainLayout)
    );
    assert_eq!(
        state, before,
        "failed terrain-aware opening admission must not mutate world state"
    );
}

fn generate(state: &mut AppState, seed: WorldGenerationSeed, spawn: VoxelCoord) {
    let registries = build_registries();
    validate_generate_initial_wilderness(&registries, state, seed, spawn)
        .unwrap_or_else(|error| panic!("wilderness generation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("wilderness generation commit failed: {error}"));
}

#[test]
fn initial_wilderness_generation_is_deterministic_and_seed_sensitive() {
    let spawn = VoxelCoord::new(12, 3, -8);
    let seed = WorldGenerationSeed::new(0xD33F_0001_5EED_A11C);
    let mut first = AppState::new();
    let mut replay = AppState::new();
    generate(&mut first, seed, spawn);
    generate(&mut replay, seed, spawn);
    assert_eq!(
        first, replay,
        "identical explicit seeds must replay the whole generated opening"
    );
    assert_eq!(generated_profile(&first), generated_profile(&replay));

    let layout = generated_opening_layout(seed, spawn);
    let profile = generated_profile(&first);
    assert_eq!(profile.len(), 5);
    assert_eq!(
        first.logistics().traversable_edges().count(),
        INITIAL_WILDERNESS_ACCESS_EDGES
    );
    assert!(
        layout
            .edge_pairs()
            .into_iter()
            .all(|(from, to)| first.logistics().has_traversal_edge(from, to))
    );
    assert!(
        layout
            .positions()
            .iter()
            .all(|position| position.y() == spawn.y())
    );
    assert!(
        first
            .surface()
            .available_resources_at(Some(spawn))
            .next()
            .is_none()
    );
    let water = first
        .fluid()
        .stores()
        .next()
        .unwrap_or_else(|| panic!("generated wilderness water disappeared"));
    assert_eq!(first.fluid().stores().count(), 1);
    let contents = water
        .contents()
        .unwrap_or_else(|| panic!("generated wilderness water store is empty"));
    assert_eq!(contents.fluid(), FLUID_WATER);
    assert!(
        (Volume::from_microliters(OPENING_WATER_BASE_VOLUME_UL)
            ..=Volume::from_microliters(OPENING_WATER_BASE_VOLUME_UL + OPENING_WATER_VARIATION_UL,))
            .contains(&contents.volume())
    );
    assert_eq!(water.capacity(), contents.volume());
    assert_eq!(contents.temperature(), WILDERNESS_TEMPERATURE);
    let water_position = first
        .logistics()
        .fluid_store_position(water.id())
        .unwrap_or_else(|| panic!("generated wilderness water has no world location"));
    assert_eq!(water_position, layout.water);
    let mass_for = |commodity| {
        profile
            .iter()
            .find(|(found, _, _)| *found == commodity)
            .map(|(_, mass, _)| *mass)
            .unwrap_or_else(|| panic!("generated wilderness commodity {commodity:?} disappeared"))
    };
    assert!(
        (Mass::from_milligrams(STONE_BASE_MASS_MG)
            ..=Mass::from_milligrams(STONE_BASE_MASS_MG + STONE_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_STONE, FORM_LUMP)))
    );
    assert!(
        (Mass::from_milligrams(CLAY_BASE_MASS_MG)
            ..=Mass::from_milligrams(CLAY_BASE_MASS_MG + CLAY_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)))
    );
    assert!(
        (Mass::from_milligrams(TIMBER_BASE_MASS_MG)
            ..=Mass::from_milligrams(TIMBER_BASE_MASS_MG + TIMBER_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)))
    );
    assert!(
        (Mass::from_milligrams(BERRIES_BASE_MASS_MG)
            ..=Mass::from_milligrams(BERRIES_BASE_MASS_MG + BERRIES_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)))
    );
    assert!(
        (Mass::from_milligrams(WILD_NUTS_BASE_MASS_MG)
            ..=Mass::from_milligrams(WILD_NUTS_BASE_MASS_MG + WILD_NUTS_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD)))
    );
    let position_for = |commodity| {
        profile
            .iter()
            .find(|(found, _, _)| *found == commodity)
            .map(|(_, _, position)| *position)
            .unwrap_or_else(|| panic!("generated wilderness commodity {commodity:?} disappeared"))
    };
    assert_eq!(
        position_for(CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)),
        layout.forage
    );
    assert_eq!(
        position_for(CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD)),
        layout.junction,
        "the optional protein supply should be discoverable at the worksite without a detour"
    );
    assert_eq!(
        position_for(CommodityKey::new(MATERIAL_CLAY, FORM_LUMP)),
        layout.clay,
        "clay-rich earth is one separate opening leg beyond the water source"
    );
    let stone_position = position_for(CommodityKey::new(MATERIAL_STONE, FORM_LUMP));
    let timber_position = position_for(CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    assert!(
        (stone_position == layout.material_forward && timber_position == layout.material_branch)
            || (stone_position == layout.material_branch
                && timber_position == layout.material_forward)
    );

    let mut alternate = AppState::new();
    generate(
        &mut alternate,
        WorldGenerationSeed::new(seed.value() ^ 1),
        spawn,
    );
    assert_ne!(
        generated_profile(&first),
        generated_profile(&alternate),
        "different explicit world seeds should change at least one opening opportunity"
    );
}

#[test]
fn opening_food_and_water_opportunities_vary_and_exceed_one_resting_day() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let resting_day_energy = physiology
        .basal_energy_cost_per_tick()
        .nanojoules()
        .checked_mul(u128::from(ticks_per_day))
        .unwrap_or_else(|| panic!("opening resting-day energy projection overflowed"));
    let resting_day_hydration = physiology
        .hydration_loss_per_tick()
        .microliters()
        .checked_mul(ticks_per_day)
        .unwrap_or_else(|| panic!("opening resting-day hydration projection overflowed"));
    let berries = registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD))
        .unwrap_or_else(|| panic!("opening berries lost their food definition"));
    let minimum_food_energy = berries
        .dietary_energy_for_mass(Mass::from_milligrams(BERRIES_BASE_MASS_MG))
        .nanojoules();

    assert!(
        minimum_food_energy > resting_day_energy,
        "the minimum opening forage opportunity should buy more than one resting day instead of forcing repeated subsistence within hours"
    );
    assert!(
        OPENING_WATER_BASE_VOLUME_UL > resting_day_hydration,
        "the minimum opening water opportunity should buy more than one resting day of basal hydration"
    );

    let spawn = VoxelCoord::new(0, 0, 0);
    let opportunities = (0_u64..8)
        .map(|seed| {
            let mut state = AppState::new();
            generate(&mut state, WorldGenerationSeed::new(seed), spawn);
            let berries = state
                .surface()
                .resources()
                .find(|resource| {
                    resource.commodity() == CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
                })
                .map(|resource| resource.remaining_mass().milligrams())
                .unwrap_or_else(|| panic!("varied opening lost generated forage"));
            let water = state
                .fluid()
                .stores()
                .next()
                .and_then(|store| store.contents())
                .map(|contents| contents.volume().microliters())
                .unwrap_or_else(|| panic!("varied opening lost generated water"));
            (berries, water)
        })
        .collect::<Vec<_>>();
    let first = opportunities[0];
    assert!(
        opportunities.iter().any(|(berries, _)| *berries != first.0),
        "opening forage quantity should vary across explicit world seeds"
    );
    assert!(
        opportunities.iter().any(|(_, water)| *water != first.1),
        "opening water quantity should vary across explicit world seeds"
    );
}

#[test]
fn initial_wilderness_generation_handles_coordinate_extremes_without_overflow() {
    let seed = WorldGenerationSeed::new(0xDEAD_BEEF);
    for spawn in [
        VoxelCoord::new(i64::MIN, 7, i64::MIN),
        VoxelCoord::new(i64::MAX, -7, i64::MAX),
    ] {
        let mut state = AppState::new();
        generate(&mut state, seed, spawn);
        let layout = generated_opening_layout(seed, spawn);
        assert_eq!(layout.spawn, spawn);
        assert!(
            layout
                .edge_pairs()
                .into_iter()
                .all(|(from, to)| from.is_face_adjacent_to(to))
        );
        assert!(
            layout
                .positions()
                .iter()
                .all(|position| position.y() == spawn.y())
        );
        assert_eq!(
            state.logistics().traversable_edges().count(),
            INITIAL_WILDERNESS_ACCESS_EDGES
        );
    }
}

#[test]
fn initial_wilderness_generation_requires_fresh_pre_admission_world_state() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(7);

    let mut generated = AppState::new();
    generate(&mut generated, seed, spawn);
    let before_second = generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::SurfaceAlreadyGenerated)
    );
    assert_eq!(generated, before_second);

    let mut fluid_generated = AppState::new();
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut fluid_generated,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        WILDERNESS_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("preexisting fluid generation fixture failed: {error}"));
    let before_fluid = fluid_generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &fluid_generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::FluidAlreadyGenerated)
    );
    assert_eq!(fluid_generated, before_fluid);

    let mut access_generated = AppState::new();
    add_traversable_edge_for_test(&mut access_generated, spawn, VoxelCoord::new(1, 0, 0));
    let before_access = access_generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &access_generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::LogisticsAlreadyPopulated)
    );
    assert_eq!(access_generated, before_access);

    let mut located_stockpile = AppState::new();
    validate_allocate_ground_stockpile(
        &located_stockpile,
        VoxelCoord::new(8, 0, 0),
        Mass::from_milligrams(1),
    )
    .unwrap_or_else(|error| panic!("preexisting logistics allocation failed: {error}"))
    .commit(&mut located_stockpile)
    .unwrap_or_else(|error| panic!("preexisting logistics allocation commit failed: {error}"));
    let before_located_stockpile = located_stockpile.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &located_stockpile, seed, spawn).err(),
        Some(InitialWildernessGenerationError::LogisticsAlreadyPopulated),
        "fresh-world generation must reject preexisting located custody, not only traversal edges"
    );
    assert_eq!(located_stockpile, before_located_stockpile);

    let mut admitted = AppState::new();
    validate_admit_player(&registries, &admitted, spawn)
        .unwrap_or_else(|error| panic!("world-generation admission setup failed: {error}"))
        .commit(&mut admitted)
        .unwrap_or_else(|error| panic!("world-generation admission commit failed: {error}"));
    let before_admitted = admitted.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &admitted, seed, spawn).err(),
        Some(InitialWildernessGenerationError::PlayerAlreadyInitialized {
            survival_initialized: true,
            logistics_initialized: true,
        })
    );
    assert_eq!(admitted, before_admitted);

    let mut aged = AppState::new();
    apply_clock_advance(&mut aged, SimulationTick::new(1));
    let before_aged = aged.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &aged, seed, spawn).err(),
        Some(InitialWildernessGenerationError::NotInitialTick {
            tick: SimulationTick::new(1),
        })
    );
    assert_eq!(aged, before_aged);
}

#[test]
fn initial_wilderness_generation_requires_exact_initial_control_tick() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(73);
    let mut state = AppState::new();
    let validated = validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("initial-control generation validation failed: {error}"));

    let outcome = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("initial-control advance failed: {error}"));
    assert_eq!(outcome.control_tick(), ControlTick::new(1));
    assert!(outcome.simulation().is_none());
    assert_eq!(
        validated.commit(&mut state),
        Err(InitialWildernessGenerationCommitError::ControlTickChanged {
            expected: ControlTick::ZERO,
            actual: ControlTick::new(1),
        })
    );
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &state, seed, spawn).err(),
        Some(InitialWildernessGenerationError::NotInitialControlTick {
            control_tick: ControlTick::new(1),
        })
    );
}

#[test]
fn initial_wilderness_generation_rejects_stale_fluid_commit_without_surface_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(12),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale fluid generation validation failed: {error}"));

    let mut state = AppState::new();
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        WILDERNESS_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("competing fluid generation failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validated.commit(&mut state),
        Err(InitialWildernessGenerationCommitError::StaleFluidRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert!(state.surface().resources().next().is_none());
}

#[test]
fn initial_wilderness_generation_rejects_stale_logistics_commit_without_owner_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(14),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale logistics generation validation failed: {error}"));

    let mut state = AppState::new();
    validate_allocate_ground_stockpile(&state, VoxelCoord::new(8, 0, 0), Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("competing logistics allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("competing logistics allocation commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validated.commit(&mut state),
        Err(
            InitialWildernessGenerationCommitError::StaleLogisticsRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
    assert!(state.surface().resources().next().is_none());
    assert!(state.fluid().stores().next().is_none());
}

#[test]
fn initial_wilderness_generation_rejects_stale_surface_commit_without_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(11),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale world generation validation failed: {error}"));

    let mut state = AppState::new();
    let competing = SurfaceResourceSpec::new(
        crate::surface::SurfaceResourceKind::Opportunity,
        spawn,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("competing surface spec failed: {error}"));
    validate_create_surface_resources(&registries, &state, [competing])
        .unwrap_or_else(|error| panic!("competing surface generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("competing surface generation commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(
            InitialWildernessGenerationCommitError::StaleSurfaceRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn initial_wilderness_generation_rejects_admission_or_time_change_before_commit() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(13);

    let mut admitted = AppState::new();
    let validated = validate_generate_initial_wilderness(&registries, &admitted, seed, spawn)
        .unwrap_or_else(|error| panic!("stale admission generation validation failed: {error}"));
    validate_admit_player(&registries, &admitted, spawn)
        .unwrap_or_else(|error| panic!("stale admission player validation failed: {error}"))
        .commit(&mut admitted)
        .unwrap_or_else(|error| panic!("stale admission player commit failed: {error}"));
    let before_admitted_commit = admitted.clone();
    assert_eq!(
        validated.commit(&mut admitted),
        Err(InitialWildernessGenerationCommitError::PlayerInitialized {
            survival_initialized: true,
            logistics_initialized: true,
        })
    );
    assert_eq!(admitted, before_admitted_commit);

    let mut aged = AppState::new();
    let validated = validate_generate_initial_wilderness(&registries, &aged, seed, spawn)
        .unwrap_or_else(|error| panic!("stale tick generation validation failed: {error}"));
    apply_clock_advance(&mut aged, SimulationTick::new(1));
    let before_aged_commit = aged.clone();
    assert_eq!(
        validated.commit(&mut aged),
        Err(InitialWildernessGenerationCommitError::TickChanged {
            expected: SimulationTick::ZERO,
            actual: SimulationTick::new(1),
        })
    );
    assert_eq!(aged, before_aged_commit);
}

#[test]
fn generated_wilderness_persists_with_distributed_opening_and_access_after_load() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(-3, 1, 5);
    let seed = WorldGenerationSeed::new(0xCAFE_BABE);
    let mut state = AppState::new();
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("persistent wilderness generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("persistent wilderness generation commit failed: {error}"));
    validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("persistent wilderness admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("persistent wilderness admission commit failed: {error}"));
    let layout = generated_opening_layout(seed, spawn);
    let visible_before = state
        .available_surface_resources()
        .map(|record| (record.commodity(), record.remaining_mass()))
        .collect::<Vec<_>>();
    assert!(visible_before.is_empty());
    let adjacent_surface_before = available_adjacent_surface_resources(&state)
        .map(|resource| {
            (
                resource.resource(),
                resource.commodity(),
                resource.position(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(adjacent_surface_before.len(), 1);
    assert_eq!(
        adjacent_surface_before[0].1,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
    );
    assert_eq!(adjacent_surface_before[0].2, layout.forage);
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        0
    );
    let adjacent_before = available_adjacent_drink_sources(&registries, &state)
        .map(|source| (source.store(), source.fluid(), source.position()))
        .collect::<Vec<_>>();
    assert!(adjacent_before.is_empty());
    let destinations_before = state
        .available_adjacent_traversal_destinations()
        .collect::<Vec<_>>();
    assert_eq!(destinations_before, vec![layout.forage]);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("wilderness generation save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("wilderness generation decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("wilderness generation load failed: {error}"));
    let visible_after = loaded
        .available_surface_resources()
        .map(|record| (record.commodity(), record.remaining_mass()))
        .collect::<Vec<_>>();
    assert_eq!(visible_after, visible_before);
    assert_eq!(
        available_adjacent_surface_resources(&loaded)
            .map(|resource| (
                resource.resource(),
                resource.commodity(),
                resource.position()
            ))
            .collect::<Vec<_>>(),
        adjacent_surface_before
    );
    let adjacent_after = available_adjacent_drink_sources(&registries, &loaded)
        .map(|source| (source.store(), source.fluid(), source.position()))
        .collect::<Vec<_>>();
    assert_eq!(adjacent_after, adjacent_before);
    assert_eq!(
        loaded
            .available_adjacent_traversal_destinations()
            .collect::<Vec<_>>(),
        destinations_before
    );
    assert_eq!(
        available_local_drink_sources(&registries, &loaded).count(),
        0
    );
}
