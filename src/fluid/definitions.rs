//! Immutable fluid identities; sibling fluid state stores only generated runtime ownership.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::material::{MaterialId, MaterialRegistry};

/// Stable authored identity for one fluid class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FluidDefinitionId(u32);

impl FluidDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "fluid definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Immutable authored identity tying a fluid class to its underlying material.
///
/// Runtime mixtures and contamination are intentionally not authored here. They require a future
/// composition-owning fluid model rather than silently turning one authored fluid into another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FluidDefinition {
    id: FluidDefinitionId,
    name: String,
    material: MaterialId,
    density_kg_per_m3: u32,
}

impl FluidDefinition {
    #[must_use]
    pub fn new(
        id: FluidDefinitionId,
        name: impl Into<String>,
        material: MaterialId,
        density_kg_per_m3: u32,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "fluid definition name must not be empty"
        );
        assert!(
            density_kg_per_m3 > 0,
            "fluid definition density must be nonzero"
        );
        Self {
            id,
            name,
            material,
            density_kg_per_m3,
        }
    }

    #[must_use]
    pub const fn id(&self) -> FluidDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn density_kg_per_m3(&self) -> u32 {
        self.density_kg_per_m3
    }
}

/// Immutable deterministic fluid-definition lookup table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluidRegistry {
    definitions: BTreeMap<FluidDefinitionId, FluidDefinition>,
}

impl FluidRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = FluidDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate fluid definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_fluid(&self, id: FluidDefinitionId) -> Option<&FluidDefinition> {
        self.definitions.get(&id)
    }

    pub(crate) fn validate_references(&self, materials: &MaterialRegistry) {
        for definition in self.definitions.values() {
            assert!(
                materials.get_material(definition.material()).is_some(),
                "fluid definition {} references missing material {}",
                definition.id().value(),
                definition.material().value()
            );
        }
    }
}
