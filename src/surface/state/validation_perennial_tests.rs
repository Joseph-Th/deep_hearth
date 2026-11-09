//! Seed-plausible forage must not become authoritative before its terrain exists.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
    validate_generate_player_neighborhood,
};

#[test]
fn seed_plausible_forage_cannot_load_from_an_ungenerated_chunk() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x46_4F_52_41_47_45_57);
    let mut partial = AppState::new();
    validate_generate_chunk(
        &registries,
        &partial,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut partial)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| partial.world().block_at(VoxelCoord::new(0, y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    validate_admit_player(&registries, &partial, VoxelCoord::new(0, floor + 1, 0))
        .unwrap()
        .commit(&mut partial)
        .unwrap();

    // Only one region is generated in the first branch. The second branch
    // acquires authentic, seed-verified forage through ordinary neighborhood
    // generation, so this test cannot rely on a fabricated resource identity.
    let mut generated = partial.clone();
    validate_generate_player_neighborhood(&registries, &generated)
        .unwrap()
        .unwrap()
        .commit(&mut generated)
        .unwrap();
    let stray = generated
        .surface()
        .perennial_resources()
        .filter(|resource| {
            !partial
                .world()
                .has_generated_terrain_at(resource.position())
        })
        .min_by_key(|resource| resource.id())
        .expect("the expanded neighborhood must contain a seeded plant in another chunk");
    let id = stray.id();
    let position = stray.position();
    let profile = generated.world().generation_profile().unwrap();
    assert!(crate::world::seeded_perennial_matches(
        profile,
        position,
        stray.commodity(),
        stray.perennial.unwrap().original_fruit,
    ));
    assert_eq!(validate_loaded_state(&registries, &generated), Ok(()));

    // Moving only the surface owner into the incomplete world simulates a
    // forged save with otherwise authentic root, age, food identity and mass.
    // A seed match alone cannot establish that this plant was ever generated.
    *partial.surface_state_mut() = generated.surface().clone();
    let expected =
        StateValidationError::Surface(SurfaceResourceValidationError::PerennialSiteNotGenerated {
            resource: id,
            position,
        });
    assert_eq!(
        validate_loaded_state(&registries, &partial),
        Err(expected.clone())
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &partial)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries),
        Err(LoadError::InvalidState(expected)),
    );

    // The same physical and biological records remain legal when their
    // originating terrain really was generated.
    let authentic = serde_json::to_vec(&SaveEnvelope::new(&registries, &generated)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&authentic)
            .unwrap()
            .into_state(&registries),
        Ok(generated),
    );
}
