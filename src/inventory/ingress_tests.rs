//! Tests for the sibling ingress module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::material::CommodityKey;

use super::super::state::StockpileStorageProfile;
use super::super::transactions::add_stockpile;

fn add_test_stockpile(state: &mut AppState, capacity: Mass) -> StockpileId {
    match add_stockpile(state, capacity, StockpileStorageProfile::solid_only()) {
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
