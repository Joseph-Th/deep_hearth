//! Manual-power projection boundary contracts.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, MANUAL_POWER_HAND_CRANK,
    build_registries,
};

#[test]
fn projection_accepts_exact_store_capacity_and_rejects_one_nanojoule_more() {
    let registries = build_registries();
    let capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel definition disappeared"))
        .capacity();

    let exact = project_manual_power(
        &registries,
        MANUAL_POWER_HAND_CRANK,
        EQUIPMENT_STONE_HAND_CRANK,
        Condition::PRISTINE,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        capacity,
    )
    .unwrap_or_else(|error| panic!("exact-capacity manual-power projection failed: {error}"));
    assert!(!exact.duration().is_zero());

    let above_capacity = Energy::from_nanojoules(
        capacity
            .nanojoules()
            .checked_add(1)
            .unwrap_or_else(|| panic!("stone flywheel capacity cannot be incremented")),
    );
    assert_eq!(
        project_manual_power(
            &registries,
            MANUAL_POWER_HAND_CRANK,
            EQUIPMENT_STONE_HAND_CRANK,
            Condition::PRISTINE,
            ENERGY_STONE_FLYWHEEL_DRIVE,
            above_capacity,
        ),
        Err(ManualPowerProjectionError::EnergyExceedsStoreCapacity {
            store: ENERGY_STONE_FLYWHEEL_DRIVE,
            requested: above_capacity,
            capacity,
        })
    );
}

#[test]
fn projection_rejects_equipment_that_requires_structural_installation() {
    let registries = build_registries();
    let method = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .copied()
        .unwrap_or_else(|| panic!("hand-crank manual-power definition disappeared"));
    let equipment = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .unwrap_or_else(|| panic!("stone hand-crank definition disappeared"))
        .clone()
        .with_required_structural_support();
    let store = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel definition disappeared"));

    assert_eq!(
        project_manual_power_configuration(
            registries.core(),
            registries.survival().physiology(),
            method,
            &equipment,
            Condition::PRISTINE,
            store,
            Energy::from_nanojoules(1),
        ),
        Err(
            ManualPowerProjectionError::EquipmentRequiresStructuralSupport {
                equipment: EQUIPMENT_STONE_HAND_CRANK,
            }
        )
    );
}
