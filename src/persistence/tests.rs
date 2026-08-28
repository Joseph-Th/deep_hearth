//! Persistence envelope, strict decoding, round-trip, and tamper-rejection tests.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG,
    MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    make_test_registries_with_energy_store, make_test_registries_with_equipment,
    make_test_registries_with_process, make_test_registries_with_sensible_heating,
};
use crate::core::quantity::{Area, Energy, Force, Mass, Power, Temperature};
use crate::core::state::apply_clock_advance;
use crate::core::time::{SimulationTick, WorldSeed};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyValidationError,
    add_energy_store_with_initial_for_test,
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
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialInputSpec,
    MaterialLotSpec,
};
use crate::production::{
    ProcessDefinition, ProcessId, ProcessResolution, ProductionJobId, ProductionValidationError,
    make_test_process_resolution, validate_process_inputs, validate_start_process,
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

const TEST_PROCESS: ProcessId = ProcessId::new(900_101);
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
    make_test_registries_with_energy_store(EnergyStoreDefinition::new(
        TEST_ENERGY_DEFINITION,
        "persistence test electrical buffer",
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(1_000_000),
        Power::from_microwatts(100_000),
    ))
}

#[test]
fn future_material_storage_transition_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0028));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("storage-history stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("storage-history lot fixture failed: {error}"));
    let current = state.tick();
    let future = SimulationTick::new(current.value() + 1);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage-history tamper serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["last_transition_at"] = serde_json::json!(future.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("storage-history tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotStorageTransitionInFuture {
                lot,
                transition: future,
                current,
            }
        )))
    );
}

#[test]
fn impossible_material_history_times_are_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0029));
    apply_clock_advance(&mut state, SimulationTick::new(2));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("material-history stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("material-history lot fixture failed: {error}"));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("material-history serialization failed: {error}"));

    let mut future_provenance = base.clone();
    future_provenance["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["provenance"]
        ["latest_created_at"] = serde_json::json!(3_u64);
    let future_provenance: LoadedSaveEnvelope = serde_json::from_value(future_provenance)
        .unwrap_or_else(|error| panic!("future provenance decode failed: {error}"));
    assert_eq!(
        future_provenance.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotProvenanceInFuture {
                lot,
                latest: SimulationTick::new(3),
                current: SimulationTick::new(2),
            }
        )))
    );

    let mut early_transition = base;
    early_transition["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["last_transition_at"] = serde_json::json!(1_u64);
    let early_transition: LoadedSaveEnvelope = serde_json::from_value(early_transition)
        .unwrap_or_else(|error| panic!("early storage-transition decode failed: {error}"));
    assert_eq!(
        early_transition.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotStorageTransitionBeforeCreation {
                lot,
                transition: SimulationTick::new(1),
                created: SimulationTick::new(2),
            }
        )))
    );
}

#[test]
fn tampered_structural_unauthored_embodiment_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0018));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let molten_wood = CommodityKey::new(MATERIAL_WOOD, FORM_MOLTEN);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural liquid-embodiment save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["embodied_material"]
        [0]["profile"]["commodity"] = serde_json::json!(molten_wood.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => {
            panic!("tampered structural liquid-embodiment save failed decode: {error}")
        }
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::UnknownEmbodiedCommodity { element: member }
        )))
    );
}

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
    let process = ProcessDefinition::new_selected_batch(
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
        EnergyStoreDefinition::new(
            TEST_HEAT_ENERGY_DEFINITION,
            "persistence electrical buffer",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000_000_000),
            Power::from_microwatts(500_000),
        ),
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

