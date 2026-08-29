//! Contract tests for authoritative material ingress.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::material::CommodityKey;

use super::super::fixture::add_stockpile;
use super::super::state::StockpileStorageProfile;

fn add_test_stockpile(state: &mut AppState, capacity: Mass) -> StockpileId {
    match add_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    ) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("ingress stockpile fixture failed: {error:?}"),
    }
}

fn wood_log_spec(mass: Mass) -> MaterialLotSpec {
    MaterialLotSpec::new(
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        Temperature::from_millikelvin(293_150),
    )
}

#[test]
fn empty_ingress_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A61_0001));
    let destination = add_test_stockpile(&mut state, Mass::from_milligrams(10));
    let before = state.clone();
    let current_tick = state.tick();

    let result = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        std::iter::empty::<MaterialIngressEntry>(),
        current_tick,
    );

    assert_eq!(result, Err(MaterialIngressError::Empty));
    assert_eq!(state, before);
}

#[test]
fn compatible_ingress_reuses_existing_identity_without_advancing_lot_cursor() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A61_0004));
    let destination = add_test_stockpile(&mut state, Mass::from_milligrams(10));
    let current_tick = state.tick();
    let first = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        [MaterialIngressEntry::from_lot_spec(
            wood_log_spec(Mass::from_milligrams(2)),
            current_tick,
        )],
        current_tick,
    )
    .unwrap_or_else(|error| panic!("first ingress validation failed: {error:?}"));
    let first_lots = apply_material_ingress(state.inventory_state_mut(), first);
    let [first_lot] = first_lots.as_slice() else {
        panic!("single first ingress must resolve one lot identity");
    };
    let cursor_before_merge = state.inventory().next_lot_id();

    let second = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        [MaterialIngressEntry::from_lot_spec(
            wood_log_spec(Mass::from_milligrams(3)),
            current_tick,
        )],
        current_tick,
    )
    .unwrap_or_else(|error| panic!("second ingress validation failed: {error:?}"));
    let second_lots = apply_material_ingress(state.inventory_state_mut(), second);

    assert_eq!(second_lots.as_slice(), &[*first_lot]);
    assert_eq!(state.inventory().next_lot_id(), cursor_before_merge);
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(
        state
            .inventory()
            .get_lot(*first_lot)
            .map(MaterialLotRecord::mass),
        Some(Mass::from_milligrams(5))
    );
}

#[test]
fn compatible_parcels_in_one_ingress_allocate_only_one_persistent_identity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A61_0005));
    let destination = add_test_stockpile(&mut state, Mass::from_milligrams(10));
    let current_tick = state.tick();
    let cursor_before = state.inventory().next_lot_id();
    let ingress = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        [
            MaterialIngressEntry::from_lot_spec(
                wood_log_spec(Mass::from_milligrams(2)),
                current_tick,
            ),
            MaterialIngressEntry::from_lot_spec(
                wood_log_spec(Mass::from_milligrams(3)),
                current_tick,
            ),
        ],
        current_tick,
    )
    .unwrap_or_else(|error| panic!("batched ingress validation failed: {error:?}"));
    let resulting_lots = apply_material_ingress(state.inventory_state_mut(), ingress);

    assert_eq!(resulting_lots.len(), 2);
    assert_eq!(resulting_lots[0], resulting_lots[1]);
    assert_eq!(state.inventory().next_lot_id(), cursor_before + 1);
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
}

#[test]
fn future_provenance_is_rejected_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A61_0002));
    let destination = add_test_stockpile(&mut state, Mass::from_milligrams(10));
    let current_tick = state.tick();
    let future_tick = SimulationTick::new(current_tick.value() + 1);
    let entry =
        MaterialIngressEntry::from_lot_spec(wood_log_spec(Mass::from_milligrams(1)), future_tick);
    let before = state.clone();

    let result = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        [entry],
        current_tick,
    );

    assert_eq!(
        result,
        Err(MaterialIngressError::ProvenanceInFuture {
            latest: future_tick,
            current: current_tick,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn aggregate_capacity_is_checked_before_any_ingress_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A61_0003));
    let capacity = Mass::from_milligrams(5);
    let destination = add_test_stockpile(&mut state, capacity);
    let current_tick = state.tick();
    let entries = [
        MaterialIngressEntry::from_lot_spec(wood_log_spec(Mass::from_milligrams(3)), current_tick),
        MaterialIngressEntry::from_lot_spec(wood_log_spec(Mass::from_milligrams(3)), current_tick),
    ];
    let before = state.clone();

    let result = validate_material_ingress(
        &registries,
        state.inventory(),
        destination,
        entries,
        current_tick,
    );

    assert_eq!(
        result,
        Err(MaterialIngressError::CapacityExceeded {
            stockpile: destination,
            capacity,
            committed: Mass::ZERO,
            requested: Mass::from_milligrams(6),
        })
    );
    assert_eq!(state, before);
}
