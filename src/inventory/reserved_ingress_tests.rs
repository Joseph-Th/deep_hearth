//! Tests for the sibling reserved ingress module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_LUMP, MATERIAL_CHARCOAL, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::inventory::add_solid_stockpile_for_test;
use crate::inventory::deposit_lot_for_test;
use crate::material::CommodityKey;

#[test]
fn reserved_deposit_plan_owns_lot_ids_and_revision_advance() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3001));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reserved ingress stockpile fixture failed: {error}"));
    let expected_revision = state.inventory().revision();
    let first_lot_id = state.inventory().next_lot_id();
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(10);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );

    let plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        SimulationTick::new(7),
        vec![ReservedDepositRequest::new(
            destination,
            vec![output],
            Mass::from_milligrams(10),
            0,
        )],
    )
    .unwrap_or_else(|error| panic!("reserved ingress planning failed: {error:?}"));
    assert_eq!(plan.expected_revision(), expected_revision);
    assert_eq!(state.inventory().revision(), expected_revision);

    apply_reserved_deposits(state.inventory_state_mut(), plan);

    assert_eq!(state.inventory().revision(), expected_revision + 1);
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("reserved ingress destination disappeared"));
    assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(10));
    let lot = state
        .inventory()
        .get_lot(MaterialLotId::new(first_lot_id))
        .unwrap_or_else(|| panic!("reserved ingress did not use inventory-owned lot cursor"));
    assert_eq!(lot.stockpile(), destination);
    assert_eq!(lot.temperature(), Temperature::from_millikelvin(500_000));
}

#[test]
fn empty_reserved_deposit_plan_is_a_true_noop() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3002));
    let before = state.clone();
    let plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        SimulationTick::new(1),
        Vec::new(),
    )
    .unwrap_or_else(|error| panic!("empty reserved ingress planning failed: {error:?}"));

    apply_reserved_deposits(state.inventory_state_mut(), plan);

    assert_eq!(state, before);
}

#[test]
fn reserved_output_merges_without_consuming_an_unused_lot_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3003));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("reserved ingress stockpile fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(4),
        Temperature::from_millikelvin(500_000),
    )
    .unwrap_or_else(|error| panic!("reserved ingress seed lot failed: {error}"));
    let cursor_before = state.inventory().next_lot_id();
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(6);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(6),
        Temperature::from_millikelvin(500_000),
    );
    let plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        state.tick(),
        vec![ReservedDepositRequest::new(
            destination,
            vec![output],
            Mass::from_milligrams(6),
            0,
        )],
    )
    .unwrap_or_else(|error| panic!("reserved ingress merge planning failed: {error:?}"));

    apply_reserved_deposits(state.inventory_state_mut(), plan);

    assert_eq!(state.inventory().next_lot_id(), cursor_before);
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
}
