//! Primitive contact uses the authoritative player clock and save graph.

use super::*;
use crate::content::build_registries;
use crate::core::state::validate_loaded_state;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;

fn east() -> ViewDirection {
    ViewDirection::new(1_000_000, 0, 0).unwrap()
}

#[test]
fn unadmitted_player_cannot_strike_or_create_recovery_history() {
    let registries = build_registries();
    let state = AppState::new();
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveShoveError::PlayerNotAdmitted)
    );
    assert_eq!(state.combat().revision(), 0);
}

#[test]
fn recovery_has_a_real_deadline_and_survives_persistence() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let first = validate_primitive_shove(&registries, &state, east());
    assert!(matches!(
        first,
        Err(PrimitiveShoveError::Target(
            PlayerBlockTargetError::WorldAuthorityUnavailable { .. }
        ))
    ));
    // The target fails closed before recovery is consumed.
    assert_eq!(state.combat().revision(), 0);
    state.combat_state_mut().apply_shove(0, ControlTick::ZERO);
    assert_eq!(
        validate_primitive_shove(&registries, &state, east()).err(),
        Some(PrimitiveShoveError::Recovering {
            last_at: ControlTick::ZERO
        })
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, loaded);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn close_range_shove_startles_a_real_scout_and_retreat_replays_after_save() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::logistics::add_traversable_edge_for_test;
    use crate::simulation::{
        PlayerDirectControlInput, PrimaryActionOutcome, PrimaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input, advance_tick,
    };
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(0, 0, 0);
    let junction = VoxelCoord::new(1, 0, 0);
    let tip = VoxelCoord::new(2, 0, 0);
    for (from, to) in [(den, junction), (junction, tip)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(7),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for x in 0..=4 {
        let ground = VoxelCoord::new(x, -1, 0);
        set_generated_block_for_test(registries.world(), &mut state, ground, BLOCK_STATE_STONE);
        for y in 0..=2 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    for _ in 0..180 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        close_contact_at(&state, junction),
        Ok(Some(KoboldRole::Scout))
    );
    validate_admit_player(&registries, &state, tip)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let west = ViewDirection::new(-1_000_000, 0, 0).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let shove = PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::shove(west));
    for game in [&mut state, &mut replay] {
        let action = advance_control_tick_with_direct_input(&registries, game, shove).unwrap();
        assert!(matches!(
            action.primary_action(),
            Some(PrimaryActionOutcome::Shove(
                PrimitiveShoveOutcome::Startled {
                    resident: KoboldRole::Scout,
                    ..
                }
            ))
        ));
        assert_eq!(game.combat().revision(), 1);
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
        let repeated = advance_control_tick_with_direct_input(&registries, game, shove).unwrap();
        assert!(matches!(
            repeated.primary_action(),
            Some(PrimaryActionOutcome::ShoveRejected(
                PrimitiveShoveError::Recovering { .. }
            ))
        ));
    }
    assert_eq!(state, replay);
    while state.tick().value() < 185 {
        let left = advance_control_tick(&registries, &mut state).unwrap();
        let right = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(left, right);
    }
    assert_eq!(state, replay);
    assert_eq!(
        crate::kobolds::available_nearby_kobold_scouts(&registries, &state)
            .find(|resident| resident.role() == KoboldRole::Scout)
            .map(|resident| resident.position()),
        Some(den),
        "the struck scout must withdraw through a real patrol edge"
    );
}

#[test]
fn saved_future_combat_history_is_rejected() {
    let registries = build_registries();
    let mut state = AppState::new();
    state.combat_state_mut().apply_shove(0, ControlTick::new(1));
    assert!(validate_loaded_state(&registries, &state).is_err());
}
