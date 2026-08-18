//! Immutable definitions for finite energy stores; runtime state owns only changing stored energy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::material::{MaterialAssemblyProfile, MaterialRegistry};

/// Stable authored identity for one energy-store class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EnergyStoreDefinitionId(u32);

impl EnergyStoreDefinitionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "energy store definition id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Explicit carrier represented by a finite energy store.
///
/// Chemical energy is intentionally absent: fuels remain conserved material and must be resolved
/// through combustion/chemistry rather than becoming an abstract energy balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnergyCarrier {
    Electrical,
    Thermal,
    Mechanical,
}

/// Immutable authored capacity and directional transfer envelopes for one energy-store class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreDefinition {
    id: EnergyStoreDefinitionId,
    name: String,
    carrier: EnergyCarrier,
    capacity: Energy,
    max_input_power: Power,
    max_output_power: Power,
    assembly_profile: Option<MaterialAssemblyProfile>,
}

impl EnergyStoreDefinition {
    #[must_use]
    pub fn new(
        id: EnergyStoreDefinitionId,
        name: impl Into<String>,
        carrier: EnergyCarrier,
        capacity: Energy,
        max_output_power: Power,
    ) -> Self {
        Self::new_with_transfer_limits(id, name, carrier, capacity, Power::ZERO, max_output_power)
    }

    /// Builds a finite energy store with explicit independent input and output power envelopes.
    ///
    /// Either direction may be zero, allowing a pure source or pure sink. A store with no transfer
    /// direction at all would be inert runtime state and is rejected.
    #[must_use]
    pub fn new_with_transfer_limits(
        id: EnergyStoreDefinitionId,
        name: impl Into<String>,
        carrier: EnergyCarrier,
        capacity: Energy,
        max_input_power: Power,
        max_output_power: Power,
    ) -> Self {
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "energy store name must not be empty"
        );
        assert!(!capacity.is_zero(), "energy store capacity must be nonzero");
        assert!(
            !max_input_power.is_zero() || !max_output_power.is_zero(),
            "energy store must accept input, provide output, or both"
        );
        Self {
            id,
            name,
            carrier,
            capacity,
            max_input_power,
            max_output_power,
            assembly_profile: None,
        }
    }

    /// Adds the exact conserved matter required to construct this store in gameplay.
    #[must_use]
    pub fn with_assembly_profile(mut self, profile: MaterialAssemblyProfile) -> Self {
        self.assembly_profile = Some(profile);
        self
    }

    #[must_use]
    pub const fn id(&self) -> EnergyStoreDefinitionId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn carrier(&self) -> EnergyCarrier {
        self.carrier
    }

    #[must_use]
    pub const fn capacity(&self) -> Energy {
        self.capacity
    }

    #[must_use]
    pub const fn max_input_power(&self) -> Power {
        self.max_input_power
    }

    #[must_use]
    pub const fn max_output_power(&self) -> Power {
        self.max_output_power
    }

    #[must_use]
    pub fn assembly_profile(&self) -> Option<&MaterialAssemblyProfile> {
        self.assembly_profile.as_ref()
    }
}

/// Immutable deterministic authored lookup table for finite energy stores.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyRegistry {
    definitions: BTreeMap<EnergyStoreDefinitionId, EnergyStoreDefinition>,
}

impl EnergyRegistry {
    pub(crate) fn new(definitions: impl IntoIterator<Item = EnergyStoreDefinition>) -> Self {
        let mut by_id = BTreeMap::new();
        for definition in definitions {
            let id = definition.id();
            assert!(
                by_id.insert(id, definition).is_none(),
                "duplicate energy store definition id {}",
                id.value()
            );
        }
        Self { definitions: by_id }
    }

    #[must_use]
    pub fn get_store(&self, id: EnergyStoreDefinitionId) -> Option<&EnergyStoreDefinition> {
        self.definitions.get(&id)
    }

    /// Iterates authored storage definitions in stable ID order.
    pub fn definitions(&self) -> impl Iterator<Item = &EnergyStoreDefinition> {
        self.definitions.values()
    }

    pub(crate) fn validate_references(&self, materials: &MaterialRegistry) {
        for definition in self.definitions.values() {
            if let Some(assembly) = definition.assembly_profile() {
                assert!(
                    assembly
                        .validate_infrastructure_references(materials)
                        .is_ok(),
                    "energy store definition {} assembly profile must use existing consolidated solid commodities",
                    definition.id().value()
                );
            }
        }
    }
}
