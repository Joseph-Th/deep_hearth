//! Terrain profiles require actual conserved, collidable rock instead of any non-air block.

use super::*;
use crate::content::build_registries;
use crate::core::quantity::Mass;
use crate::world::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockPhysicalDefinition,
    BlockStateDefinition,
};

#[test]
fn generation_rejects_non_air_ghost_and_massless_full_cube_substrates() {
    let air = BlockDefinition::new(
        BlockDefinitionId::AIR,
        "air",
        BlockStateId::AIR,
        BlockPhysicalDefinition::new(None, Mass::ZERO, BlockCollisionShape::Empty, false),
        None,
    );
    let ghost = BlockStateId::new(90);
    let barrier = BlockStateId::new(91);
    let registry = WorldRegistry::new(
        [
            air,
            BlockDefinition::new(
                BlockDefinitionId::new(90),
                "non-colliding apparent rock",
                ghost,
                BlockPhysicalDefinition::new(None, Mass::ZERO, BlockCollisionShape::Empty, false),
                None,
            ),
            BlockDefinition::new(
                BlockDefinitionId::new(91),
                "massless full-cube barrier",
                barrier,
                BlockPhysicalDefinition::new(
                    None,
                    Mass::ZERO,
                    BlockCollisionShape::FullCube,
                    false,
                ),
                None,
            ),
        ],
        [
            BlockStateDefinition::new(BlockStateId::AIR, BlockDefinitionId::AIR),
            BlockStateDefinition::new(ghost, BlockDefinitionId::new(90)),
            BlockStateDefinition::new(barrier, BlockDefinitionId::new(91)),
        ],
    );
    let state = AppState::new();
    let seed = WorldGenerationSeed::new(7);
    assert_eq!(
        validate_generation_profile(
            &registry,
            &state,
            WorldGenerationProfile::new(seed, BlockStateId::AIR),
        ),
        Err(ChunkGenerationError::AirGenerationSolidState),
    );
    let unknown = BlockStateId::new(92);
    assert_eq!(
        validate_generation_profile(
            &registry,
            &state,
            WorldGenerationProfile::new(seed, unknown)
        ),
        Err(ChunkGenerationError::UnknownSolidState { state: unknown }),
    );
    for invalid in [ghost, barrier] {
        assert_eq!(
            validate_generation_profile(
                &registry,
                &state,
                WorldGenerationProfile::new(seed, invalid)
            ),
            Err(ChunkGenerationError::InvalidTerrainSubstrate { state: invalid }),
        );
        // A decoded world must not bypass the same foundation rule by
        // directly persisting an invalid generation profile and chunk.
        let mut loaded = AppState::new();
        loaded.world_state_mut().insert_generated_chunks(
            0,
            1,
            WorldGenerationProfile::new(seed, invalid),
            vec![WorldChunk::new(ChunkCoord::new(0, 0))],
        );
        assert_eq!(
            crate::world::validate_loaded_world(&registry, loaded.world()),
            Err(crate::world::WorldValidationError::InvalidGenerationSubstrate { state: invalid }),
        );
    }
    assert_eq!(state.world().revision(), 0);
    let valid = build_registries();
    assert_eq!(
        validate_generation_profile(
            valid.world(),
            &state,
            WorldGenerationProfile::new(seed, crate::content::BLOCK_STATE_STONE),
        ),
        Ok(()),
    );
}
