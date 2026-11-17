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

#[test]
fn natural_spawn_offers_a_walkable_level_exit() {
    let registries = build_registries();
    let mut selected = 0;
    for raw_seed in 0..24_u64 {
        let seed = WorldGenerationSeed::new(raw_seed);
        for (cx, cz) in [(0, 0), (3, 2), (-3, -2), (10, -8), (-9, 7)] {
            let chunk = ChunkCoord::new(cx, cz);
            let mut state = AppState::new();
            validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            let selection_hash = raw_seed.wrapping_mul(0xD1B5_4A32_D192_ED03);
            let Some(spawn) =
                select_natural_world_spawn(&registries, &state, chunk, selection_hash, None)
            else {
                continue;
            };
            selected += 1;
            // The client materializes the entire neighboring region immediately
            // after admission. A chunk-edge spawn may leave through that region.
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let neighbor = VoxelCoord::new(spawn.x() + dx, spawn.y(), spawn.z() + dz);
                let adjacent = ChunkCoord::containing(neighbor);
                if state.world().get_chunk(adjacent).is_none() {
                    validate_generate_chunk(&registries, &state, seed, adjacent, BLOCK_STATE_STONE)
                        .unwrap()
                        .commit(&mut state)
                        .unwrap();
                }
            }
            let has_exit = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .map(|(dx, dz)| VoxelCoord::new(spawn.x() + dx, spawn.y(), spawn.z() + dz))
                .any(|neighbor| is_level_walking_exit(&registries, &state, neighbor));
            assert!(
                has_exit,
                "natural start {raw_seed} at {chunk:?} {spawn:?} has no same-level walkable exit"
            );
        }
    }
    assert!(
        selected > 0,
        "sampled wilderness should have usable natural starts"
    );
}

#[test]
fn natural_spawn_exits_through_real_control_motion_without_a_forced_jump() {
    use crate::logistics::{HorizontalControlInput, PlayerControlInput};
    use crate::simulation::advance_control_tick_with_input;
    use crate::world::validate_generate_player_neighborhood;

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0);
    let chunk = ChunkCoord::new(-9, 7);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let spawn = select_natural_world_spawn(&registries, &state, chunk, 0, None)
        .expect("the cliff-edge sample has a supported alternative");
    assert_ne!(
        spawn,
        VoxelCoord::new(-144, 6, 112),
        "the former first valid standing spot lacks a safe walking exit"
    );
    let (destination, dx, dz) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .find_map(|(dx, dz)| {
            let neighbor = VoxelCoord::new(spawn.x() + dx, spawn.y(), spawn.z() + dz);
            (ChunkCoord::containing(neighbor) == chunk
                && is_level_walking_exit(&registries, &state, neighbor))
            .then_some((neighbor, dx, dz))
        })
        .expect("natural admission must supply an adjacent level exit");
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let direction = HorizontalControlInput::new(
        i32::try_from(dx).unwrap() * 1_000_000,
        i32::try_from(dz).unwrap() * 1_000_000,
    )
    .unwrap();
    let input = PlayerControlInput::new(direction);
    for _ in 0..80 {
        let _ = advance_control_tick_with_input(&registries, &mut state, input).unwrap();
        if state.logistics().player().unwrap().position() == destination {
            return;
        }
    }
    panic!("a natural spawn's validated neighboring exit could not be walked to");
}