#[test]
fn duplicate_persistent_map_and_set_entries_are_rejected_during_decode() {
    let registries = make_test_registries_with_process(make_test_process());
    let mut production_state = AppState::new(WorldSeed::new(0xD001_0001));
    let source = add_solid_stockpile_for_test(&mut production_state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("duplicate-job source fixture failed: {error}"));
    let destination =
        add_solid_stockpile_for_test(&mut production_state, Mass::from_milligrams(20))
            .unwrap_or_else(|error| panic!("duplicate-job destination fixture failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut production_state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("duplicate-job input fixture failed: {error}"));
    let resolution = make_test_resolution(&registries, &production_state, source);
    validate_start_process(
        &registries,
        &production_state,
        &resolution,
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("duplicate-job validation fixture failed: {error}"))
    .commit(&mut production_state)
    .unwrap_or_else(|error| panic!("duplicate-job commit fixture failed: {error}"));
    let production_value = serde_json::to_value(SaveEnvelope::new(&registries, &production_state))
        .unwrap_or_else(|error| panic!("duplicate-job fixture serialization failed: {error}"));
    let production_json = serde_json::to_string(&production_value)
        .unwrap_or_else(|error| panic!("duplicate-job JSON serialization failed: {error}"));
    let duplicate_jobs = duplicate_first_object_entry(
        &production_json,
        "jobs",
        &production_value["state"]["systems"]["production"]["jobs"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_jobs).is_err());

    let registries = build_registries();
    let mut inventory_state = AppState::new(WorldSeed::new(0xD001_0002));
    let stockpile = add_solid_stockpile_for_test(&mut inventory_state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("duplicate-inventory stockpile failed: {error}"));
    deposit_bulk_for_test(
        &registries,
        &mut inventory_state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("duplicate-inventory material failed: {error}"));
    let inventory_value = serde_json::to_value(SaveEnvelope::new(&registries, &inventory_state))
        .unwrap_or_else(|error| panic!("duplicate-inventory fixture failed: {error}"));
    let inventory_json = serde_json::to_string(&inventory_value)
        .unwrap_or_else(|error| panic!("duplicate-inventory JSON failed: {error}"));
    let duplicate_contents = duplicate_first_object_entry(
        &inventory_json,
        "contents",
        &inventory_value["state"]["systems"]["inventory"]["stockpiles"]
            [stockpile.value().to_string()]["contents"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_contents).is_err());

    let mut structure_state = AppState::new(WorldSeed::new(0xD001_0003));
    let foundation = make_test_structural_element(&registries, &mut structure_state, 0, 0, true);
    let upper = make_test_structural_element(&registries, &mut structure_state, 0, 1, false);
    link_test_structural_support(&registries, &mut structure_state, upper, foundation);
    let structure_value = serde_json::to_value(SaveEnvelope::new(&registries, &structure_state))
        .unwrap_or_else(|error| panic!("duplicate-structure fixture failed: {error}"));
    let structure_json = serde_json::to_string(&structure_value)
        .unwrap_or_else(|error| panic!("duplicate-structure JSON failed: {error}"));
    let duplicate_loads = duplicate_first_object_entry(
        &structure_json,
        "loads",
        &structure_value["state"]["systems"]["structures"]["elements"]
            [foundation.value().to_string()]["loads"],
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_loads).is_err());

    let supports = &structure_value["state"]["systems"]["structures"]["supports_by_element"];
    let mut duplicate_supports = supports.clone();
    let support_sets = duplicate_supports
        .as_object_mut()
        .unwrap_or_else(|| panic!("support-set fixture is not an object"));
    let support_set = support_sets
        .values_mut()
        .find(|value| value.as_array().is_some_and(|values| !values.is_empty()))
        .unwrap_or_else(|| panic!("support-set fixture has no linked support"));
    let duplicate = support_set
        .as_array()
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_else(|| panic!("support-set fixture lost its linked support"));
    support_set
        .as_array_mut()
        .unwrap_or_else(|| panic!("support-set fixture changed shape"))
        .push(duplicate);
    let duplicate_support_set = replace_serialized_field(
        &structure_json,
        "supports_by_element",
        supports,
        &duplicate_supports,
    );
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&duplicate_support_set).is_err());
}

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
    commit_test_structural_mutation(token, state);
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
    commit_test_structural_mutation(token, state);
}

fn make_test_process_with_input_mass(milligrams: u64) -> ProcessDefinition {
    ProcessDefinition::new(
        TEST_PROCESS,
        "persistence test transform",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(milligrams),
        )],
        Vec::new(),
    )
}

fn make_test_process() -> ProcessDefinition {
    make_test_process_with_input_mass(10)
}

fn make_test_resolution(
    registries: &Registries,
    state: &AppState,
    source: crate::inventory::StockpileId,
) -> ProcessResolution {
    let inputs = match validate_process_inputs(registries, state, TEST_PROCESS, source) {
        Ok(inputs) => inputs,
        Err(error) => panic!("persistence process input binding failed: {error}"),
    };
    make_test_process_resolution(
        inputs,
        5,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        )],
    )
}

