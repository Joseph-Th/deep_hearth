//! Versioned persistence envelope and decoded-state validation, independent of storage and encoding.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::registry::{Registries, RegistrySchemaVersion};

/// Save schema currently emitted and accepted by this build.
pub const CURRENT_SAVE_SCHEMA_VERSION: u32 = 13;

/// Minimal version metadata that adapters decode before choosing a concrete save payload decoder.
///
/// Serde ignores unknown fields by default, so this type can be decoded from a full envelope even
/// when that envelope's `state` field belongs to an older schema that the current `AppState` can no
/// longer deserialize directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct SaveMetadata {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
}

impl SaveMetadata {
    #[must_use]
    pub const fn schema_version(self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn registry_schema_version(self) -> RegistrySchemaVersion {
        self.registry_schema_version
    }

    /// Checks whether this metadata can be decoded directly by the current payload type.
    pub fn validate_current(self, registries: &Registries) -> Result<(), LoadError> {
        validate_versions(
            self.schema_version,
            self.registry_schema_version,
            registries,
        )
    }
}

/// Borrowed versioned save payload suitable for any Serde encoding adapter.
#[derive(Debug, Serialize)]
pub struct SaveEnvelope<'state> {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    state: &'state AppState,
}

impl<'state> SaveEnvelope<'state> {
    /// Wraps current state in the current semantic save schema.
    #[must_use]
    pub const fn new(registries: &Registries, state: &'state AppState) -> Self {
        Self {
            schema_version: CURRENT_SAVE_SCHEMA_VERSION,
            registry_schema_version: registries.schema_version(),
            state,
        }
    }
}

/// Owned decoded save payload; callers must validate it with `into_state` before runtime use.
#[derive(Debug, Deserialize)]
pub struct LoadedSaveEnvelope {
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    state: AppState,
}

impl LoadedSaveEnvelope {
    /// Validates schema compatibility and persistent invariants before returning runtime state.
    pub fn into_state(self, registries: &Registries) -> Result<AppState, LoadError> {
        let Self {
            schema_version,
            registry_schema_version,
            state,
        } = self;

        validate_versions(schema_version, registry_schema_version, registries)?;

        validate_loaded_state(registries, &state).map_err(LoadError::InvalidState)?;
        Ok(state)
    }
}

fn validate_versions(
    schema_version: u32,
    registry_schema_version: RegistrySchemaVersion,
    registries: &Registries,
) -> Result<(), LoadError> {
    if schema_version != CURRENT_SAVE_SCHEMA_VERSION {
        return Err(LoadError::UnsupportedSchemaVersion {
            found: schema_version,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        });
    }
    if registry_schema_version != registries.schema_version() {
        return Err(LoadError::RegistrySchemaMismatch {
            found: registry_schema_version,
            supported: registries.schema_version(),
        });
    }
    Ok(())
}

/// Semantic persistence failure after bytes have already been decoded by an adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The save requires an explicit migration or a different build.
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    /// Stable authored registry identities are not compatible with this build.
    RegistrySchemaMismatch {
        found: RegistrySchemaVersion,
        supported: RegistrySchemaVersion,
    },
    /// The decoded runtime data violates a persistent state invariant.
    InvalidState(StateValidationError),
}

