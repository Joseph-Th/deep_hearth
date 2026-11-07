//! Terrain admission must not instantiate rock through stationary tool matter.

use crate::content::{
    BLOCK_STATE_STONE, EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::equipment::{EquipmentId, validate_assemble_equipment};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{LogisticsValidationError, place_equipment_for_fixture};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MIN_Y, set_generated_block_for_test, validate_generate_chunk,
};

use super::{ChunkGenerationError, WorldGenerationSeed};

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x4551_5549_504D_454E);

/// Build a real stone pick from an inventory, then locate the finished matter
/// before player admission. No abstract equipment ID implies free creation.
fn material_built_pick_at(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
) -> EquipmentId {
    let stockpile = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000)).unwrap();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            stockpile,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let pick = validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, stockpile)
        .unwrap()
        .commit(state)
        .unwrap();
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap()
            .embodied_mass(),
        Mass::from_milligrams(1_000_000)
    );
    place_equipment_for_fixture(state, pick, position);
    pick
}

#[test]
fn generation_cannot_bury_stationary_material_built_equipment() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    let pick = material_built_pick_at(&registries, &mut state, position);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::containing(position),
            BLOCK_STATE_STONE,
        )
        .is_err(),
        "material-built stationary equipment cannot be buried during generation"
    );
    assert_eq!(state, before);
    assert_eq!(
        state.logistics().stationary_equipment_position(pick),
        Some(position)
    );
}

#[test]
fn loaded_state_rejects_material_equipment_intersecting_generated_stone() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::containing(position),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    set_generated_block_for_test(registries.world(), &mut state, position, BlockStateId::AIR);
    material_built_pick_at(&registries, &mut state, position);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );

    // Test-only corruption proves the trusted-load boundary does not silently
    // legitimize an embodied pick which intersects rock.
    set_generated_block_for_test(registries.world(), &mut state, position, BLOCK_STATE_STONE);
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Logistics(
            LogisticsValidationError::LocatedEquipmentIntersectsSolidTerrain { voxel: position }
        ))
    );
    let corrupt = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&corrupt)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn chunk_equipment_collision_preserves_identity_priority_and_excludes_remote_items() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = VoxelCoord::new(15, WORLD_MIN_Y, 15);
    let later = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let remote = VoxelCoord::new(0, WORLD_MIN_Y, 32);
    for position in [first, later, remote] {
        material_built_pick_at(&registries, &mut state, position);
    }
    assert_eq!(
        state
            .logistics()
            .equipment_in_chunk(ChunkCoord::new(0, 0))
            .count(),
        2
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::EquipmentBuriedByTerrain { voxel: first })
    );
    assert_eq!(state, before);
}
