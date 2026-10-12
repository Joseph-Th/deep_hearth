//! Contracts for ordinary face-adjacent open-space player movement.

use crate::content::build_registries;
use crate::content::{FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::inventory::deposit_lot_for_test;
use crate::logistics::{
    GroundStockpileAllocationError, PlayerGroundStockpileAllocationError,
    validate_allocate_player_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::project_survival_resource_budget;

use super::*;

#[test]
fn nonadjacent_player_step_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("movement admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("movement admission commit failed: {error}"));
    let before = state.clone();
    let destination = VoxelCoord::new(1, 1, 0);
    assert_eq!(
        validate_start_player_step(&registries, &state, destination).err(),
        Some(PlayerStepError::DestinationNotAdjacent {
            origin,
            destination
        })
    );
    assert_eq!(state, before);
}

fn admitted_state(origin: VoxelCoord) -> (crate::registry::Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("movement fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("movement fixture admission commit failed: {error}"));
    (registries, state)
}

#[test]
fn adjacent_player_step_is_delayed_and_pays_canonical_survival_cost() {
    let origin = VoxelCoord::new(0, 0, 0);
    let destination = VoxelCoord::new(1, 0, 0);
    let (registries, mut state) = admitted_state(origin);
    let physiology = registries.survival().physiology();
    let carrying = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("movement carrying assessment disappeared"));
    let exertion = physiology
        .locomotion()
        .exertion_for_load(carrying.stored(), carrying.capacity());
    let expected = project_survival_resource_budget(
        physiology,
        exertion,
        physiology.locomotion().step_duration(),
    )
    .unwrap_or_else(|error| panic!("movement survival projection failed: {error:?}"));
    let before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("movement survival player disappeared"));

    let work = validate_start_player_step(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("adjacent movement validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("adjacent movement commit failed: {error}"));
    assert_eq!(work.origin(), origin);
    assert_eq!(work.destination(), destination);
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(origin)
    );

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("adjacent movement tick failed: {error}"));
    let step = outcome
        .player_step()
        .unwrap_or_else(|| panic!("movement tick lost its step outcome"));
    assert_eq!(step.origin(), origin);
    assert_eq!(step.destination(), destination);
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(destination)
    );
    assert_eq!(state.player_work().active(), None);

    let after = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("movement survival player disappeared after tick"));
    assert_eq!(
        before
            .metabolic_energy()
            .checked_sub(after.metabolic_energy()),
        Some(expected.metabolic_energy())
    );
    assert_eq!(
        before.hydration().checked_sub(after.hydration()),
        Some(expected.hydration())
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn carried_mass_increases_player_step_survival_cost() {
    let origin = VoxelCoord::new(0, 0, 0);
    let destination = VoxelCoord::new(1, 0, 0);
    let (registries, mut unloaded) = admitted_state(origin);
    let mut loaded = unloaded.clone();
    let carried = loaded
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("loaded movement fixture lost carried stockpile"));
    deposit_lot_for_test(
        &registries,
        &mut loaded,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(12_500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("loaded movement material fixture failed: {error}"));

    let before_unloaded = unloaded.survival().player().copied().unwrap();
    let before_loaded = loaded.survival().player().copied().unwrap();
    for state in [&mut unloaded, &mut loaded] {
        validate_start_player_step(&registries, state, destination)
            .unwrap_or_else(|error| panic!("load-sensitive movement validation failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("load-sensitive movement commit failed: {error}"));
        let _ = advance_tick(&registries, state)
            .unwrap_or_else(|error| panic!("load-sensitive movement tick failed: {error}"));
    }
    let after_unloaded = unloaded.survival().player().copied().unwrap();
    let after_loaded = loaded.survival().player().copied().unwrap();
    let unloaded_energy = before_unloaded.metabolic_energy().nanojoules()
        - after_unloaded.metabolic_energy().nanojoules();
    let loaded_energy = before_loaded.metabolic_energy().nanojoules()
        - after_loaded.metabolic_energy().nanojoules();
    let unloaded_hydration =
        before_unloaded.hydration().microliters() - after_unloaded.hydration().microliters();
    let loaded_hydration =
        before_loaded.hydration().microliters() - after_loaded.hydration().microliters();
    assert!(loaded_energy > unloaded_energy);
    assert!(loaded_hydration > unloaded_hydration);
}

#[test]
fn active_player_step_round_trips_and_replays_deterministically() {
    let origin = VoxelCoord::new(-4, 2, 9);
    let destination = VoxelCoord::new(-4, 3, 9);
    let (registries, mut state) = admitted_state(origin);
    validate_start_player_step(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("movement replay validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("movement replay commit failed: {error}"));
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("movement replay save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("movement replay decode failed: {error}"));
    let mut resumed = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("movement replay trusted load failed: {error}"));
    let mut uninterrupted = state;
    let resumed_outcome = advance_tick(&registries, &mut resumed)
        .unwrap_or_else(|error| panic!("movement resumed tick failed: {error}"));
    let uninterrupted_outcome = advance_tick(&registries, &mut uninterrupted)
        .unwrap_or_else(|error| panic!("movement uninterrupted tick failed: {error}"));
    assert_eq!(resumed_outcome, uninterrupted_outcome);
    assert_eq!(resumed, uninterrupted);
}

#[test]
fn active_player_step_reserves_its_future_logistics_revision() {
    let origin = VoxelCoord::new(0, 0, 0);
    let destination = VoxelCoord::new(1, 0, 0);
    let (registries, mut state) = admitted_state(origin);
    validate_start_player_step(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("movement headroom validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("movement headroom commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("movement headroom serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded.clone())
        .unwrap_or_else(|error| panic!("movement headroom decode failed: {error}"));
    let mut near_exhausted = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("movement reserved headroom should load: {error}"));
    assert_eq!(
        validate_allocate_player_ground_stockpile(&near_exhausted, Mass::from_milligrams(1)).err(),
        Some(PlayerGroundStockpileAllocationError::Allocation(
            GroundStockpileAllocationError::LogisticsRevisionExhausted,
        ))
    );
    let _ = advance_tick(&registries, &mut near_exhausted)
        .unwrap_or_else(|error| panic!("movement reserved-headroom completion failed: {error}"));
    assert_eq!(near_exhausted.logistics().revision(), u64::MAX);
    assert_eq!(
        near_exhausted
            .logistics()
            .player()
            .map(|player| player.position()),
        Some(destination)
    );

    encoded["state"]["systems"]["logistics"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("movement exhausted-headroom decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureLogisticsRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
            }
        ))
    );
}
