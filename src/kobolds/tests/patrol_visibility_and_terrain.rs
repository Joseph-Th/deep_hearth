//! Scout contact, visibility, and terrain obstruction contracts.

use super::*;

#[test]
fn grounded_player_fast_forward_matches_neutral_controls_across_scout_steps_and_load() {
    let (registries, mut fast, spawn) = generated_opening();
    advance_to_tick(&registries, &mut fast, 179);
    validate_admit_player(&registries, &fast, spawn)
        .unwrap()
        .commit(&mut fast)
        .unwrap();
    let mut direct = fast.clone();
    let conserved = calculate_matter_accounting(&fast).unwrap().total();

    for step in 0..21 {
        let fast_result = advance_tick(&registries, &mut fast).unwrap();
        let direct_result = loop {
            let control = advance_control_tick(&registries, &mut direct).unwrap();
            assert_eq!(
                control.surface_drop_pickup(),
                None,
                "safe neutral skip unexpectedly hid an automatic pickup"
            );
            if let Some(simulation) = control.simulation() {
                break simulation.clone();
            }
        };
        assert_eq!(
            fast_result, direct_result,
            "slow outcome diverged at step {step}"
        );
        assert_eq!(fast, direct, "runtime diverged at step {step}");
        if step == 9 {
            let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &fast)).unwrap();
            fast = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
                .unwrap()
                .into_state(&registries)
                .unwrap();
            assert_eq!(fast, direct);
        }
    }
    assert_eq!(
        calculate_matter_accounting(&fast).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &fast),
        Ok(())
    );

    // A newly available exact-local parcel changes the control contract:
    // fast-forward must now reject without modifying any owner, since a
    // neutral control step will collect it before the next slow boundary.
    let resource = place_loose_stone_drop(&registries, &mut fast, spawn);
    assert_eq!(
        resource,
        place_loose_stone_drop(&registries, &mut direct, spawn)
    );
    let matter_with_drop = calculate_matter_accounting(&fast).unwrap().total();
    let before_rejected_skip = fast.clone();
    assert!(matches!(
        advance_tick(&registries, &mut fast),
        Err(crate::simulation::TickError::ControlSteppingRequired { .. })
    ));
    assert_eq!(fast, before_rejected_skip);
    let control = advance_control_tick(&registries, &mut fast).unwrap();
    assert_eq!(
        control,
        advance_control_tick(&registries, &mut direct).unwrap()
    );
    assert_eq!(
        control
            .surface_drop_pickup()
            .map(|picked| picked.resource()),
        Some(resource)
    );
    assert_eq!(fast, direct);
    assert_eq!(
        calculate_matter_accounting(&fast).unwrap().total(),
        matter_with_drop
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &fast),
        Ok(())
    );

    // The pickup has left us one control tick past the previous slow boundary.
    // Fast-forward may skip only the remaining neutral controls, not a full 72
    // extra steps or a duplicate slow settlement.
    let next_slow = advance_tick(&registries, &mut fast).unwrap();
    let replayed_slow = loop {
        let outcome = advance_control_tick(&registries, &mut direct).unwrap();
        if let Some(slow) = outcome.simulation() {
            break slow.clone();
        }
    };
    assert_eq!(next_slow, replayed_slow);
    assert_eq!(fast, direct);
}