#[test]
fn save_round_trip_preserves_authoritative_runtime_state() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5EED));
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(25),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    for _ in 0..37 {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("fixture tick unexpectedly failed: {error}");
        }
    }

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("save serialization unexpectedly failed: {error}"),
    };
    let encoded_value: serde_json::Value = match serde_json::from_slice(&encoded) {
        Ok(encoded) => encoded,
        Err(error) => panic!("serialized save failed JSON inspection: {error}"),
    };
    let systems = &encoded_value["state"]["systems"];
    let inventory = &systems["inventory"];
    assert!(inventory.get("lot_indexes").is_none());
    assert!(inventory.get("stockpiles_by_support").is_none());
    assert!(systems["fluid"].get("stores_by_support").is_none());
    assert!(systems["equipment"].get("equipment_by_support").is_none());
    assert!(systems["structures"].get("dependents_by_support").is_none());
    assert!(
        systems["geological_knowledge"]
            .get("observations_by_material")
            .is_none()
    );
    for owner in ["production", "mining"] {
        assert!(systems[owner].get("due_jobs").is_none());
        assert!(systems[owner].get("equipment_occupancy").is_none());
    }
    assert!(systems["production"].get("energy_occupancy").is_none());
    assert!(
        systems["production"]
            .get("output_stockpile_occupancy")
            .is_none()
    );
    let stockpile_value = &inventory["stockpiles"][stockpile.value().to_string()];
    assert!(stockpile_value.get("lot_ids").is_none());
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("save deserialization unexpectedly failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("decoded save unexpectedly failed validation: {error}"),
    };

    assert_eq!(loaded, state);
    assert_eq!(loaded.inventory().lot_ids(stockpile).count(), 1);
    let reencoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("loaded save reserialization unexpectedly failed: {error}"),
    };
    assert_eq!(reencoded, encoded);
}

#[test]
fn unsupported_schema_is_rejected_before_runtime_use() {
    let encoded = br#"{
            "schema_version": 999,
            "registry_schema_version": 5,
            "state": {
                "world_seed": 11,
                "clock": {"tick": 0},
                "random": {
                    "root_seed": 11,
                    "derivation": "SplitMix64V1",
                    "streams": {
                        "1": {
                            "algorithm": "Xoshiro256StarStarV1",
                            "words": [1, 2, 3, 4]
                        }
                    }
                },
                "systems": {
                    "energy": {"revision": 0, "next_store_id": 1, "records": {}},
                "fluid": {
                    "revision": 0,
                    "next_store_id": 1,
                    "records": {}
                },
                "equipment": {
                    "revision": 0,
                    "next_equipment_id": 1,
                    "records": {}
                },
                "structures": {
                    "revision": 0,
                    "next_element_id": 1,
                    "elements": {},
                    "supports_by_element": {}
                },
                "geology": {"revision": 0, "next_deposit_id": 1, "deposits": {}},
                "geological_knowledge": {
                    "revision": 0,
                    "next_observation_id": 1,
                    "observations": {}
                },
                "inventory": {
                    "revision": 0,
                    "next_stockpile_id": 1,
                    "next_lot_id": 1,
                    "stockpiles": {},
                    "lots": {}
                },
                "production": {
                    "revision": 0,
                    "next_job_id": 1,
                    "jobs": {}
                },
                "mining": {
                    "revision": 0,
                    "next_job_id": 1,
                    "jobs": {}
                },
                "player_work": {
                    "revision": 0,
                    "active": null
                },
                "survival": {
                    "revision": 0,
                    "player": null,
                    "consumed_matter": {},
                    "consumed_fluids": {}
                }
                }
            }
        }"#;
    let registries = build_registries();
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("fixture save failed to decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::UnsupportedSchemaVersion {
            found: 999,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        })
    );
}

#[test]
fn previous_save_schema_is_rejected_without_compatibility_path() {
    let registries = build_registries();
    let state = AppState::new(WorldSeed::new(0x5700_0022));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("previous-schema fixture serialization failed: {error}"));
    encoded["schema_version"] = serde_json::json!(52_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("previous-schema fixture decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::UnsupportedSchemaVersion {
            found: 52,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        })
    );
}

#[test]
fn unknown_fields_are_rejected_at_envelope_and_nested_state_boundaries() {
    let registries = build_registries();
    let state = AppState::new(WorldSeed::new(0x5700_0020));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("strict-field fixture serialization failed: {error}"));

    let mut envelope = base.clone();
    envelope["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(envelope).is_err());

    let mut nested = base;
    nested["state"]["systems"]["inventory"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(nested).is_err());
}

