//! Respawn across resident occupancy, fixed footing, and custody boundaries.

use super::*;

#[test]
fn respawn_rejects_a_moving_resident_as_its_spawn_and_corpse_floor() {
    use crate::logistics::{PlayerBodyState, player_body_has_fixed_floor, player_body_is_grounded};
    use crate::matter::calculate_matter_accounting;
    use crate::world::BlockStateId;

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x0054_454D_504C_4553),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let post = temple_warden_route_at(&registries, &state, VoxelCoord::new(8, -19, 8)).unwrap()[0];
    let spawn = VoxelCoord::new(post.x(), post.y() + 2, post.z());
    let safe = VoxelCoord::new(spawn.x() + 1, spawn.y(), spawn.z());
    for position in [spawn, safe] {
        for y in position.y()..=position.y() + 2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(safe.x(), safe.y() - 1, safe.z()),
        BLOCK_STATE_STONE,
    );
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    assert!(player_body_is_grounded(&registries, &state));
    assert!(!player_body_has_fixed_floor(&registries, &state));
    let mass = crate::core::quantity::Mass::from_milligrams(100_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(crate::content::MATERIAL_STONE, crate::content::FORM_CHIP),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), safe);
    let cache = outcome.death_cache_position();
    assert_ne!(cache, post);
    assert!(!crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        cache
    ));
    assert!(
        cache == safe
            || (cache.x().abs_diff(post.x()) <= 1
                && cache.y().abs_diff(post.y()) <= 1
                && cache.z().abs_diff(post.z()) <= 1),
        "the cache must settle near the blocked landing or at safe recovery"
    );
    assert!(crate::logistics::body_has_fixed_placement_floor(
        &registries,
        &state,
        PlayerBodyState::centered_on_voxel_floor(cache).unwrap()
    ));
    assert!(player_body_has_fixed_floor(&registries, &state));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .stored_mass(),
        mass
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn respawn_rechecks_structural_clearance_and_avoids_a_completed_panel() {
    let (registries, mut state, route) = occupied_temple();
    relocate_player_for_test(&mut state, route[1]);
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let plan = validate_respawn_player(&registries, &state).unwrap();
    let expected = state.structures().revision();
    materialize_full_footprint_panel_at(&registries, &mut state, route[2]);
    let actual = state.structures().revision();
    let before = state.clone();
    assert_eq!(
        plan.commit(&mut state).err(),
        Some(PlayerRespawnCommitError::StaleStructureRevision { expected, actual })
    );
    assert_eq!(state, before);
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), route[1]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn respawn_falls_back_when_a_warden_now_guards_the_original_spawn() {
    let (registries, mut state, route) = occupied_temple_lit();
    advance_to(&registries, &mut state, 5);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1),
    );
    advance_to(&registries, &mut state, 10);
    assert_eq!(visible_guard(&registries, &state), Some(route[2]));
    let death = VoxelCoord::new(route[2].x() + 3, route[2].y(), route[2].z());
    relocate_player_for_test(&mut state, death);
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), death);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn autonomous_warden_motion_invalidates_pending_respawn_clearance() {
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    let chunk = ChunkCoord::new(0, 0);
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let route = temple_warden_route_at(
        &registries,
        &state,
        VoxelCoord::new(
            chunk.world_min_x().unwrap() + 8,
            -19,
            chunk.world_min_z().unwrap() + 8,
        ),
    )
    .unwrap();
    validate_admit_player(&registries, &state, route[1])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    relocate_player_for_test(&mut state, route[2]);
    advance_to(&registries, &mut state, 5);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[1]
    ));
    let approach = VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1);
    relocate_player_for_test(&mut state, approach);
    advance_to(&registries, &mut state, 10);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[2]
    ));
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let pending = validate_respawn_player(&registries, &state).unwrap();
    let logistics_pending =
        crate::logistics::validate_player_logistics_respawn(&registries, &state).unwrap();
    let prepared_at = state.tick();
    let survival_revision = state.survival().revision();
    let logistics_revision = state.logistics().revision();
    let world_revision = state.world().revision();
    let structure_revision = state.structures().revision();
    let inventory_revision = state.inventory().revision();
    advance_to(&registries, &mut state, 15);
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[1]
    ));
    let actual = state.tick();
    assert_eq!(state.survival().revision(), survival_revision);
    assert_eq!(state.logistics().revision(), logistics_revision);
    assert_eq!(state.world().revision(), world_revision);
    assert_eq!(state.structures().revision(), structure_revision);
    assert_eq!(state.inventory().revision(), inventory_revision);
    let before = state.clone();
    assert_eq!(
        logistics_pending.precheck(&state),
        Err(
            crate::logistics::PlayerLogisticsRespawnCommitError::SimulationTick {
                expected: prepared_at,
                actual,
            }
        )
    );
    assert_eq!(
        pending.commit(&mut state),
        Err(PlayerRespawnCommitError::StaleSimulationTick {
            expected: prepared_at,
            actual,
        })
    );
    assert_eq!(state, before);
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), approach);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
