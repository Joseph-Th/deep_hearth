//! Contract tests for exact inventory selection.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_STONE, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::material::{CompositionComponent, MaterialComposition};

#[test]
fn implicit_selection_uses_ordered_eligible_lots_and_stops_at_requested_mass() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_0003));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("implicit selection source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let first = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(4),
        Temperature::from_millikelvin(290_000),
    )
    .unwrap_or_else(|error| panic!("first implicit selection lot failed: {error}"));
    let second = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(7),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("second implicit selection lot failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(50),
        Temperature::from_millikelvin(310_000),
    )
    .unwrap_or_else(|error| panic!("unused implicit selection lot failed: {error}"));

    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, Mass::from_milligrams(6))],
    )
    .unwrap_or_else(|error| panic!("implicit selection validation failed: {error:?}"));

    assert_eq!(selection.total_consumed(), Mass::from_milligrams(6));
    assert_eq!(selection.lot_slices.len(), 2);
    assert_eq!(
        selection.lot_slices[0],
        LotSlice {
            lot: first,
            mass: Mass::from_milligrams(4)
        }
    );
    assert_eq!(
        selection.lot_slices[1],
        LotSlice {
            lot: second,
            mass: Mass::from_milligrams(2)
        }
    );
}

#[test]
fn implicit_selection_shortage_reports_only_composition_eligible_free_mass() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_0004));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("implicit shortage source fixture failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(3),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("pure implicit shortage lot failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("implicit shortage composition failed: {error}"));
    deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(40),
        Temperature::from_millikelvin(300_000),
        mixed,
    )
    .unwrap_or_else(|error| panic!("mixed implicit shortage lot failed: {error}"));

    assert_eq!(
        validate_consumption_selection(
            state.inventory(),
            source,
            &[MaterialInputSpec::pure(commodity, Mass::from_milligrams(4))],
        ),
        Err(ConsumptionSelectionError::InsufficientMass {
            stockpile: source,
            commodity,
            available: Mass::from_milligrams(3),
            requested: Mass::from_milligrams(4),
        })
    );
}

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
