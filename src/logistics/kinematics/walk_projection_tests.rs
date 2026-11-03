//! Local walk forecasts: actor liveness, exact capacity and save/load.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::quantity::Energy;
use crate::core::state::validate_loaded_state;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
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
