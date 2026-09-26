//! Contracts for inventory-owned empty stockpile allocation.

use super::*;
use crate::core::quantity::Mass;
use crate::core::state::AppState;

fn profile() -> StockpileStorageProfile {
    StockpileStorageProfile::unbounded_solid_only()
}

#[test]
fn validated_empty_stockpile_allocation_creates_exact_capacity_and_advances_owner() {
    let mut state = AppState::new();
    let capacity = Mass::from_milligrams(12_000_000);
    let before_revision = state.inventory().revision();
    let validated = validate_empty_stockpile_allocation(state.inventory(), capacity, profile())
        .unwrap_or_else(|error| panic!("empty stockpile allocation validation failed: {error}"));
    let expected = validated.stockpile();

    let stockpile = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("empty stockpile allocation commit failed: {error}"));

    assert_eq!(stockpile, expected);
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("allocated stockpile disappeared"));
    assert_eq!(record.capacity(), capacity);
    assert_eq!(record.stored_mass(), Mass::ZERO);
    assert!(record.enclosure().is_none());
    assert!(record.supported_by().is_none());
    assert_eq!(state.inventory().revision(), before_revision + 1);
}

#[test]
fn empty_stockpile_allocation_rejects_zero_capacity_without_mutation() {
    let state = AppState::new();
    assert_eq!(
        validate_empty_stockpile_allocation(state.inventory(), Mass::ZERO, profile()).err(),
        Some(EmptyStockpileAllocationError::ZeroCapacity)
    );
    assert!(state.inventory().stockpiles().next().is_none());
}

#[test]
fn empty_stockpile_allocation_rejects_stale_commit_without_mutation() {
    let mut state = AppState::new();
    let first = validate_empty_stockpile_allocation(
        state.inventory(),
        Mass::from_milligrams(10),
        profile(),
    )
    .unwrap_or_else(|error| panic!("first allocation validation failed: {error}"));
    let competing = validate_empty_stockpile_allocation(
        state.inventory(),
        Mass::from_milligrams(20),
        profile(),
    )
    .unwrap_or_else(|error| panic!("competing allocation validation failed: {error}"));
    competing
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("competing allocation commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        first.commit(&mut state),
        Err(
            EmptyStockpileAllocationCommitError::StaleInventoryRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
}