#[test]
fn structural_graph_damage_and_load_round_trip_exactly() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0001));
    let left = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let right = make_test_structural_element(&registries, &mut state, 2, 0, true);
    let deck = make_test_structural_element(&registries, &mut state, 1, 1, false);
    activate_test_structural_element(&registries, &mut state, left);
    activate_test_structural_element(&registries, &mut state, right);
    link_test_structural_support(&registries, &mut state, deck, left);
    link_test_structural_support(&registries, &mut state, deck, right);
    activate_test_structural_element(&registries, &mut state, deck);
    let load = match validate_set_structural_load(
        &registries,
        &state,
        deck,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(35_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("structural persistence load validation failed: {error}"),
    };
    commit_test_structural_mutation(load, &mut state);
    assert!(
        state
            .structures()
            .get_element(deck)
            .is_some_and(|record| record.is_cracked())
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("structural save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("structural save validation failed: {error}"),
    };

    assert_eq!(loaded, state);
    let loaded_supports: Vec<_> = match loaded.structures().supports(deck) {
        Some(supports) => supports.collect(),
        None => panic!("loaded deck lost structural support index"),
    };
    assert_eq!(loaded_supports, vec![left, right]);
    assert_eq!(
        loaded
            .structures()
            .get_element(deck)
            .map(|record| record.load(StructuralLoadKind::Snow)),
        Some(Force::from_millinewtons(35_000_000))
    );
    let analysis = match analyze_structure(
        registries.structural(),
        registries.materials(),
        loaded.structures(),
    ) {
        Ok(analysis) => analysis,
        Err(error) => panic!("loaded structure analysis failed: {error}"),
    };
    assert!(analysis.damage_events().is_empty());
}

#[test]
fn tampered_structural_embodied_mass_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0013));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural embodied-mass save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["embodied_mass"] =
        serde_json::json!(2_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural embodied-mass save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::EmbodiedMassMismatch {
                element: member,
                stored: Mass::from_milligrams(2),
                traced: Mass::from_milligrams(1),
            }
        )))
    );
}

#[test]
fn tampered_structural_length_cannot_change_required_embodied_mass() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0017));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural length save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["configuration"]
        ["geometry"]["length"] = serde_json::json!(2_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural length save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::EmbodiedMassGeometryMismatch {
                element: member,
                stored: Mass::from_milligrams(1),
                required: Mass::from_milligrams(2),
            }
        )))
    );
}

#[test]
fn tampered_structural_self_weight_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0014));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural self-weight save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["loads"]["SelfWeight"] =
        serde_json::json!(2_u128);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural self-weight save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SelfWeightMismatch {
                element: member,
                stored: Force::from_millinewtons(2),
                expected: Force::from_millinewtons(1),
            }
        )))
    );
}

#[test]
fn tampered_planned_structural_damage_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0015));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("planned structural damage save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["is_cracked"] =
        serde_json::json!(true);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered planned structural damage save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::PlannedElementCracked { element: member }
        )))
    );
}

#[test]
fn tampered_structural_cycle_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0003));
    let first = make_test_structural_element(&registries, &mut state, 0, 0, false);
    let second = make_test_structural_element(&registries, &mut state, 1, 0, false);
    link_test_structural_support(&registries, &mut state, first, second);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural cycle save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["supports_by_element"][second.value().to_string()] =
        serde_json::json!([first.value()]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural cycle save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SupportCycle {
                element: first,
                support: second,
            }
        )))
    );
}

#[test]
fn tampered_structural_support_across_empty_space_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0013));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, false);
    let nearby_support = make_test_structural_element(&registries, &mut state, 1, 0, false);
    let distant_support = make_test_structural_element(&registries, &mut state, 4, 0, false);
    link_test_structural_support(&registries, &mut state, member, nearby_support);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural contact save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["supports_by_element"][member.value().to_string()] =
        serde_json::json!([distant_support.value()]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural contact save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SupportOutOfContact {
                element: member,
                support: distant_support,
            }
        )))
    );
}

#[test]
fn save_with_unresolved_structural_overload_is_rejected() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0004));
    let column = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, column);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural overload save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][column.value().to_string()]["loads"]["Snow"] =
        serde_json::json!(50_000_000_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural overload save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::UnresolvedStructuralDamage {
                event: StructuralDamageEvent::Failed {
                    element: column,
                    cause: StructuralFailureCause::Overloaded {
                        carried_load: Force::from_millinewtons(50_000_001),
                        effective_capacity: Force::from_millinewtons(40_000_000),
                    },
                },
            }
        ))
    );
}

#[test]
fn current_save_rejects_registry_schema_mismatch() {
    let registries = build_registries();
    let state = AppState::new(WorldSeed::new(0x5700_0005));
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("registry mismatch save serialization failed: {error}"),
    };
    let mismatched = RegistrySchemaVersion::new(u32::MAX);
    encoded["registry_schema_version"] = serde_json::json!(mismatched.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("registry mismatch save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::RegistrySchemaMismatch {
            found: mismatched,
            supported: registries.schema_version(),
        })
    );
}

