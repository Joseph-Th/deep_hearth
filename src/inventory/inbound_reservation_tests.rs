//! Exact capacity return for canceled cross-owner inbound reservations.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    add_solid_stockpile_for_test, validate_inbound_reservation,
    validate_inbound_reservation_release,
};

#[test]
fn reservation_release_returns_exact_capacity_under_one_inventory_revision() {
    let mut state = AppState::new();
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reservation-release stockpile failed: {error}"));
    let reservation =
        validate_inbound_reservation(state.inventory(), stockpile, Mass::from_milligrams(40))
            .unwrap_or_else(|error| panic!("reservation validation failed: {error:?}"));
    reservation.apply(state.inventory_state_mut());
    let reserved_revision = state.inventory().revision();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(40))
    );

    let release = validate_inbound_reservation_release(
        state.inventory(),
        stockpile,
        Mass::from_milligrams(40),
    )
    .unwrap_or_else(|error| panic!("reservation release validation failed: {error:?}"));
    assert_eq!(release.expected_revision(), reserved_revision);
    release.apply(state.inventory_state_mut());

    assert_eq!(state.inventory().revision(), reserved_revision + 1);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .map(|record| record.reserved_inbound()),
        Some(Mass::ZERO)
    );
}
