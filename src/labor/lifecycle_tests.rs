//! Admission contracts for exclusive player-work revision and survival budgeting.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::logistics::{HorizontalControlInput, PlayerControlInput};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::production::ProductionJobId;
use crate::simulation::{advance_control_tick, advance_control_tick_with_input};
use crate::spatial::VoxelCoord;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn slow_player_work_requires_ground_contact_in_generated_world() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xA1B0_0001),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("airborne-work terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("airborne-work terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("airborne-work terrain column has no surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 10, 0))
        .unwrap_or_else(|error| panic!("airborne-work player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("airborne-work player commit failed: {error}"));

    assert_eq!(
        validate_player_work_start(
            &registries,
            &state,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            TickSpan::new(1),
            SurvivalExertion::REST,
        ),
        Err(PlayerWorkStartError::PlayerAirborne)
    );
}

#[test]
fn slow_player_work_requires_body_to_stop() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xA1B0_0002),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("moving-work terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("moving-work terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("moving-work terrain column has no surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("moving-work player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("moving-work player commit failed: {error}"));
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(23_255, 0)
            .unwrap_or_else(|error| panic!("moving-work input fixture failed: {error}")),
    );
    while !state.control_tick().is_simulation_boundary()
        || state.control_tick() == ControlTick::ZERO
    {
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("moving-work control tick failed: {error}"));
    }
    assert!(state.logistics().player().is_some_and(|player| {
        player
            .body()
            .horizontal_velocity()
            .x_nanometers_per_second()
            > 0
    }));
    assert_eq!(
        validate_player_work_start(
            &registries,
            &state,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            TickSpan::new(1),
            SurvivalExertion::REST,
        ),
        Err(PlayerWorkStartError::PlayerMoving)
    );
}

#[test]
fn slow_player_work_rejects_grounded_upward_velocity_from_persisted_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xA1B0_0003),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("vertical-work terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("vertical-work terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("vertical-work terrain column has no surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("vertical-work player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("vertical-work player commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("vertical-work serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["vertical_velocity"]["nanometers_per_second"] =
        serde_json::json!(1_i64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("vertical-work decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle grounded upward-velocity state should remain loadable: {error}")
    });

    assert_eq!(
        validate_player_work_start(
            &registries,
            &loaded,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            TickSpan::new(1),
            SurvivalExertion::REST,
        ),
        Err(PlayerWorkStartError::PlayerMoving)
    );
}

#[test]
fn slow_player_work_requires_a_control_boundary_and_commit_cannot_cross_one() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("boundary fixture player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("boundary fixture player commit failed: {error}"));
    let work = PlayerWork::ManualProduction {
        job: ProductionJobId::new(1),
    };
    let token = validate_player_work_start(
        &registries,
        &state,
        work,
        TickSpan::new(1),
        SurvivalExertion::REST,
    )
    .unwrap_or_else(|error| panic!("boundary fixture work validation failed: {error}"));

    let outcome = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("boundary fixture control advance failed: {error}"));
    assert_eq!(outcome.control_tick(), ControlTick::new(1));
    assert!(outcome.simulation().is_none());
    assert_eq!(
        token.precheck(&state),
        Err(PlayerWorkCommitError::StaleControlTick {
            expected: ControlTick::ZERO,
            actual: ControlTick::new(1),
        })
    );
    assert!(matches!(
        validate_player_work_start(
            &registries,
            &state,
            work,
            TickSpan::new(1),
            SurvivalExertion::REST,
        ),
        Err(PlayerWorkStartError::SimulationBoundary(error))
            if error.control_tick() == ControlTick::new(1)
    ));
}

#[test]
fn player_work_admission_requires_revision_capacity_for_later_release() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("player-work revision fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("player-work revision fixture admission commit failed: {error}")
        });

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("player-work revision fixture serialization failed: {error}")
        });
    encoded["state"]["systems"]["player_work"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("player-work revision fixture decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle near-exhausted player-work revision fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_player_work_start(
            &registries,
            &loaded,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            TickSpan::new(1),
            SurvivalExertion::REST,
        )
        .err(),
        Some(PlayerWorkStartError::RevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn player_work_admission_requires_survival_revision_capacity_for_full_interval() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("survival-revision fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("survival-revision fixture admission commit failed: {error}")
        });

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("survival-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("survival-revision fixture decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle near-exhausted survival revision fixture should load: {error}")
    });
    let before = loaded.clone();
    let duration = TickSpan::new(2);

    assert_eq!(
        validate_player_work_start(
            &registries,
            &loaded,
            PlayerWork::ManualProduction {
                job: ProductionJobId::new(1),
            },
            duration,
            SurvivalExertion::REST,
        )
        .err(),
        Some(PlayerWorkStartError::SurvivalRevisionExhausted { duration })
    );
    assert_eq!(loaded, before);
}
