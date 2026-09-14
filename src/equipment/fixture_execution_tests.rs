//! Contract tests for controlled equipment fixture allocation and condition setup.

use super::*;
use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
};
use crate::content::make_test_registries_with_equipment;
use crate::content::{EQUIPMENT_STONE_PICK, build_registries};
use crate::core::quantity::Mass;
use crate::core::time::WorldSeed;
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
use crate::maintenance::MaintenanceThresholds;

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(810_001);
const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(810_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("condition fixture failed: {error}"),
    }
}

#[test]
fn bootstrap_creation_cannot_bypass_authored_assembly() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(19));
    let before = state.clone();

    assert_eq!(
        add_equipment(
            &registries,
            &mut state,
            EQUIPMENT_STONE_PICK,
            Condition::PRISTINE,
        ),
        Err(AddEquipmentError::RequiresAssembly {
            definition: EQUIPMENT_STONE_PICK,
        })
    );
    assert_eq!(state, before);
}

fn make_registries() -> Registries {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(50_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("maintenance fixture failed: {error}"),
    };
    make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "test press",
            Mass::from_milligrams(40_000),
            profile,
            thresholds,
        ),
    )
}

#[test]
fn creation_and_condition_fixture_use_canonical_revisioned_state() {
    let registries = make_registries();
    let mut state = AppState::new(WorldSeed::new(17));
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment creation failed: {error}"),
    };
    degrade_equipment_condition_for_test(&mut state, equipment, 300_000);

    let record = match state.equipment().get_equipment(equipment) {
        Some(record) => record,
        None => panic!("equipment disappeared after condition changes"),
    };
    assert_eq!(record.condition(), condition(700_000));
    assert_eq!(state.equipment().revision(), 2);
}
