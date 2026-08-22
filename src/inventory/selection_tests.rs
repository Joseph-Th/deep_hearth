//! Tests for the sibling selection module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};

#[test]
fn explicit_selection_binds_partial_lot_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
    let before = state.clone();

    let selection = validate_explicit_consumption_selection(
        state.inventory(),
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(7))],
    )
    .unwrap_or_else(|error| panic!("explicit selection validation failed: {error:?}"));

    assert_eq!(selection.total_consumed(), Mass::from_milligrams(7));
    assert_eq!(selection.consumed_inputs().len(), 1);
    assert_eq!(
        selection.consumed_inputs()[0].mass(),
        Mass::from_milligrams(7)
    );
    assert_eq!(
        selection.consumed_inputs()[0].profile().temperature(),
        Temperature::from_millikelvin(300_000)
    );
    assert_eq!(state, before);
}

#[test]
fn explicit_selection_rejects_duplicate_lot_and_wrong_source() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_0002));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection source fixture failed: {error}"));
    let other = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("explicit selection secondary fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("explicit selection lot fixture failed: {error}"));
    let before = state.clone();
    let slice = MaterialLotSelection::new(lot, Mass::from_milligrams(5));

    assert_eq!(
        validate_explicit_consumption_selection(state.inventory(), source, &[slice, slice],),
        Err(ExplicitConsumptionSelectionError::DuplicateLot { lot })
    );
    assert_eq!(
        validate_explicit_consumption_selection(state.inventory(), other, &[slice]),
        Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot,
            requested_source: other,
            actual_source: source,
        })
    );
    assert_eq!(state, before);
}
