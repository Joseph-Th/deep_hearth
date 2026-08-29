//! Immutable material-backed storage-enclosure definitions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::material::{MaterialAssemblyProfile, MaterialRegistry};

use super::{AMBIENT_PRESERVATION_MULTIPLIER_PPM, StockpileStorageProfile};

/// Stable authored identity for one constructible stockpile enclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StorageDefinitionId(u32);

impl StorageDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "storage definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Authored physical enclosure that can improve one existing stockpile's storage environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageDefinition {
    id: StorageDefinitionId,
    name: &'static str,
    maximum_stockpile_capacity: Mass,
    storage_profile: StockpileStorageProfile,
    assembly_profile: MaterialAssemblyProfile,
}

impl StorageDefinition {
    #[must_use]
    pub fn new(
        id: StorageDefinitionId,
        name: &'static str,
        maximum_stockpile_capacity: Mass,
        storage_profile: StockpileStorageProfile,
        assembly_profile: MaterialAssemblyProfile,
    ) -> Self {
        assert!(
            !maximum_stockpile_capacity.is_zero(),
            "storage definition maximum capacity must be nonzero"
        );
        storage_profile
            .validate()
            .unwrap_or_else(|error| panic!("storage definition has invalid profile: {error}"));
        assert!(
            storage_profile.preservation_multiplier_ppm() > AMBIENT_PRESERVATION_MULTIPLIER_PPM,
            "constructible preservation storage must improve on ambient shelf life"
        );
        assert!(
            !assembly_profile.input_mass().is_zero(),
            "constructible storage must embody nonzero construction matter"
        );
        Self {
            id,
            name,
            maximum_stockpile_capacity,
            storage_profile,
            assembly_profile,
        }
    }

    #[must_use]
    pub const fn id(&self) -> StorageDefinitionId {
        self.id
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn maximum_stockpile_capacity(&self) -> Mass {
        self.maximum_stockpile_capacity
    }

    #[must_use]
    pub const fn storage_profile(&self) -> StockpileStorageProfile {
        self.storage_profile
    }

    #[must_use]
    pub const fn assembly_profile(&self) -> &MaterialAssemblyProfile {
        &self.assembly_profile
    }
}

/// Immutable authored storage definitions keyed by stable identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRegistry {
    definitions: BTreeMap<StorageDefinitionId, StorageDefinition>,
}

impl StorageRegistry {
    #[must_use]
    pub fn new(definitions: impl IntoIterator<Item = StorageDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate storage definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get(&self, id: StorageDefinitionId) -> Option<&StorageDefinition> {
        self.definitions.get(&id)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &StorageDefinition> {
        self.definitions.values()
    }

    pub(crate) fn validate_references(&self, materials: &MaterialRegistry) {
        for definition in self.definitions.values() {
            definition
                .assembly_profile()
                .validate_infrastructure_references(materials)
                .unwrap_or_else(|error| {
                    panic!(
                        "storage definition {} has invalid construction material: {error:?}",
                        definition.id().value()
                    )
                });
        }
    }
}
