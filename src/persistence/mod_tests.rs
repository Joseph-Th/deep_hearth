//! Persistence envelope, strict decoding, round-trip, and tamper-rejection tests.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries, make_test_registries_with_energy_store,
    make_test_registries_with_equipment, make_test_registries_with_sensible_heating,
};
use crate::core::quantity::{Area, Energy, Force, Mass, Power, Temperature};
use crate::core::state::apply_clock_advance;
use crate::core::time::SimulationTick;
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyValidationError,
    add_energy_store_with_initial_for_fixture,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentDefinitionId, EquipmentValidationError, add_equipment,
    validate_mount_equipment,
};
use crate::inventory::{
    InventoryValidationError, MaterialLotSelection, add_solid_stockpile_for_test,
    deposit_bulk_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
    validate_mount_stockpile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition, MaterialId};
use crate::production::{
    ProcessDefinition, ProcessId, ProductionValidationError, validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralDamageEvent, StructuralElementId, StructuralFailureCause, StructuralLoadKind,
    StructuralMutationOutcome, StructureValidationError, ValidatedStructuralMutation,
    add_structural_element, analyze_structure, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_link_support, validate_set_structural_load,
};
use crate::thermal::{
    SensibleHeatingProcessDefinition, SensibleHeatingRequest, ThermalJobValidationError,
    resolve_sensible_heating_process,
};

const TEST_EQUIPMENT_CAPABILITY: CapabilityId = CapabilityId::new(900_301);
const TEST_EQUIPMENT_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(900_301);
const TEST_ENERGY_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(900_401);
const TEST_HEAT_POWER: CapabilityId = CapabilityId::new(900_501);
const TEST_HEAT_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(900_502);
const TEST_HEAT_MAX_BATCH_MASS: CapabilityId = CapabilityId::new(900_503);
const TEST_HEATER_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(900_501);
const TEST_HEAT_ENERGY_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(900_501);
const TEST_HEAT_PROCESS: ProcessId = ProcessId::new(900_501);

fn make_test_energy_registries() -> Registries {
    make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
        TEST_ENERGY_DEFINITION,
        "persistence test electrical buffer",
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(1_000_000),
        Power::ZERO,
        Power::from_microwatts(100_000),
    ))
}

#[path = "mod_tests/history.rs"]
mod history;