#[test]
fn approaching_a_loaded_scout_forces_it_to_abandon_recoverable_material() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);

    validate_admit_player(&registries, &state, tip)
        .expect("player can intercept adjacent to the carrying scout")
        .commit(&mut state)
        .expect("interception admission failed");
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut replay = loaded
        .into_state(&registries)
        .expect("carrying scout saved");
    for _ in 0..5 {
        let a = advance_tick(&registries, &mut state).unwrap();
        let b = advance_tick(&registries, &mut replay).unwrap();
        assert_eq!(a.kobold_scout(), b.kobold_scout());
        if state.tick().value() == 195 {
            let event = a.kobold_scout().expect("retreat is locally observable");
            assert!(event.fled_player());
            assert_eq!(event.abandoned_drop(), Some(drop));
            assert_eq!(event.relocated_drop(), None);
        }
    }
    assert_eq!(state, replay);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn blocked_flight_releases_material_without_moving_or_multiplying_it() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, tip] = colony(&state).route;
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    validate_admit_player(&registries, &state, den)
        .expect("player can guard the only retreat cell")
        .commit(&mut state)
        .expect("guard must enter authoritative world");
    advance_to_tick(&registries, &mut state, 194);
    let event = advance_tick(&registries, &mut state)
        .expect("blocked retreat must still complete slow tick")
        .kobold_scout()
        .expect("abandoned cargo should be observable near the player");
    assert!(
        !event.fled_player(),
        "no false movement through the occupied den"
    );
    assert_eq!(event.abandoned_drop(), Some(drop));
    assert_eq!(event.relocated_drop(), None);
    assert_eq!(event.position(), junction);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(colony(&state).scavenged_drop, None);
    assert_eq!(state.surface().get(drop).unwrap().position(), junction);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn tampered_scavenged_parcel_identity_is_rejected_on_load() {
    let (registries, mut state, _) = generated_opening();
    let tip = colony(&state).route[2];
    let drop = place_loose_stone_drop(&registries, &mut state, tip);
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scavenged_drop, Some(drop));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["kobolds"]["colony"]["scavenged_drop"] = serde_json::json!(999999);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn opaque_terrain_conceals_a_close_scout_until_line_of_sight_is_cleared() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .expect("generated stone floor exists");
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let [den, junction, tip] = colony(&state).route;
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_position(), tip);
    assert_eq!(state.world().block_at(junction), Some(BlockStateId::AIR));
    validate_admit_player(&registries, &state, den)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let visible = available_nearby_kobold_scouts(&registries, &state)
        .next()
        .expect("unobstructed scout is visible at two voxels");
    assert_eq!(visible.position(), tip);
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    set_generated_block_for_test(registries.world(), &mut state, junction, BLOCK_STATE_STONE);
    let hidden_scout = available_nearby_kobold_scouts(&registries, &state).collect::<Vec<_>>();
    assert!(
        hidden_scout
            .iter()
            .all(|resident| resident.role() != crate::kobolds::KoboldRole::Scout),
        "a stone wall must conceal the scout across the junction"
    );
    assert!(
        hidden_scout.iter().any(|resident| {
            resident.role() == crate::kobolds::KoboldRole::Watchman && resident.position() == den
        }),
        "the original den resident remains physically visible beside the player"
    );

    set_generated_block_for_test(registries.world(), &mut state, junction, BlockStateId::AIR);
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .unwrap()
            .position(),
        tip
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(
        available_nearby_kobold_scouts(&registries, &state)
            .next()
            .is_some()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn opaque_wall_hides_a_moving_scout_event_without_blocking_its_actual_route() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };
    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap();
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let [den, junction, tip] = colony(&state).route;
    let (mid, observer) = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .find_map(|(dx, dz)| {
            let mid = VoxelCoord::new(junction.x() + dx, junction.y(), junction.z() + dz);
            let observer = VoxelCoord::new(mid.x() + dx, mid.y(), mid.z() + dz);
            (mid != den
                && mid != tip
                && state.world().block_at(mid) == Some(BlockStateId::AIR)
                && state.world().block_at(observer) == Some(BlockStateId::AIR))
            .then_some((mid, observer))
        })
        .expect("opening terrain includes one clear off-route two-voxel sightline");
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    crate::logistics::relocate_player_for_test(&mut state, observer);
    advance_to_tick(&registries, &mut state, 179);
    let mut concealed = state.clone();
    set_generated_block_for_test(registries.world(), &mut concealed, mid, BLOCK_STATE_STONE);
    let clear_event = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(clear_event.kobold_scout().unwrap().position(), junction);
    let concealed_event = advance_tick(&registries, &mut concealed).unwrap();
    assert!(
        concealed_event.kobold_scout().is_none(),
        "the same moving scout must not leak through a nearby opaque wall"
    );
    assert_eq!(
        colony(&state).scout_position(),
        colony(&concealed).scout_position(),
        "sight occlusion must not stop or teleport the actual scout"
    );
    assert!(
        available_nearby_kobold_scouts(&registries, &concealed)
            .next()
            .is_none()
    );
    set_generated_block_for_test(registries.world(), &mut concealed, mid, BlockStateId::AIR);
    assert_eq!(
        available_nearby_kobold_scouts(&registries, &concealed)
            .next()
            .unwrap()
            .position(),
        junction
    );
}

