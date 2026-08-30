//! Contract tests for reserved material ingress planning and commit.

use super::*;
use crate::content::{FORM_LUMP, MATERIAL_CHARCOAL, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::{SimulationTick, WorldSeed};
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, STORAGE_AGE_PARTS_PER_TICK, add_solid_stockpile_for_test,
    deposit_lot_for_test,
};
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
        SimulationTick::new(7),
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
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
        state.tick(),
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
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

#[test]
fn delayed_reserved_output_uses_admission_time_for_merging_and_preserves_creation_time() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3004));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("delayed ingress stockpile fixture failed: {error}"));
    let admitted_at = SimulationTick::new(5);
    apply_clock_advance(&mut state, admitted_at);
    let existing = deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(4),
        Temperature::from_millikelvin(500_000),
    )
    .unwrap_or_else(|error| panic!("delayed ingress seed lot failed: {error}"));
    let cursor_before = state.inventory().next_lot_id();
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(6);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(6),
        Temperature::from_millikelvin(500_000),
    );
    let provenance_created_at = SimulationTick::new(2);
    let storage_age_parts = 3 * STORAGE_AGE_PARTS_PER_TICK;

    let plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        provenance_created_at,
        admitted_at,
        vec![ReservedDepositRequest::new(
            destination,
            vec![output],
            storage_age_parts,
        )],
    )
    .unwrap_or_else(|error| panic!("delayed reserved ingress planning failed: {error:?}"));
    apply_reserved_deposits(state.inventory_state_mut(), plan);

    assert_eq!(state.inventory().next_lot_id(), cursor_before);
    let merged = state
        .inventory()
        .get_lot(existing)
        .unwrap_or_else(|| panic!("delayed reserved ingress did not merge into existing lot"));
    assert_eq!(merged.mass(), Mass::from_milligrams(10));
    assert_eq!(merged.created_at(), provenance_created_at);
    assert_eq!(merged.latest_created_at(), admitted_at);
    assert_eq!(
        merged
            .storage_history()
            .project(admitted_at, AMBIENT_PRESERVATION_MULTIPLIER_PPM),
        Some(storage_age_parts)
    );
}

#[test]
fn malformed_reserved_deposit_identity_plan_fails_before_authoritative_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3005));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("malformed reserved ingress stockpile failed: {error}"));
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(10);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );
    let mut plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        state.tick(),
        state.tick(),
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
    )
    .unwrap_or_else(|error| panic!("malformed reserved ingress planning failed: {error:?}"));
    plan.entries[0].lot_ids.clear();
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_reserved_deposits(state.inventory_state_mut(), plan);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn shape_valid_but_wrong_reserved_identity_fails_before_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3006));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("wrong reserved identity stockpile failed: {error}"));
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(10);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );
    let mut plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        state.tick(),
        state.tick(),
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
    )
    .unwrap_or_else(|error| panic!("wrong reserved identity planning failed: {error:?}"));
    plan.entries[0].lot_ids[0] = MaterialLotId::new(plan.entries[0].lot_ids[0].value() + 1);
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_reserved_deposits(state.inventory_state_mut(), plan);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn shape_valid_but_wrong_reserved_cursor_fails_before_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3007));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("wrong reserved cursor stockpile failed: {error}"));
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(10);
    let output = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(500_000),
    );
    let mut plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        state.tick(),
        state.tick(),
        vec![ReservedDepositRequest::new(destination, vec![output], 0)],
    )
    .unwrap_or_else(|error| panic!("wrong reserved cursor planning failed: {error:?}"));
    plan.next_lot_id += 1;
    let before = state.clone();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_reserved_deposits(state.inventory_state_mut(), plan);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn same_destination_reserved_entries_are_preflighted_as_one_total() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A70_3008));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("combined reserved stockpile failed: {error}"));
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(10);
    let first = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(6),
        Temperature::from_millikelvin(500_000),
    );
    let second = MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
        Mass::from_milligrams(4),
        Temperature::from_millikelvin(500_000),
    );
    let plan = decide_reserved_deposits(
        &registries,
        state.inventory(),
        state.tick(),
        state.tick(),
        vec![
            ReservedDepositRequest::new(destination, vec![first], 0),
            ReservedDepositRequest::new(destination, vec![second], 0),
        ],
    )
    .unwrap_or_else(|error| panic!("combined reserved planning failed: {error:?}"));

    // Simulate an internal ownership defect that preserves the inventory revision while reducing
    // the aggregate reservation below the two planned entries combined.
    get_stockpile_mut_or_panic(state.inventory_state_mut(), destination).reserved_inbound =
        Mass::from_milligrams(9);
    let before = state.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_reserved_deposits(state.inventory_state_mut(), plan);
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}

#[test]
fn empty_reserved_deposit_request_is_rejected_at_construction() {
    let destination = StockpileId::new(1);
    assert!(
        std::panic::catch_unwind(|| ReservedDepositRequest::new(destination, Vec::new(), 0))
            .is_err()
    );
}
