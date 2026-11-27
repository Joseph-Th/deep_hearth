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

#[test]
fn unsafe_fuel_feedback_names_the_material_and_exact_temperature_limit() {
    use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
    use deep_hearth::core::quantity::Temperature;
    use deep_hearth::material::CommodityKey;

    let registries = build_registries();
    let fuel = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let name = registries.materials().commodity_name(fuel).unwrap();
    let message = fuel_blocker(
        &registries,
        CombustionError::FuelTooHot {
            fuel,
            temperature: Temperature::from_millikelvin(353_151),
            maximum: Temperature::from_millikelvin(353_150),
        },
    );
    assert_eq!(
        message,
        format!("{name} is too hot for hand fueling (80.001 C; maximum 80 C)")
    );
}
