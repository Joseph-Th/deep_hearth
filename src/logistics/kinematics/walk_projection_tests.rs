//! Local walk forecasts: actor liveness, exact capacity and save/load.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::quantity::Energy;
use crate::core::state::validate_loaded_state;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::simulation::advance_control_tick_with_input;
use crate::survival::{Vitality, player_record};
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn dead_player_walk_load_forecast_rejects_liveness_before_overcapacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5741_4C4B_4C49_5645),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(0, y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let at = VoxelCoord::new(0, floor + 1, 0);
    assert!(state.world().has_standing_clearance(registries.world(), at));
    validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let goal = [VoxelCoord::new(1, at.y(), 0)];
    let overloaded = Mass::from_milligrams(25_000_001);
    assert_eq!(
        project_player_walk_path_for_load(&registries, &state, &goal, overloaded),
        Err(PlayerWalkProjectionError::ProjectedLoadExceedsCapacity {
            carried: overloaded,
            capacity: Mass::from_milligrams(25_000_000),
        }),
        "a living actor should still get exact physical carrying limits"
    );

    let player = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let dead = state.clone();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for actor in [&state, &replay] {
        assert_eq!(
            project_player_walk_path_for_load(&registries, actor, &goal, overloaded),
            Err(PlayerWalkProjectionError::PlayerDead),
            "a dead player cannot get a hypothetical-load walk forecast"
        );
    }
    assert_eq!(state, dead);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    for actor in [&mut state, &mut replay] {
        let _ = validate_respawn_player(&registries, actor)
            .unwrap()
            .commit(actor)
            .unwrap();
        assert_eq!(
            project_player_walk_path_for_load(&registries, actor, &goal, overloaded),
            Err(PlayerWalkProjectionError::ProjectedLoadExceedsCapacity {
                carried: overloaded,
                capacity: Mass::from_milligrams(25_000_000),
            })
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

/// Ordinary generated-world corridor shared by canonical-motion regression tests.
fn generated_two_cell_runway() -> (Registries, AppState, [VoxelCoord; 2]) {
    let registries = build_registries();
    let mut world = AppState::new();
    validate_generate_chunk(
        &registries,
        &world,
        WorldGenerationSeed::new(0x5741_4C4B_4C49_5645),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut world)
    .unwrap();

    // Select actual flat, unobstructed generated terrain rather than supplying
    // an unphysical path fixture or relying on an ungenerated traversal graph.
    let mut runway = None;
    'sites: for z in 2..12 {
        for x in 2..12 {
            let Some(floor) = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE - 2).rev().find(|&y| {
                world.world().block_at(VoxelCoord::new(x, y, z)) == Some(BLOCK_STATE_STONE)
            }) else {
                continue;
            };
            let y = floor + 1;
            if !(0..=2).all(|step| {
                let position = VoxelCoord::new(x + step, y, z);
                world.world().block_at(VoxelCoord::new(x + step, floor, z))
                    == Some(BLOCK_STATE_STONE)
                    && world
                        .world()
                        .has_standing_clearance(registries.world(), position)
            }) {
                continue;
            }
            let origin = VoxelCoord::new(x, y, z);
            let destinations = [VoxelCoord::new(x + 1, y, z), VoxelCoord::new(x + 2, y, z)];
            let Ok(admission) = validate_admit_player(&registries, &world, origin) else {
                continue;
            };
            let mut actor = world.clone();
            admission.commit(&mut actor).unwrap();
            if project_player_walk_path(&registries, &actor, &destinations).is_ok() {
                runway = Some((actor, destinations));
                break 'sites;
            }
        }
    }
    let (state, destinations) = runway.expect("seeded stone chunk has a usable two-cell runway");
    (registries, state, destinations)
}

#[test]
fn straight_walk_path_matches_continuous_held_control_without_waypoint_braking() {
    let (registries, state, destinations) = generated_two_cell_runway();
    let forecast = project_player_walk_path(&registries, &state, &destinations).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        project_player_walk_path(&registries, &replay, &destinations),
        Ok(forecast),
        "route projection must be continuation-stable"
    );
    let mut live = state.clone();
    let held = PlayerControlInput::new(
        HorizontalControlInput::new(CONTROL_INPUT_PARTS_PER_MILLION, 0).unwrap(),
    );
    let mut ticks = 0;
    while live.logistics().player().unwrap().position() != destinations[1] {
        assert!(ticks < CONTROL_TICKS_PER_SIMULATION_TICK - 1);
        let _ = advance_control_tick_with_input(&registries, &mut live, held).unwrap();
        ticks += 1;
    }
    while live
        .logistics()
        .player()
        .unwrap()
        .body()
        .horizontal_velocity()
        != crate::spatial::HorizontalVelocity::ZERO
    {
        assert!(ticks < CONTROL_TICKS_PER_SIMULATION_TICK - 1);
        let _ =
            advance_control_tick_with_input(&registries, &mut live, PlayerControlInput::NEUTRAL)
                .unwrap();
        ticks += 1;
        assert_eq!(
            live.logistics().player().unwrap().position(),
            destinations[1]
        );
    }
    assert_eq!(forecast.control_ticks(), ticks);
    assert_eq!(
        forecast.locomotion_exertion(),
        live.logistics()
            .player()
            .unwrap()
            .locomotion_exertion()
            .pending()
    );
    assert_eq!(
        calculate_matter_accounting(&live).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &live), Ok(()));
}

#[test]
fn reversed_walk_leg_brakes_before_turning_and_matches_live_motion() {
    let (registries, state, destinations) = generated_two_cell_runway();
    let origin = state.logistics().player().unwrap().position();
    let route = [destinations[0], origin];
    let forecast = project_player_walk_path(&registries, &state, &route).unwrap();
    let mut live = state.clone();
    let commands = [
        PlayerControlInput::new(
            HorizontalControlInput::new(CONTROL_INPUT_PARTS_PER_MILLION, 0).unwrap(),
        ),
        PlayerControlInput::new(
            HorizontalControlInput::new(-CONTROL_INPUT_PARTS_PER_MILLION, 0).unwrap(),
        ),
    ];
    let mut control_ticks = 0_u64;
    for (destination, command) in route.into_iter().zip(commands) {
        while live.logistics().player().unwrap().position() != destination {
            assert!(control_ticks < CONTROL_TICKS_PER_SIMULATION_TICK - 1);
            let _ = advance_control_tick_with_input(&registries, &mut live, command).unwrap();
            control_ticks += 1;
        }
        while live
            .logistics()
            .player()
            .unwrap()
            .body()
            .horizontal_velocity()
            != crate::spatial::HorizontalVelocity::ZERO
        {
            assert!(control_ticks < CONTROL_TICKS_PER_SIMULATION_TICK - 1);
            let _ = advance_control_tick_with_input(
                &registries,
                &mut live,
                PlayerControlInput::NEUTRAL,
            )
            .unwrap();
            control_ticks += 1;
            assert_eq!(live.logistics().player().unwrap().position(), destination);
        }
    }
    assert_eq!(forecast.control_ticks(), control_ticks);
    assert_eq!(
        forecast.locomotion_exertion(),
        live.logistics()
            .player()
            .unwrap()
            .locomotion_exertion()
            .pending()
    );
    assert_eq!(live.logistics().player().unwrap().position(), origin);
    assert_eq!(validate_loaded_state(&registries, &live), Ok(()));
}
