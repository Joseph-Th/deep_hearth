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

#[derive(Clone, Copy)]
enum StationaryAsset {
    Pick,
    Campfire,
}

fn assemble_at_player(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    asset: StationaryAsset,
) {
    let carried = crate::player::validate_admit_player(registries, state, position)
        .unwrap()
        .commit(state)
        .unwrap()
        .carried_stockpile();
    let materials = match asset {
        StationaryAsset::Pick => [
            (CommodityKey::new(MATERIAL_STONE, FORM_TOOL), 800_000),
            (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 200_000),
        ],
        StationaryAsset::Campfire => [
            (
                CommodityKey::new(MATERIAL_STONE, crate::content::FORM_LUMP),
                1_000_000,
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, crate::content::FORM_LOG),
                1_000_000,
            ),
        ],
    };
    for (commodity, milligrams) in materials {
        deposit_lot_for_test(
            registries,
            state,
            carried,
            commodity,
            Mass::from_milligrams(milligrams),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    let conserved = crate::matter::calculate_matter_accounting(state)
        .unwrap()
        .total();
    match asset {
        StationaryAsset::Pick => {
            let pick =
                validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, carried)
                    .unwrap()
                    .commit(state)
                    .unwrap();
            assert_eq!(
                state.logistics().stationary_equipment_position(pick),
                Some(position)
            );
        }
        StationaryAsset::Campfire => {
            let fire = crate::energy::validate_assemble_energy_store(
                registries,
                state,
                crate::content::ENERGY_STONE_RING_CAMPFIRE,
                carried,
            )
            .unwrap()
            .commit(state)
            .unwrap();
            assert_eq!(
                state.logistics().energy_store_position(fire),
                Some(position)
            );
        }
    }
    assert_eq!(
        crate::matter::calculate_matter_accounting(state)
            .unwrap()
            .total(),
        conserved
    );
}

#[test]
fn stationary_asset_footing_generation_rejects_unsupported_material_assembly() {
    for asset in [StationaryAsset::Pick, StationaryAsset::Campfire] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(5, 100, 5);
        // Give neighborhood demand an established generation profile, without
        // introducing terrain authority at the abstract assembly location.
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(3, 3),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        assemble_at_player(&registries, &mut state, position, asset);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let before = state.clone();
        let expected = match asset {
            StationaryAsset::Pick => {
                ChunkGenerationError::EquipmentRequiresFixedFloor { voxel: position }
            }
            StationaryAsset::Campfire => {
                ChunkGenerationError::EnergyStoreRequiresFixedFloor { voxel: position }
            }
        };
        assert_eq!(
            validate_generate_chunk(
                &registries,
                &state,
                SEED,
                ChunkCoord::containing(position),
                BLOCK_STATE_STONE,
            )
            .err(),
            Some(expected.clone()),
            "terrain admission must not leave an assembled tool or machine suspended"
        );
        assert_eq!(
            crate::world::validate_generate_player_neighborhood(&registries, &state).err(),
            Some(expected)
        );
        assert_eq!(state, before);
    }
}

#[test]
fn stationary_asset_footing_trusted_load_rejects_unsupported_material_assembly() {
    for asset in [StationaryAsset::Pick, StationaryAsset::Campfire] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(5, 100, 5);
        let floor = VoxelCoord::new(5, 99, 5);
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
        set_generated_block_for_test(registries.world(), &mut state, floor, BLOCK_STATE_STONE);
        assemble_at_player(&registries, &mut state, position, asset);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let valid = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        assert_eq!(
            serde_json::from_slice::<LoadedSaveEnvelope>(&valid)
                .unwrap()
                .into_state(&registries),
            Ok(state.clone())
        );
        set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
        let expected = StateValidationError::Logistics(match asset {
            StationaryAsset::Pick => LogisticsValidationError::LocatedEquipmentRequiresFixedFloor {
                equipment: state.logistics().equipment_locations().next().unwrap().0,
                voxel: position,
            },
            StationaryAsset::Campfire => {
                LogisticsValidationError::LocatedEnergyStoreRequiresFixedFloor {
                    store: state.logistics().energy_store_locations().next().unwrap().0,
                    voxel: position,
                }
            }
        });
        assert_eq!(
            validate_loaded_state(&registries, &state),
            Err(expected.clone())
        );
        let invalid = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        assert_eq!(
            serde_json::from_slice::<LoadedSaveEnvelope>(&invalid)
                .unwrap()
                .into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(expected))
        );
    }
}

#[test]
fn stationary_asset_footing_generation_preserves_supported_assembly() {
    for asset in [StationaryAsset::Pick, StationaryAsset::Campfire] {
        let registries = build_registries();
        let mut state = AppState::new();
        let position = VoxelCoord::new(0, super::terrain_height(SEED, 0, 0) + 1, 0);
        assemble_at_player(&registries, &mut state, position, asset);
        let before = state.clone();
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
        assert_eq!(state.equipment(), before.equipment());
        assert_eq!(state.energy(), before.energy());
        assert_eq!(state.inventory(), before.inventory());
        assert_eq!(state.logistics(), before.logistics());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        assert_eq!(
            serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
                .unwrap()
                .into_state(&registries),
            Ok(state)
        );
    }
}

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
    let position = VoxelCoord::new(5, WORLD_MIN_Y + 1, 5);
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
