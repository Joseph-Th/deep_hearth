//! Immutable maintainable-equipment definitions; sibling state stores only persistent references and changing condition.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::capability::{CapabilityProfile, CapabilityRegistry};
use crate::core::quantity::Mass;
use crate::maintenance::MaintenanceThresholds;

/// Stable authored identifier for one equipment definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EquipmentDefinitionId(u32);

impl EquipmentDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "equipment definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Immutable authored properties shared by all runtime instances of one equipment class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentDefinition {
    id: EquipmentDefinitionId,
    name: String,
    mass: Mass,
    capabilities: CapabilityProfile,
    maintenance_thresholds: MaintenanceThresholds,
}

impl EquipmentDefinition {
    #[must_use]
    pub fn new(
        id: EquipmentDefinitionId,
        name: impl Into<String>,
        mass: Mass,
        capabilities: CapabilityProfile,
        maintenance_thresholds: MaintenanceThresholds,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "equipment definition name must not be empty"
        );
        assert!(!mass.is_zero(), "equipment definition mass must be nonzero");
        Self {
            id,
            name,
            mass,
            capabilities,
            maintenance_thresholds,
        }
    }

    #[must_use]
    pub const fn id(&self) -> EquipmentDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityProfile {
        &self.capabilities
    }

    #[must_use]
    pub const fn maintenance_thresholds(&self) -> MaintenanceThresholds {
        self.maintenance_thresholds
    }
}

/// Immutable deterministic authored equipment lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentRegistry {
    definitions: BTreeMap<EquipmentDefinitionId, EquipmentDefinition>,
}

impl EquipmentRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = EquipmentDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate equipment definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_equipment(&self, id: EquipmentDefinitionId) -> Option<&EquipmentDefinition> {
        self.definitions.get(&id)
    }

    pub(crate) fn validate_references(&self, capabilities: &CapabilityRegistry) {
        for definition in self.definitions.values() {
            for (capability, value) in definition.capabilities().entries() {
                let Some(capability_definition) = capabilities.get_capability(capability) else {
                    panic!(
                        "equipment definition {} references missing capability {}",
                        definition.id().value(),
                        capability.value()
                    );
                };
                assert_eq!(
                    value.kind(),
                    capability_definition.kind(),
                    "equipment definition {} capability {} has wrong physical value kind",
                    definition.id().value(),
                    capability.value()
                );
            }
        }
    }
}
