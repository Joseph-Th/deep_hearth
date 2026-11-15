//! Spawn searches retain candidate ordering without discarding valid target habitats.

use super::*;
use crate::content::build_registries;
use crate::world::{WorldGenerationSeed, validate_generate_chunk};

#[test]
fn requested_habitat_searches_other_safe_cells_in_the_same_chunk() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4841_4249_5441_5453);
    let mut found = false;
    // Find a real biome boundary, rather than manufacturing an impossible
    // world or making a particular RNG-selected site part of the contract.
    'districts: for cx in -12..=12 {
        for cz in -12..=12 {
            let chunk = ChunkCoord::new(cx, cz);
            let Some(x) = chunk.world_min_x() else {
                continue;
            };
            let Some(z) = chunk.world_min_z() else {
                continue;
            };
            let first = habitat_at(seed, x, z);
            if (0..CHUNK_SIDE).all(|dx| {
                (0..CHUNK_SIDE).all(|dz| habitat_at(seed, x + dx as i64, z + dz as i64) == first)
            }) {
                continue;
            }
            let mut state = AppState::new();
            validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            let Some(default_site) =
                select_natural_world_spawn(&registries, &state, chunk, 0, None)
            else {
                continue;
            };
            let default_habitat = habitat_at(seed, default_site.x(), default_site.z());
            for target in [
                HabitatKind::DampGrove,
                HabitatKind::MixedWoodland,
                HabitatKind::DryScrub,
            ] {
                if target == default_habitat {
                    continue;
                }
                if let Some(other_site) =
                    select_natural_world_spawn(&registries, &state, chunk, 0, Some(target))
                {
                    assert_ne!(other_site, default_site);
                    assert_eq!(habitat_at(seed, other_site.x(), other_site.z()), target);
                    assert!(validate_admit_player(&registries, &state, other_site).is_ok());
                    validate_admit_player(&registries, &state, other_site)
                        .unwrap()
                        .commit(&mut state)
                        .unwrap();
                    assert_eq!(
                        select_natural_world_spawn(
                            &registries,
                            &state,
                            chunk,
                            0,
                            Some(default_habitat)
                        ),
                        None,
                        "world-creation habitat search is unavailable after player admission"
                    );
                    found = true;
                    break 'districts;
                }
            }
        }
    }
    assert!(
        found,
        "mixed terrain must offer a real alternate safe habitat"
    );
}
