//! Versioned persistence envelope and decoded-state validation, independent of storage and encoding.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::registry::{Registries, RegistrySchemaVersion};

/// Save schema currently emitted and accepted by this build.
pub const CURRENT_SAVE_SCHEMA_VERSION: u32 = 9;

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
    use crate::content::{
        FORM_LOG, FORM_LUMP, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG,
        MATERIAL_WOOD, build_registries, make_test_registries_with_process,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::time::WorldSeed;
    use crate::inventory::{add_stockpile, deposit_bulk_for_test, deposit_composed_lot_for_test};
    use crate::material::{
        CommodityKey, CompositionComponent, MaterialComposition, MaterialId, MaterialInputSpec,
        MaterialLotSpec,
    };
    use crate::production::{
        ProcessDefinition, ProcessId, ProcessResolution, ProductionJobId,
        ProductionValidationError, make_test_process_resolution, validate_start_process,
    };
    use crate::simulation::advance_tick;

    const TEST_PROCESS: ProcessId = ProcessId::new(900_101);

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

    fn make_test_resolution() -> ProcessResolution {
        make_test_process_resolution(
            TEST_PROCESS,
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
            "registry_schema_version": 1,
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
    fn in_flight_job_round_trip_preserves_deterministic_continuation() {
        let registries = make_test_registries_with_process(make_test_process());
        let resolution = make_test_resolution();
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
        let resolution = make_test_resolution();
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
        let resolution = make_test_resolution();
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
