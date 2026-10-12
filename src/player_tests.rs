//! Atomic ordinary-player admission and stale-owner rejection contracts.

use crate::content::build_registries;
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{StockpileStorageProfile, add_stockpile};
use crate::logistics::validate_player_logistics_initialization;
use crate::spatial::VoxelCoord;
use crate::survival::initialize_player_survival;

use super::{PlayerAdmissionCommitError, PlayerAdmissionError, validate_admit_player};

#[test]
fn ordinary_player_admission_commits_all_owner_state_together() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(4, -2, 7);

    let admitted = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player admission commit failed: {error}"));

    assert!(state.survival().player().is_some());
    assert_eq!(state.logistics().player().copied(), Some(admitted));
    assert_eq!(admitted.position(), position);
    let carried = state
        .inventory()
        .get_stockpile(admitted.carried_stockpile())
        .unwrap_or_else(|| panic!("admitted carried stockpile disappeared"));
    assert_eq!(
        carried.capacity(),
        registries.survival().physiology().carried_mass_capacity()
    );
    assert_eq!(carried.stored_mass(), Mass::ZERO);
}

#[test]
fn ordinary_player_admission_rejects_partial_survival_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival-only fixture failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: true,
            logistics_initialized: false,
        })
    );
}

#[test]
fn ordinary_player_admission_rejects_partial_logistics_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_player_logistics_initialization(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("logistics-only fixture failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("logistics-only fixture commit failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: false,
            logistics_initialized: true,
        })
    );
}

#[test]
fn stale_inventory_rejects_player_admission_without_partial_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let validated = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"));
    let _competitor = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("competing inventory mutation failed: {error}"));
    let before_commit = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(PlayerAdmissionCommitError::StaleInventoryRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before_commit);
}