#[test]
fn supported_stockpile_round_trip_preserves_reverse_index_and_derived_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0021));
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("supported persistence stockpile failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000),
        Temperature::from_millikelvin(293_150),
    ) {
        panic!("supported persistence material failed: {error}");
    }
    let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
        Ok(mount) => mount,
        Err(error) => panic!("supported persistence mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("supported persistence mount commit failed: {error}");
    }
    let expected_load = state
        .structures()
        .get_element(support)
        .map(|record| record.load(StructuralLoadKind::StoredMatter));

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("supported stockpile save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("supported stockpile save decode failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("supported stockpile save validation failed: {error}"),
    };

    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert_eq!(
        loaded
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        expected_load
    );
}

#[test]
fn tampered_stored_matter_load_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0022));
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("support corruption stockpile failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000),
        Temperature::from_millikelvin(293_150),
    ) {
        panic!("support corruption material failed: {error}");
    }
    let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
        Ok(mount) => mount,
        Err(error) => panic!("support corruption mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("support corruption mount commit failed: {error}");
    }

    let mut wrong_load = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("stored-matter tamper serialization failed: {error}"),
    };
    wrong_load["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]
        ["StoredMatter"] = serde_json::json!(999_u128);
    let wrong_load: LoadedSaveEnvelope = match serde_json::from_value(wrong_load) {
        Ok(decoded) => decoded,
        Err(error) => panic!("stored-matter tamper failed decode: {error}"),
    };
    assert_eq!(
        wrong_load.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::StoredMatterStructuralLoadMismatch {
                element: support,
                stored: Force::from_millinewtons(999),
                expected: Force::from_millinewtons(10),
            }
        ))
    );
}

#[test]
fn energy_store_round_trip_preserves_definition_energy_and_revision() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new(WorldSeed::new(0xE900_0001));
    let store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(600_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("energy save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("energy save validation failed: {error}"),
    };

    let record = match loaded.energy().get_store(store) {
        Some(record) => record,
        None => panic!("energy store disappeared after round trip"),
    };
    assert_eq!(record.definition(), TEST_ENERGY_DEFINITION);
    assert_eq!(record.stored(), Energy::from_nanojoules(600_000));
    assert_eq!(loaded.energy().revision(), 1);
    assert_eq!(loaded, state);
}

#[test]
fn mounted_equipment_round_trip_preserves_support_and_derived_structural_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new(WorldSeed::new(0xE011_0010));
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        condition(575_000),
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("mounted equipment fixture creation failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("mounted equipment support validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("mounted equipment support commit failed: {error}");
    }
    let expected_load = state
        .structures()
        .get_element(support)
        .map(|record| record.load(StructuralLoadKind::Equipment));

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("mounted equipment save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("mounted equipment save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("mounted equipment save validation failed: {error}"),
    };

    assert_eq!(
        loaded
            .equipment()
            .get_equipment(equipment)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert_eq!(
        loaded
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        expected_load
    );
    assert_eq!(loaded, state);
}

#[test]
fn mounted_equipment_with_missing_support_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new(WorldSeed::new(0xE011_0011));
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("missing-support equipment fixture failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("missing-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("missing-support mount commit failed: {error}");
    }
    let missing = StructuralElementId::new(999_991);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("missing-support save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["supported_by"] =
        serde_json::json!(missing.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("missing-support tampered save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::UnknownEquipmentSupport {
                equipment,
                element: missing,
            }
        ))
    );
}

#[test]
fn tampered_equipment_structural_load_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new(WorldSeed::new(0xE011_0012));
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("load-tamper equipment fixture failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("load-tamper mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("load-tamper mount commit failed: {error}");
    }
    let expected = match state.structures().get_element(support) {
        Some(record) => record.load(StructuralLoadKind::Equipment),
        None => panic!("load-tamper support disappeared"),
    };
    let stored = Force::from_millinewtons(expected.millinewtons() - 1);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("load-tamper save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]["Equipment"] =
        serde_json::json!(stored.millinewtons());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("load-tamper save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::EquipmentStructuralLoadMismatch {
                element: support,
                stored,
                expected,
            }
        ))
    );
}

#[test]
fn energy_store_with_unknown_definition_is_rejected_on_load() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new(WorldSeed::new(0xE900_0002));
    let store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(100),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };
    let unknown = EnergyStoreDefinitionId::new(999_992);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["definition"] =
        serde_json::json!(unknown.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered energy save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::UnknownDefinition {
                store,
                definition: unknown,
            }
        )))
    );
}

