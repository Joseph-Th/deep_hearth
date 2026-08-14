//! Immutable definitions for finite energy stores; runtime state owns only changing stored energy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};

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

/// Immutable authored capacity and discharge envelope for one energy-store class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyStoreDefinition {
    id: EnergyStoreDefinitionId,
    name: String,
    carrier: EnergyCarrier,
    capacity: Energy,
    max_output_power: Power,
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
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "energy store name must not be empty"
        );
        assert!(!capacity.is_zero(), "energy store capacity must be nonzero");
        assert!(
            !max_output_power.is_zero(),
            "energy store output power must be nonzero"
        );
        Self {
            id,
            name,
            carrier,
            capacity,
            max_output_power,
        }
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
    pub const fn max_output_power(&self) -> Power {
        self.max_output_power
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
}
