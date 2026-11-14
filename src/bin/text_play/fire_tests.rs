//! Fire admission and text-feedback regressions.

use super::*;
use deep_hearth::core::quantity::Energy;

#[test]
fn thermal_capacity_blocker_reports_charge_and_store_in_megajoules() {
    let session = super::super::Session::new(24318).unwrap();
    let million_joules = 1_000_000_000_000_000;
    let message = fuel_blocker(
        &session.registries,
        CombustionError::InsufficientThermalCapacity {
            store: EnergyStoreId::new(1),
            stored: Energy::from_nanojoules(20 * million_joules),
            released: Energy::from_nanojoules(12 * million_joules),
            capacity: Energy::from_nanojoules(24 * million_joules),
        },
    );
    assert_eq!(
        message,
        "fire has 20.000 MJ stored / 24.000 MJ capacity; this charge adds 12.000 MJ. Let it burn down before adding fuel"
    );
    assert!(!message.contains(" nJ"));
}