#[test]
fn energy_store_above_authored_capacity_is_rejected_on_load() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new(WorldSeed::new(0xE900_0003));
    let store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(100),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["stored"] =
        serde_json::json!(1_000_001_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered energy capacity save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::CapacityExceeded {
                store,
                stored: Energy::from_nanojoules(1_000_001),
                capacity: Energy::from_nanojoules(1_000_000),
            }
        )))
    );
}

#[test]
fn equipment_round_trip_preserves_definition_condition_and_revision() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new(WorldSeed::new(0xE011_0001));
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        condition(575_000),
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment fixture creation failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("equipment save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("equipment save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("equipment save validation failed: {error}"),
    };

    let loaded_equipment = match loaded.equipment().get_equipment(equipment) {
        Some(record) => record,
        None => panic!("equipment disappeared after round trip"),
    };
    assert_eq!(loaded_equipment.definition(), TEST_EQUIPMENT_DEFINITION);
    assert_eq!(loaded_equipment.condition(), condition(575_000));
    assert_eq!(loaded.equipment().revision(), 1);
    assert_eq!(loaded, state);
}

#[test]
fn equipment_with_unknown_definition_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new(WorldSeed::new(0xE011_0002));
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment fixture creation failed: {error}"),
    };
    let unknown_definition = EquipmentDefinitionId::new(999_991);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("equipment save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["definition"] =
        serde_json::json!(unknown_definition.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered equipment save failed structural decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::UnknownDefinition {
                equipment,
                definition: unknown_definition,
            }
        )))
    );
}