#[test]
fn a_diagonal_scout_sighting_does_not_peek_through_an_opaque_corner() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    let anchor = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(anchor),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(anchor.x(), y, anchor.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap();
    let spawn = VoxelCoord::new(anchor.x(), surface_y + 1, anchor.z());
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let scout = colony(&state).route[1];
    let diagonal = VoxelCoord::new(scout.x() + 1, scout.y(), scout.z() + 1);
    let x_leg = VoxelCoord::new(diagonal.x(), scout.y(), scout.z());
    let z_leg = VoxelCoord::new(scout.x(), scout.y(), diagonal.z());
    for voxel in [scout, diagonal, x_leg, z_leg] {
        assert_eq!(state.world().block_at(voxel), Some(BlockStateId::AIR));
    }
    assert!(crate::kobolds::scout_visible_from(
        &registries,
        &state,
        scout,
        diagonal
    ));
    for occlusion in [x_leg, z_leg] {
        set_generated_block_for_test(registries.world(), &mut state, occlusion, BLOCK_STATE_STONE);
        assert!(
            !crate::kobolds::scout_visible_from(&registries, &state, scout, diagonal),
            "a solid wall on either side of a corner conceals the scout"
        );
        set_generated_block_for_test(registries.world(), &mut state, occlusion, BlockStateId::AIR);
    }
    assert!(crate::kobolds::scout_visible_from(
        &registries,
        &state,
        scout,
        diagonal
    ));
}

#[test]
fn terrain_barrier_after_first_patrol_step_sends_scout_home_without_teleportation() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, spawn) = generated_terrain_scout_opening();
    let [den, junction, tip] = colony(&state).route;
    advance_to_tick(&registries, &mut state, 180);
    assert_eq!(colony(&state).scout_position(), junction);
    assert_eq!(state.world().block_at(tip), Some(BlockStateId::AIR));

    // The observer beyond the den now actually guards the return edge.
    // An obstructed scout must wait at the junction until both the route
    // and the visible threat allow it to reach home.
    let beyond = VoxelCoord::new(
        den.x() + (den.x() - junction.x()),
        den.y(),
        den.z() + (den.z() - junction.z()),
    );
    validate_admit_player(&registries, &state, beyond)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = calculate_matter_accounting(&state).unwrap().total();
    // Block standing head clearance without burying the finite natural
    // opportunity already present on the trail floor at tip.
    let tip_head = VoxelCoord::new(tip.x(), tip.y() + 1, tip.z());
    set_generated_block_for_test(registries.world(), &mut state, tip_head, BLOCK_STATE_STONE);
    advance_to_tick(&registries, &mut state, 184);
    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(!colony(&state).returning);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    assert_eq!(outcome.kobold_scout(), None);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(state, loaded);
    crate::logistics::relocate_player_for_test(&mut state, spawn);
    crate::logistics::relocate_player_for_test(&mut loaded, spawn);
    advance_to_tick(&registries, &mut state, 190);
    advance_to_tick(&registries, &mut loaded, 190);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), den);
    assert!(colony(&state).returning);
    set_generated_block_for_test(registries.world(), &mut state, tip_head, BlockStateId::AIR);
    set_generated_block_for_test(registries.world(), &mut loaded, tip_head, BlockStateId::AIR);
    advance_to_tick(&registries, &mut state, 200);
    advance_to_tick(&registries, &mut loaded, 200);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(
        colony(&state).wary_until,
        Some(SimulationTick::new(230)),
        "the den watchman's physically delivered warning keeps the scout cautious after retreat"
    );
    advance_to_tick(&registries, &mut state, 235);
    advance_to_tick(&registries, &mut loaded, 235);
    assert_eq!(state, loaded);
    assert_eq!(colony(&state).scout_position(), tip);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn blocked_far_cache_goal_cancels_detour_and_preserves_original_parcel() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{BlockStateId, set_generated_block_for_test};
    let (registries, mut state, spawn) = generated_terrain_scout_opening();
    let [den, junction, tip] = colony(&state).route;
    let cache = place_loose_stone_drop(&registries, &mut state, spawn);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to_tick(&registries, &mut state, 185);
    assert_eq!(colony(&state).scout_step, 2);
    assert_eq!(colony(&state).detour_goal, Some(spawn));
    let branch = colony(&state).scout_position();
    // Keep the cache's real drop exposed while closing the headroom above it.
    let cache_head = VoxelCoord::new(spawn.x(), spawn.y() + 1, spawn.z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        cache_head,
        BLOCK_STATE_STONE,
    );
    advance_to_tick(&registries, &mut state, 190);
    assert_eq!(colony(&state).scout_position(), junction);
    assert!(colony(&state).returning);
    assert_eq!(state.surface().get(cache).unwrap().position(), spawn);
    advance_to_tick(&registries, &mut state, 195);
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(colony(&state).detour_goal, None);
    assert_eq!(colony(&state).detour, None);
    assert_eq!(colony(&state).route, [den, junction, tip]);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        cache_head,
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut loaded,
        cache_head,
        BlockStateId::AIR,
    );
    for _ in 0..30 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_ne!(branch, spawn);
    assert_eq!(state.surface().get(cache).unwrap().position(), den);
    assert_eq!(
        state.surface().get(cache).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn terrain_scout_waits_for_floor_and_head_clearance_and_resumes_after_repair() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::state::validate_loaded_state;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
    for block_floor in [true, false] {
        let registries = build_registries();
        let mut state = AppState::new();
        let horizontal = VoxelCoord::new(8, 0, 8);
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::containing(horizontal),
            BLOCK_STATE_STONE,
        )
        .unwrap_or_else(|error| panic!("patrol terrain generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("patrol terrain commit failed: {error}"));
        let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
            .rev()
            .find(|&y| {
                state
                    .world()
                    .block_at(VoxelCoord::new(horizontal.x(), y, horizontal.z()))
                    == Some(BLOCK_STATE_STONE)
            })
            .unwrap_or_else(|| panic!("patrol terrain surface missing"));
        let spawn = VoxelCoord::new(horizontal.x(), surface_y + 1, horizontal.z());
        validate_generate_initial_wilderness(&registries, &state, seed, spawn)
            .unwrap_or_else(|error| panic!("terrain patrol opening rejected: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("terrain patrol opening commit failed: {error}"));
        validate_admit_player(&registries, &state, spawn)
            .unwrap_or_else(|error| panic!("terrain patrol player admission failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("terrain patrol player commit failed: {error}"));

        let [den, junction, _] = colony(&state).route;
        let blocked_y = if block_floor {
            junction.y() - 1
        } else {
            junction.y() + 1
        };
        let blocked = VoxelCoord::new(junction.x(), blocked_y, junction.z());
        let original = state
            .world()
            .block_at(blocked)
            .unwrap_or_else(|| panic!("patrol obstacle coordinate not generated"));
        let obstruction = if block_floor {
            BlockStateId::AIR
        } else {
            BLOCK_STATE_STONE
        };
        assert_ne!(original, obstruction);
        set_generated_block_for_test(registries.world(), &mut state, blocked, obstruction);
        assert!(
            !state
                .world()
                .has_standing_clearance(registries.world(), junction)
        );

        for _ in 0..179 {
            let _ = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("obstructed patrol prelude failed: {error}"));
        }
        let blocked_step = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("obstructed patrol advance failed: {error}"));
        assert_eq!(blocked_step.kobold_scout(), None);
        assert_eq!(colony(&state).scout_position(), den);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("blocked patrol save failed: {error}"));
        let envelope: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("blocked patrol decode failed: {error}"));
        let mut loaded = envelope
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("blocked patrol load failed: {error}"));
        assert_eq!(loaded, state);

        set_generated_block_for_test(registries.world(), &mut state, blocked, original);
        set_generated_block_for_test(registries.world(), &mut loaded, blocked, original);
        assert!(
            state
                .world()
                .has_standing_clearance(registries.world(), junction)
        );
        for _ in 0..5 {
            let original_step = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("repaired patrol advance failed: {error}"));
            let loaded_step = advance_tick(&registries, &mut loaded)
                .unwrap_or_else(|error| panic!("loaded repaired patrol advance failed: {error}"));
            assert_eq!(original_step.kobold_scout(), loaded_step.kobold_scout());
        }
        assert_eq!(colony(&state).scout_position(), junction);
        assert_eq!(loaded, state);
        assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    }
}

