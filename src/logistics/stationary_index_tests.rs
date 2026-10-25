//! Ground custody and stationary occupancy share one rebuildable local index.

use super::*;
use crate::content::build_registries;
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::inventory::add_solid_stockpile_for_test;
use crate::logistics::validate_place_ground_stockpile;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::survival::{Vitality, player_record};

#[test]
fn stationary_occupancy_uses_exact_positions_and_survives_reconstruction() {
    let registries = build_registries();
    let mut state = AppState::new();
    let shared = VoxelCoord::new(1, 2, 3);
    let far = VoxelCoord::new(10_000, 2, 3);
    let empty = VoxelCoord::new(2, 2, 3);
    for index in 0..128 {
        let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .expect("finite test stockpile");
        let location = if index < 2 {
            shared
        } else {
            VoxelCoord::new(far.x() + i64::from(index), far.y(), far.z())
        };
        validate_place_ground_stockpile(&state, stockpile, location)
            .expect("canonical ground placement")
            .commit(&mut state)
            .expect("ground placement commit");
    }
    assert!(state.logistics().has_valid_stockpile_position_index());
    assert!(state.logistics().has_stationary_occupant_at(shared));
    assert!(
        state
            .logistics()
            .has_stationary_occupant_at(VoxelCoord::new(far.x() + 127, 2, 3))
    );
    assert!(!state.logistics().has_stationary_occupant_at(empty));
    assert!(!state.logistics().has_stationary_occupant_at(far));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let bytes =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).expect("valid persistence");
    assert!(!String::from_utf8_lossy(&bytes).contains("stockpiles_by_position"));
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .expect("saved envelope")
        .into_state(&registries)
        .expect("trusted reconstruction");
    assert_eq!(state, restored);
    assert!(restored.logistics().has_stationary_occupant_at(shared));
    assert!(!restored.logistics().has_stationary_occupant_at(empty));
}

#[test]
fn inconsistent_stockpile_index_is_rejected_and_save_rebuilds_it() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(-3, 5, 8);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .expect("finite ground stockpile");
    validate_place_ground_stockpile(&state, stockpile, position)
        .expect("ground placement")
        .commit(&mut state)
        .expect("ground placement commit");
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    assert!(
        state
            .logistics_state_mut()
            .stockpiles_by_position
            .get_mut(&position)
            .expect("indexed location")
            .remove(&stockpile)
    );
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Logistics(
            crate::logistics::LogisticsValidationError::StockpilePositionIndexInconsistent,
        ))
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .expect("index is derived and absent from save");
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .expect("decode saved ground stockpile")
        .into_state(&registries)
        .expect("rebuild index from authoritative locations");
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
    assert!(restored.logistics().has_valid_stockpile_position_index());
    assert!(restored.logistics().has_stationary_occupant_at(position));
}

#[test]
fn respawn_moves_only_the_old_carried_pack_into_stationary_index() {
    let registries = build_registries();
    let mut state = AppState::new();
    let death = VoxelCoord::new(1, 0, 0);
    validate_admit_player(&registries, &state, death)
        .expect("player admission")
        .commit(&mut state)
        .expect("player admission commit");
    let previous_pack = state
        .logistics()
        .player()
        .expect("player logistics")
        .carried_stockpile();
    assert!(!state.logistics().has_stationary_occupant_at(death));
    let player = state.survival().player().copied().expect("player survival");
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let outcome = validate_respawn_player(&registries, &state)
        .expect("valid respawn")
        .commit(&mut state)
        .expect("respawn commit");
    assert_eq!(outcome.death_cache(), previous_pack);
    assert_eq!(
        state
            .logistics()
            .stationary_stockpile_position(previous_pack),
        Some(death)
    );
    assert!(state.logistics().has_stationary_occupant_at(death));
    assert_eq!(state.logistics().stockpiles_by_position[&death].len(), 1);
    assert!(state.logistics().has_valid_stockpile_position_index());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).expect("persisted respawn");
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .expect("respawn decode")
        .into_state(&registries)
        .expect("respawn reconstruction");
    assert_eq!(state, restored);
}