#[test]
fn in_flight_sensible_heating_round_trip_preserves_energy_trace_and_continuation() {
    let registries = make_test_heating_registries();
    let mut state = AppState::new(WorldSeed::new(0xE900_0100));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence destination failed: {error}"),
    };
    let input_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("heating persistence material fixture failed: {error}"),
    };
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_HEATER_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence equipment failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        TEST_HEAT_ENERGY_DEFINITION,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence energy failed: {error}"),
    };
    let resolved = match resolve_sensible_heating_process(
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
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("heating persistence resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let expected_energy = resolved.process_resolution().energy_input();
    let expected_equipment = resolved.process_resolution().equipment_input();
    let expected_equipment_condition_after =
        resolved.process_resolution().equipment_condition_after();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("heating persistence start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("heating persistence start commit failed: {error}"),
    };
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        expected_energy
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider()),
        expected_equipment
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        expected_equipment_condition_after
    );

    let mut tampered = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating provenance tamper serialization failed: {error}"),
    };
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_energy"]
        ["carrier"] = serde_json::json!("Thermal");
    let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating provenance tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEnergyCarrierMismatch {
                job,
                traced: EnergyCarrier::Thermal,
                authored: EnergyCarrier::Electrical,
            }
        ))
    );

    let mut tampered_condition_outcome =
        match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("heating wear tamper serialization failed: {error}"),
        };
    tampered_condition_outcome["state"]["systems"]["production"]["jobs"][job.value().to_string()]
        ["equipment"]["condition_after"] = serde_json::json!(999_999_u32);
    let tampered_condition_outcome: LoadedSaveEnvelope =
        match serde_json::from_value(tampered_condition_outcome) {
            Ok(decoded) => decoded,
            Err(error) => panic!("heating wear tamper failed decode: {error}"),
        };
    assert_eq!(
        tampered_condition_outcome.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::EquipmentConditionOutcomeMismatch {
                job,
                stored: condition(999_999),
                required: condition(999_000),
            }
        )))
    );

    let mut tampered_energy = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating energy tamper serialization failed: {error}"),
    };
    tampered_energy["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_energy"]["energy"] = serde_json::json!(1_u64);
    let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating energy tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_energy.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::EnergyMismatch {
                job,
                traced: Energy::from_nanojoules(1),
                required: Energy::from_nanojoules(51_000_000),
            }
        )))
    );

    let mut tampered_duration = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating duration tamper serialization failed: {error}"),
    };
    tampered_duration["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]
        ["active_duration"] = serde_json::json!(duration.value() + 1);
    let tampered_duration: LoadedSaveEnvelope = match serde_json::from_value(tampered_duration) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating duration tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_duration.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::DurationMismatch {
                job,
                stored: crate::core::time::TickSpan::new(duration.value() + 1),
                required: duration,
            }
        )))
    );

    let mut tampered_output = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating output tamper serialization failed: {error}"),
    };
    tampered_output["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["commodity"] =
        serde_json::json!(CommodityKey::new(MATERIAL_WOOD, FORM_LUMP).value());
    let tampered_output: LoadedSaveEnvelope = match serde_json::from_value(tampered_output) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating output tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_output.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::OutputMismatch { job }
        )))
    );

    let mut tampered_equipment = match serde_json::to_value(SaveEnvelope::new(&registries, &state))
    {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating equipment tamper serialization failed: {error}"),
    };
    tampered_equipment["state"]["systems"]["production"]["jobs"][job.value().to_string()]["equipment"]
        ["provider"]["condition"] = serde_json::json!(999_999_u32);
    let tampered_equipment: LoadedSaveEnvelope = match serde_json::from_value(tampered_equipment) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating equipment tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_equipment.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentConditionMismatch {
                job,
                traced: condition(999_999),
                stored: Condition::PRISTINE,
            }
        ))
    );

    let mut double_booked = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating double-book tamper serialization failed: {error}"),
    };
    let second_job = job.value() + 1;
    let mut duplicated =
        double_booked["state"]["systems"]["production"]["jobs"][job.value().to_string()].clone();
    duplicated["identity"]["id"] = serde_json::json!(second_job);
    double_booked["state"]["systems"]["production"]["jobs"][second_job.to_string()] = duplicated;
    double_booked["state"]["systems"]["production"]["next_job_id"] =
        serde_json::json!(second_job + 1);
    let double_booked: LoadedSaveEnvelope = match serde_json::from_value(double_booked) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating double-book tamper failed decode: {error}"),
    };
    assert_eq!(
        double_booked.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::EnergyDoubleBooked {
                store: energy_store
            }
        )))
    );

    let mut equipment_double_booked =
        match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("equipment double-book tamper serialization failed: {error}"),
        };
    let second_equipment_job = job.value() + 1;
    let mut duplicated_equipment =
        equipment_double_booked["state"]["systems"]["production"]["jobs"][job.value().to_string()]
            .clone();
    duplicated_equipment["identity"]["id"] = serde_json::json!(second_equipment_job);
    duplicated_equipment["resources"]["consumed_energy"] = serde_json::Value::Null;
    equipment_double_booked["state"]["systems"]["production"]["jobs"]
        [second_equipment_job.to_string()] = duplicated_equipment;
    equipment_double_booked["state"]["systems"]["production"]["next_job_id"] =
        serde_json::json!(second_equipment_job + 1);
    let equipment_double_booked: LoadedSaveEnvelope =
        match serde_json::from_value(equipment_double_booked) {
            Ok(decoded) => decoded,
            Err(error) => panic!("equipment double-book tamper failed decode: {error}"),
        };
    assert_eq!(
        equipment_double_booked.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::EquipmentDoubleBooked { equipment }
        )))
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating in-flight save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating in-flight save deserialization failed: {error}"),
    };
    let mut resumed = match decoded.into_state(&registries) {
        Ok(state) => state,
        Err(error) => panic!("heating in-flight save validation failed: {error}"),
    };
    let mut uninterrupted = state.clone();
    assert_eq!(resumed, uninterrupted);
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        expected_energy
    );
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider()),
        expected_equipment
    );
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        expected_equipment_condition_after
    );

    for _ in 0..duration.value() {
        let uninterrupted_outcome = match advance_tick(&registries, &mut uninterrupted) {
            Ok(outcome) => outcome,
            Err(error) => panic!("uninterrupted heating continuation failed: {error}"),
        };
        let resumed_outcome = match advance_tick(&registries, &mut resumed) {
            Ok(outcome) => outcome,
            Err(error) => panic!("resumed heating continuation failed: {error}"),
        };
        assert_eq!(resumed_outcome, uninterrupted_outcome);
    }
    assert_eq!(resumed, uninterrupted);
    assert!(resumed.production().get_job(job).is_none());
    let output = match resumed
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination)
    {
        Some(lot) => lot,
        None => panic!("resumed heating output disappeared"),
    };
    assert_eq!(output.temperature(), Temperature::from_millikelvin(303_000));
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(
        resumed
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        expected_equipment_condition_after
    );
}

#[test]
fn in_flight_job_round_trip_preserves_deterministic_continuation() {
    let registries = make_test_registries_with_process(make_test_process());
    let mut state = AppState::new(WorldSeed::new(0xC011_71A0));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let resolution = make_test_resolution(&registries, &state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("fixture process failed validation: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("fixture process commit failed: {error}");
    }
    for _ in 0..2 {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("fixture tick failed: {error}");
        }
    }
    let mut uninterrupted = state.clone();

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("in-flight save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("in-flight save deserialization failed: {error}"),
    };
    let mut resumed = match decoded.into_state(&registries) {
        Ok(state) => state,
        Err(error) => panic!("in-flight save validation failed: {error}"),
    };

    for _ in 0..8 {
        let uninterrupted_outcome = match advance_tick(&registries, &mut uninterrupted) {
            Ok(outcome) => outcome,
            Err(error) => panic!("uninterrupted continuation failed: {error}"),
        };
        let resumed_outcome = match advance_tick(&registries, &mut resumed) {
            Ok(outcome) => outcome,
            Err(error) => panic!("resumed continuation failed: {error}"),
        };
        assert_eq!(resumed_outcome, uninterrupted_outcome);
    }

    assert_eq!(resumed, uninterrupted);
}