#[test]
fn kobold_invalid_colony_route_is_rejected_from_trusted_save() {
    let (registries, state, _) = generated_opening();
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    tampered["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(99);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

#[test]
fn kobold_invalid_patrol_schedule_is_rejected_from_trusted_save() {
    let (registries, state, _) = generated_opening();
    for due in [181_u64, 200_u64] {
        let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("patrol schedule fixture failed: {error}"));
        tampered["state"]["systems"]["kobolds"]["colony"]["next_step_at"] = serde_json::json!(due);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(tampered)
            .unwrap_or_else(|error| panic!("patrol schedule fixture decode failed: {error}"));
        assert!(decoded.into_state(&registries).is_err());
    }
}

#[test]
fn kobold_scout_does_not_pass_through_the_player() {
    let (registries, mut state, _) = generated_opening();
    let [den, junction, _] = colony(&state).route;
    validate_admit_player(&registries, &state, junction)
        .expect("player should be admitted at the contested junction")
        .commit(&mut state)
        .expect("junction admission should commit");
    for _ in 0..179 {
        let _ = advance_tick(&registries, &mut state).expect("quiet opening should advance");
    }
    let outcome = advance_tick(&registries, &mut state).expect("blocked patrol should advance");
    assert!(outcome.kobold_scout().is_none());
    assert_eq!(colony(&state).scout_position(), den);
    assert_eq!(validate_loaded_kobolds(&registries, &state), Ok(()));
}