impl Display for LoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                formatter,
                "unsupported save schema version {found}; this build supports {supported}"
            ),
            Self::RegistrySchemaMismatch { found, supported } => write!(
                formatter,
                "save registry schema {} is incompatible with this build's schema {}",
                found.value(),
                supported.value()
            ),
            Self::InvalidState(error) => write!(formatter, "invalid persisted state: {error}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsupportedSchemaVersion { .. } | Self::RegistrySchemaMismatch { .. } => None,
            Self::InvalidState(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
        CapabilityRequirement, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::{
        FORM_LOG, FORM_LUMP, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG,
        MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
        make_test_registries_with_energy_store, make_test_registries_with_equipment,
        make_test_registries_with_process, make_test_registries_with_sensible_heating,
    };
    use crate::core::quantity::{Area, Energy, Force, Mass, Power, Temperature};
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId, EnergyValidationError,
        add_energy_store_with_initial_for_test,
    };
    use crate::equipment::{
        EquipmentDefinition, EquipmentDefinitionId, EquipmentValidationError, add_equipment,
    };
    use crate::inventory::{
        MaterialLotSelection, add_stockpile, deposit_bulk_for_test, deposit_composed_lot_for_test,
        deposit_lot_for_test,
    };
    use crate::maintenance::{Condition, MaintenanceThresholds};
    use crate::material::{
        CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialInputSpec,
        MaterialLotSpec,
    };
    use crate::production::{
        ProcessDefinition, ProcessId, ProcessResolution, ProductionJobId,
        ProductionValidationError, make_test_process_resolution, validate_process_inputs,
        validate_start_process,
    };
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralDamageEvent, StructuralElementId, StructuralFailureCause, StructuralLoadKind,
        StructuralMutationOutcome, StructureValidationError, ValidatedStructuralMutation,
        add_structural_element, analyze_structure, validate_activate_structural_element,
        validate_link_support, validate_set_structural_load,
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
    const TEST_HEAT_ENERGY_DEFINITION: EnergyStoreDefinitionId =
        EnergyStoreDefinitionId::new(900_501);
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
            ),
        )
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
        grounded: bool,
    ) -> StructuralElementId {
        match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            make_test_structural_bounds(x, y),
            Area::from_square_millimeters(1_000),
            grounded,
        ) {
            Ok(element) => element,
            Err(error) => panic!("structural persistence element fixture failed: {error}"),
        }
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
        let stockpile = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
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
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("save deserialization unexpectedly failed: {error}"),
        };
        let loaded = match decoded.into_state(&registries) {
            Ok(loaded) => loaded,
            Err(error) => panic!("decoded save unexpectedly failed validation: {error}"),
        };

        assert_eq!(loaded, state);
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
            "registry_schema_version": 4,
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
                "energy": {"revision": 0, "next_store_id": 1, "records": {}},
                "equipment": {"revision": 0, "next_equipment_id": 1, "records": {}},
                "structures": {
                    "revision": 0,
                    "next_element_id": 1,
                    "elements": {},
                    "supports_by_element": {},
                    "dependents_by_support": {}
                },
                "inventory": {
                    "revision": 0,
                    "next_stockpile_id": 1,
                    "next_lot_id": 1,
                    "stockpiles": {},
                    "lots": {}
                },
                "production": {"revision": 0, "next_job_id": 1, "jobs": {}, "due_jobs": {}}
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
    fn tampered_structural_reverse_index_is_rejected_on_load() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5700_0002));
        let foundation = make_test_structural_element(&registries, &mut state, 0, 0, true);
        let upper = make_test_structural_element(&registries, &mut state, 0, 1, false);
        link_test_structural_support(&registries, &mut state, upper, foundation);
        let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("structural reverse-index save serialization failed: {error}"),
        };
        encoded["state"]["structures"]["dependents_by_support"][foundation.value().to_string()] =
            serde_json::json!([]);
        let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("tampered structural reverse-index save failed decode: {error}"),
        };

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Structure(
                StructureValidationError::ReverseIndexMismatch {
                    element: upper,
                    support: foundation,
                }
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
        encoded["state"]["structures"]["supports_by_element"][second.value().to_string()] =
            serde_json::json!([first.value()]);
        encoded["state"]["structures"]["dependents_by_support"][first.value().to_string()] =
            serde_json::json!([second.value()]);
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
    fn save_with_unresolved_structural_overload_is_rejected() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5700_0004));
        let column = make_test_structural_element(&registries, &mut state, 0, 0, true);
        activate_test_structural_element(&registries, &mut state, column);
        let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("structural overload save serialization failed: {error}"),
        };
        encoded["state"]["structures"]["elements"][column.value().to_string()]["loads"]["Equipment"] =
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
                            carried_load: Force::from_millinewtons(50_000_000),
                            effective_capacity: Force::from_millinewtons(40_000_000),
                        },
                    },
                }
            ))
        );
    }

    #[test]
    fn current_save_rejects_prior_registry_schema_after_energy_and_thermal_ids_are_added() {
        let registries = build_registries();
        let state = AppState::new(WorldSeed::new(0x5700_0005));
        let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("registry compatibility save serialization failed: {error}"),
        };
        encoded["registry_schema_version"] = serde_json::json!(3_u32);
        let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("registry compatibility save failed decode: {error}"),
        };

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::RegistrySchemaMismatch {
                found: RegistrySchemaVersion::new(3),
                supported: RegistrySchemaVersion::new(4),
            })
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
        encoded["state"]["energy"]["records"][store.value().to_string()]["definition"] =
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
        encoded["state"]["energy"]["records"][store.value().to_string()]["stored"] =
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
        encoded["state"]["equipment"]["records"][equipment.value().to_string()]["definition"] =
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
    fn metadata_preflight_identifies_legacy_payload_without_decoding_current_state() {
        let encoded = br#"{
            "schema_version": 3,
            "registry_schema_version": 1,
            "state": "intentionally not the current AppState shape"
        }"#;
        let metadata: SaveMetadata = match serde_json::from_slice(encoded) {
            Ok(metadata) => metadata,
            Err(error) => panic!("metadata preflight failed to decode: {error}"),
        };
        let registries = build_registries();

        assert_eq!(metadata.schema_version(), 3);
        assert_eq!(metadata.registry_schema_version().value(), 1);
        assert_eq!(
            metadata.validate_current(&registries),
            Err(LoadError::UnsupportedSchemaVersion {
                found: 3,
                supported: CURRENT_SAVE_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn in_flight_sensible_heating_round_trip_preserves_energy_trace_and_continuation() {
        let registries = make_test_heating_registries();
        let mut state = AppState::new(WorldSeed::new(0xE900_0100));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("heating persistence source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
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

        let mut tampered = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("heating provenance tamper serialization failed: {error}"),
        };
        tampered["state"]["production"]["jobs"][job.value().to_string()]["consumed_energy"]["carrier"] =
            serde_json::json!("Thermal");
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

        let mut tampered_energy = match serde_json::to_value(SaveEnvelope::new(&registries, &state))
        {
            Ok(encoded) => encoded,
            Err(error) => panic!("heating energy tamper serialization failed: {error}"),
        };
        tampered_energy["state"]["production"]["jobs"][job.value().to_string()]["consumed_energy"]
            ["energy"] = serde_json::json!(1_u64);
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

        let mut tampered_duration =
            match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("heating duration tamper serialization failed: {error}"),
            };
        let original_due = match state.production().get_job(job) {
            Some(record) => record.completes_at().value(),
            None => panic!("heating job disappeared before duration tamper"),
        };
        let tampered_due = original_due + 1;
        tampered_duration["state"]["production"]["jobs"][job.value().to_string()]["completes_at"] =
            serde_json::json!(tampered_due);
        let due_jobs = match tampered_duration["state"]["production"]["due_jobs"].as_object_mut() {
            Some(due_jobs) => due_jobs,
            None => panic!("heating duration tamper due-job index is not an object"),
        };
        due_jobs.remove(&original_due.to_string());
        due_jobs.insert(tampered_due.to_string(), serde_json::json!([job.value()]));
        let tampered_duration: LoadedSaveEnvelope = match serde_json::from_value(tampered_duration)
        {
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

        let mut tampered_output = match serde_json::to_value(SaveEnvelope::new(&registries, &state))
        {
            Ok(encoded) => encoded,
            Err(error) => panic!("heating output tamper serialization failed: {error}"),
        };
        tampered_output["state"]["production"]["jobs"][job.value().to_string()]["outputs"][0]["commodity"] =
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

        let mut tampered_equipment =
            match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("heating equipment tamper serialization failed: {error}"),
            };
        tampered_equipment["state"]["production"]["jobs"][job.value().to_string()]["equipment_provider"]
            ["condition"] = serde_json::json!(999_999_u32);
        let tampered_equipment: LoadedSaveEnvelope =
            match serde_json::from_value(tampered_equipment) {
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
            double_booked["state"]["production"]["jobs"][job.value().to_string()].clone();
        duplicated["id"] = serde_json::json!(second_job);
        double_booked["state"]["production"]["jobs"][second_job.to_string()] = duplicated;
        double_booked["state"]["production"]["next_job_id"] = serde_json::json!(second_job + 1);
        let due = match state.production().get_job(job) {
            Some(record) => record.completes_at().value().to_string(),
            None => panic!("heating job disappeared before double-book tamper"),
        };
        double_booked["state"]["production"]["due_jobs"][due.clone()] =
            serde_json::json!([job.value(), second_job]);
        let double_booked: LoadedSaveEnvelope = match serde_json::from_value(double_booked) {
            Ok(decoded) => decoded,
            Err(error) => panic!("heating double-book tamper failed decode: {error}"),
        };
        assert_eq!(
            double_booked.into_state(&registries),
            Err(LoadError::InvalidState(
                StateValidationError::EnergyStoreDoubleBooked {
                    store: energy_store,
                    first: job,
                    second: ProductionJobId::new(second_job),
                }
            ))
        );

        let mut equipment_double_booked =
            match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
                Ok(encoded) => encoded,
                Err(error) => panic!("equipment double-book tamper serialization failed: {error}"),
            };
        let second_equipment_job = job.value() + 1;
        let mut duplicated_equipment =
            equipment_double_booked["state"]["production"]["jobs"][job.value().to_string()].clone();
        duplicated_equipment["id"] = serde_json::json!(second_equipment_job);
        duplicated_equipment["consumed_energy"] = serde_json::Value::Null;
        equipment_double_booked["state"]["production"]["jobs"][second_equipment_job.to_string()] =
            duplicated_equipment;
        equipment_double_booked["state"]["production"]["next_job_id"] =
            serde_json::json!(second_equipment_job + 1);
        equipment_double_booked["state"]["production"]["due_jobs"][due] =
            serde_json::json!([job.value(), second_equipment_job]);
        let equipment_double_booked: LoadedSaveEnvelope =
            match serde_json::from_value(equipment_double_booked) {
                Ok(decoded) => decoded,
                Err(error) => panic!("equipment double-book tamper failed decode: {error}"),
            };
        assert_eq!(
            equipment_double_booked.into_state(&registries),
            Err(LoadError::InvalidState(
                StateValidationError::EquipmentDoubleBooked {
                    equipment,
                    first: job,
                    second: ProductionJobId::new(second_equipment_job),
                }
            ))
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
    }

    #[test]
    fn in_flight_job_round_trip_preserves_deterministic_continuation() {
        let registries = make_test_registries_with_process(make_test_process());
        let mut state = AppState::new(WorldSeed::new(0xC011_71A0));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
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
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
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
        let source = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
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
        let source = match add_stockpile(&mut state, Mass::from_milligrams(10)) {
            Ok(id) => id,
            Err(error) => panic!("fixture source failed: {error}"),
        };
        let destination = match add_stockpile(&mut state, Mass::from_milligrams(20)) {
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
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
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
        encoded["state"]["production"]["jobs"][job.value().to_string()]["consumed_mass"] =
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
        let stockpile = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
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
        let stockpile = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        };
        let unknown = MaterialId::new(999_999);
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_WOOD, 900_000),
            CompositionComponent::new(unknown, 100_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition fixture failed: {error}"),
        };
        let lot = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(295_000),
            composition,
        ) {
            Ok(id) => id,
            Err(error) => panic!("lot fixture failed: {error}"),
        };

        let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("serialization failed: {error}"),
        };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("deserialization failed: {error}"),
        };

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(
                StateValidationError::UnknownLotCompositionMaterial {
                    lot,
                    material: unknown,
                }
            ))
        );
    }
}