#[test]
fn in_flight_job_survives_later_process_requirement_rebalance() {
    let original_registries = make_test_registries_with_process(make_test_process());
    let mut state = AppState::new(WorldSeed::new(0xBA1A_0CE0));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &original_registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let resolution = make_test_resolution(&original_registries, &state, source);
    let token = match validate_start_process(
        &original_registries,
        &state,
        &resolution,
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("fixture process failed validation: {error}"),
    };
    if let Err(error) = token.commit(&mut state) {
        panic!("fixture process commit failed: {error}");
    }

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&original_registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("in-flight save serialization failed: {error}"),
    };
    let changed_registries =
        make_test_registries_with_process(make_test_process_with_input_mass(20));
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("in-flight save deserialization failed: {error}"),
    };
    let mut loaded = match decoded.into_state(&changed_registries) {
        Ok(state) => state,
        Err(error) => panic!("rebalanced in-flight save failed validation: {error}"),
    };
    for _ in 0..5 {
        if let Err(error) = advance_tick(&changed_registries, &mut loaded) {
            panic!("rebalanced in-flight continuation failed: {error}");
        }
    }
    let charcoal = CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP);
    let destination_record = match loaded.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared after completion"),
    };
    assert_eq!(
        destination_record.get_mass(charcoal),
        Mass::from_milligrams(10)
    );
}

#[test]
fn tampered_in_flight_consumed_mass_is_rejected_on_load() {
    let registries = make_test_registries_with_process(make_test_process());
    let mut state = AppState::new(WorldSeed::new(0xC0DE_0009));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(id) => id,
        Err(error) => panic!("fixture source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(id) => id,
        Err(error) => panic!("fixture destination failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    let resolution = make_test_resolution(&registries, &state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("fixture process failed validation: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("fixture process commit failed: {error}"),
    };
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_mass"] =
        serde_json::json!(9);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered save failed structural decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::ConsumedInputMassMismatch {
                job: ProductionJobId::new(job.value()),
                traced: Mass::from_milligrams(10),
                consumed: Mass::from_milligrams(9),
            }
        )))
    );
}

#[test]
fn tampered_random_root_seed_is_rejected_on_load() {
    let registries = build_registries();
    let state = AppState::new(WorldSeed::new(0x5151));
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("save serialization failed: {error}"),
    };
    encoded["state"]["random"]["root_seed"] = serde_json::json!(0x5152_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered save failed structural decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::RandomWorldSeedMismatch {
                world_seed: WorldSeed::new(0x5151),
                random_seed: WorldSeed::new(0x5152),
            }
        ))
    );
}

#[test]
fn mixed_composition_round_trip_preserves_constituents_exactly() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC0A1_1051));
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 720_000),
        CompositionComponent::new(MATERIAL_SLAG, 280_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    };
    let lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(25),
        Temperature::from_millikelvin(325_000),
        composition.clone(),
    ) {
        Ok(id) => id,
        Err(error) => panic!("composed lot fixture failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("mixed save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("mixed save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(state) => state,
        Err(error) => panic!("mixed save validation failed: {error}"),
    };

    let loaded_lot = match loaded.inventory().get_lot(lot) {
        Some(lot) => lot,
        None => panic!("mixed lot disappeared after round trip"),
    };
    assert_eq!(loaded_lot.composition(), &composition);
    assert_eq!(loaded, state);
}

#[test]
fn unknown_lot_composition_constituent_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xBAD0_C0DE));
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let unknown = MaterialId::new(999_999);
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(295_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("lot fixture failed: {error}"),
    };

    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("serialization failed: {error}"),
    };
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["profile"]["composition"]
        ["components"] = serde_json::json!([
        {"material": MATERIAL_WOOD.value(), "parts_per_million": 900_000_u32},
        {"material": unknown.value(), "parts_per_million": 100_000_u32},
    ]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("deserialization failed: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            crate::inventory::InventoryValidationError::InvalidLotPhaseState {
                lot,
                error: crate::material::MaterialPhaseStateError::UnknownMaterial {
                    material: unknown,
                },
            }
        )))
    );
}
