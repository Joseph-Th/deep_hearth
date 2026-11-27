//! Production control samples for short ledges, including persistence and effort.

use super::*;
use crate::core::state::validate_loaded_state;
use crate::logistics::{player_body_is_grounded, project_player_climb_target};

fn ledge_world(height: i64) -> (Registries, AppState, VoxelCoord) {
    let (registries, mut state, y) = generated_grounded_player_at(8, 8);
    let destination = VoxelCoord::new(9, y + height, 8);
    for dy in 0..=height + 3 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(8, y + dy, 8),
            BlockStateId::AIR,
        );
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(9, y + dy, 8),
            if dy < height {
                BLOCK_STATE_STONE
            } else {
                BlockStateId::AIR
            },
        );
    }
    (registries, state, destination)
}

fn east() -> HorizontalControlInput {
    HorizontalControlInput::new(1_000_000, 0).unwrap()
}

#[test]
fn two_metre_ledge_climb_is_gradual_costly_and_save_replay_exact() {
    let (registries, mut state, target) = ledge_world(2);
    assert_eq!(
        project_player_climb_target(&registries, &state, east()),
        Some(target)
    );
    let start = state.logistics().player().unwrap().body().foot_position();
    let mut resting = state.clone();
    let climb = PlayerControlInput::new(east()).with_climb(true);
    for _ in 0..25 {
        let _ = advance_control_tick_with_input(&registries, &mut state, climb).unwrap();
        let _ = advance_control_tick(&registries, &mut resting).unwrap();
    }
    let raised = state.logistics().player().unwrap().body().foot_position();
    assert!(raised.y_nanometers() > start.y_nanometers());
    assert!(raised.y_nanometers() < start.y_nanometers() + 2 * NANOMETERS_PER_VOXEL);
    assert_eq!(raised.x_nanometers(), start.x_nanometers());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..240 {
        let arrived = state.logistics().player().unwrap().body().voxel_position() == target;
        let input = if arrived {
            PlayerControlInput::NEUTRAL
        } else {
            climb
        };
        let outcome = advance_control_tick_with_input(&registries, &mut state, input).unwrap();
        let replayed = advance_control_tick_with_input(&registries, &mut replay, input).unwrap();
        assert_eq!(outcome, replayed);
        let _ = advance_control_tick(&registries, &mut resting).unwrap();
        if arrived && state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK == 0 {
            break;
        }
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.logistics().player().unwrap().body().voxel_position(),
        target
    );
    assert!(player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(
        state.survival().player().unwrap().metabolic_energy()
            < resting.survival().player().unwrap().metabolic_energy()
    );
    assert!(
        state.survival().player().unwrap().hydration()
            < resting.survival().player().unwrap().hydration()
    );
}

#[test]
fn ledge_climb_rejects_tall_wall_and_obstructed_lift() {
    let (registries, mut state, _) = ledge_world(3);
    assert_eq!(
        project_player_climb_target(&registries, &state, east()),
        None
    );
    let before = state.logistics().player().unwrap().body();
    let _ = advance_control_tick_with_input(
        &registries,
        &mut state,
        PlayerControlInput::new(east()).with_climb(true),
    )
    .unwrap();
    assert_eq!(state.logistics().player().unwrap().body(), before);

    let (registries, mut state, target) = ledge_world(2);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(8, target.y(), 8),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        project_player_climb_target(&registries, &state, east()),
        None
    );
}

#[test]
fn reaching_the_lip_finishes_the_lift_before_releasing_the_grip() {
    let (registries, mut state, target) = ledge_world(2);
    let climb = PlayerControlInput::new(east()).with_climb(true);
    let start = state.logistics().player().unwrap().body().foot_position();
    let lip = target.y() * NANOMETERS_PER_VOXEL;
    for _ in 0..240 {
        let _ = advance_control_tick_with_input(&registries, &mut state, climb).unwrap();
        if state
            .logistics()
            .player()
            .unwrap()
            .body()
            .foot_position()
            .y_nanometers()
            == lip
        {
            break;
        }
    }
    let at_lip = state.logistics().player().unwrap().body();
    assert_eq!(at_lip.foot_position().y_nanometers(), lip);
    assert_eq!(at_lip.foot_position().x_nanometers(), start.x_nanometers());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        at_lip.vertical_velocity().nanometers_per_second(),
        0,
        "the controlled lift ends at the lip, rather than storing an upward impulse"
    );
    let _ = advance_control_tick(&registries, &mut state).unwrap();
    let released = state.logistics().player().unwrap().body();
    assert!(released.foot_position().y_nanometers() < lip);
    assert!(released.vertical_velocity().nanometers_per_second() < 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn releasing_climb_resumes_gravity() {
    let (registries, mut state, _) = ledge_world(2);
    let start = state.logistics().player().unwrap().body().foot_position();
    let climb = PlayerControlInput::new(east()).with_climb(true);
    for _ in 0..25 {
        let _ = advance_control_tick_with_input(&registries, &mut state, climb).unwrap();
    }
    for _ in 0..60 {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.logistics().player().unwrap().body().foot_position(),
        start
    );
    assert!(player_body_is_grounded(&registries, &state));
}