fn make_test_heating_registries() -> Registries {
    let profile = match CapabilityProfile::new([
        (
            TEST_HEAT_POWER,
            CapabilityValue::Power(Power::from_microwatts(1_000_000)),
        ),
        (
            TEST_HEAT_MAX_TEMPERATURE,
            CapabilityValue::Temperature(Temperature::from_millikelvin(400_000)),
        ),
        (
            TEST_HEAT_MAX_BATCH_MASS,
            CapabilityValue::Mass(Mass::from_milligrams(20)),
        ),
    ]) {
        Ok(profile) => profile,
        Err(error) => panic!("heating persistence capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("heating persistence maintenance fixture failed: {error}"),
    };
    let process = ProcessDefinition::new(
        TEST_HEAT_PROCESS,
        "persistence sensible heating",
        vec![
            CapabilityRequirement::new(
                TEST_HEAT_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_microwatts(100_000)),
            ),
            CapabilityRequirement::new(
                TEST_HEAT_MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(350_000)),
            ),
            CapabilityRequirement::new(
                TEST_HEAT_MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    );
    make_test_registries_with_sensible_heating(
        vec![
            CapabilityDefinition::new(
                TEST_HEAT_POWER,
                "persistence heating power",
                CapabilityValueKind::Power,
            ),
            CapabilityDefinition::new(
                TEST_HEAT_MAX_TEMPERATURE,
                "persistence heating maximum temperature",
                CapabilityValueKind::Temperature,
            ),
            CapabilityDefinition::new(
                TEST_HEAT_MAX_BATCH_MASS,
                "persistence heating maximum batch mass",
                CapabilityValueKind::Mass,
            ),
        ],
        EquipmentDefinition::new(
            TEST_HEATER_DEFINITION,
            "persistence heater",
            Mass::from_milligrams(1_000_000),
            profile,
            thresholds,
        ),
        vec![EnergyStoreDefinition::new_with_transfer_limits(
            TEST_HEAT_ENERGY_DEFINITION,
            "persistence electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000_000_000),
            Power::ZERO,
            Power::from_microwatts(500_000),
        )],
        process,
        SensibleHeatingProcessDefinition::new(
            TEST_HEAT_PROCESS,
            TEST_HEAT_POWER,
            TEST_HEAT_MAX_TEMPERATURE,
            TEST_HEAT_MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1_000,
        ),
    )
}

fn make_started_test_heating_state() -> (Registries, AppState) {
    let registries = make_test_heating_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("heating persistence source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("heating persistence destination failed: {error}"));
    let input_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("heating persistence material fixture failed: {error}"));
    let equipment = add_equipment(
        &registries,
        &mut state,
        TEST_HEATER_DEFINITION,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("heating persistence equipment failed: {error}"));
    let energy_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_HEAT_ENERGY_DEFINITION,
        Energy::from_nanojoules(500_000_000),
    )
    .unwrap_or_else(|error| panic!("heating persistence energy failed: {error}"));
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            TEST_HEAT_PROCESS,
            source,
            &[MaterialLotSelection::new(
                input_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
    )
    .unwrap_or_else(|error| panic!("heating persistence resolution failed: {error}"));
    validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("heating persistence start validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("heating persistence start commit failed: {error}"));
    (registries, state)
}

fn replace_serialized_field(
    encoded: &str,
    field: &str,
    original: &serde_json::Value,
    replacement: &serde_json::Value,
) -> String {
    let original = serde_json::to_string(original)
        .unwrap_or_else(|error| panic!("JSON field fixture failed serialization: {error}"));
    let replacement = serde_json::to_string(replacement)
        .unwrap_or_else(|error| panic!("JSON field replacement failed serialization: {error}"));
    let needle = format!("\"{field}\":{original}");
    assert!(
        encoded.contains(&needle),
        "serialized fixture omitted field {field}"
    );
    encoded.replacen(&needle, &format!("\"{field}\":{replacement}"), 1)
}

fn duplicate_first_object_entry(encoded: &str, field: &str, object: &serde_json::Value) -> String {
    let entries = object
        .as_object()
        .unwrap_or_else(|| panic!("duplicate-key fixture field {field} is not an object"));
    let (key, value) = entries
        .iter()
        .next()
        .unwrap_or_else(|| panic!("duplicate-key fixture field {field} is empty"));
    let original = serde_json::to_string(object)
        .unwrap_or_else(|error| panic!("duplicate-key fixture serialization failed: {error}"));
    let key = serde_json::to_string(key)
        .unwrap_or_else(|error| panic!("duplicate-key fixture key failed serialization: {error}"));
    let value = serde_json::to_string(value).unwrap_or_else(|error| {
        panic!("duplicate-key fixture value failed serialization: {error}")
    });
    let duplicate = format!("{{{key}:{value},{key}:{value}}}");
    let needle = format!("\"{field}\":{original}");
    assert!(
        encoded.contains(&needle),
        "serialized fixture omitted field {field}"
    );
    encoded.replacen(&needle, &format!("\"{field}\":{duplicate}"), 1)
}

#[path = "mod_tests/decode.rs"]
mod decode;

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("condition fixture failed: {error}"),
    }
}

fn make_test_equipment_registries() -> Registries {
    let profile = match CapabilityProfile::new([(
        TEST_EQUIPMENT_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(125_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("equipment capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(650_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("equipment maintenance fixture failed: {error}"),
    };
    make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_EQUIPMENT_CAPABILITY,
            "test equipment supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_EQUIPMENT_DEFINITION,
            "persistence test equipment",
            Mass::from_milligrams(80_000),
            profile,
            thresholds,
        ),
    )
}

fn make_test_structural_bounds(x: i64, y: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(x, y, 0), VoxelCoord::new(x + 1, y + 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("structural persistence bounds fixture failed: {error}"),
    }
}

fn make_test_structural_element(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
    y: i64,
    is_grounded: bool,
) -> StructuralElementId {
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            make_test_structural_bounds(x, y),
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        is_grounded,
    ) {
        Ok(element) => element,
        Err(error) => panic!("structural persistence element fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    element
}

fn commit_test_structural_mutation(
    token: ValidatedStructuralMutation,
    state: &mut AppState,
) -> StructuralMutationOutcome {
    match token.commit(state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("structural persistence mutation commit failed: {error}"),
    }
}

fn activate_test_structural_element(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
) {
    let token = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("structural persistence activation failed: {error}"),
    };
    let _ = commit_test_structural_mutation(token, state);
}

fn link_test_structural_support(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    support: StructuralElementId,
) {
    let token = match validate_link_support(registries, state, element, support) {
        Ok(token) => token,
        Err(error) => panic!("structural persistence support link failed: {error}"),
    };
    let _ = commit_test_structural_mutation(token, state);
}

#[path = "mod_tests/envelope.rs"]
mod envelope;

#[path = "mod_tests/structural.rs"]
mod structural;

#[path = "mod_tests/equipment_energy.rs"]
mod equipment_energy;

#[path = "mod_tests/inflight.rs"]
mod inflight;

#[path = "mod_tests/material.rs"]
mod material;
