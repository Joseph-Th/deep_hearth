//! Unit contracts for stockpile-record observations and capacity accounting.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;

use super::{StockpileId, StockpileRecord, StockpileStorageProfile};

#[test]
fn available_capacity_subtracts_stored_and_reserved_mass() {
    let stockpile = StockpileRecord {
        id: StockpileId::new(1),
        capacity: Mass::from_milligrams(1_000),
        storage_profile: StockpileStorageProfile::unbounded_solid_only(),
        enclosure: None,
        supported_by: None,
        stored_mass: Mass::from_milligrams(350),
        reserved_inbound: Mass::from_milligrams(125),
        contents: BTreeMap::new(),
    };

    assert_eq!(stockpile.available_capacity(), Mass::from_milligrams(525));
}
