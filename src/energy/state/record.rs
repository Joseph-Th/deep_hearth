//! Durable energy-store identity, record, and prevalidated upgrade payload.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Mass};
use crate::core::time::SimulationTick;
use crate::inventory::{ConsumedMaterialTrace, checked_consumed_material_mass};

use super::super::definitions::EnergyStoreDefinitionId;

/// Persistent identity of one runtime energy store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EnergyStoreId(pub(super) u64);

impl EnergyStoreId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "energy store id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Authoritative changing state for one finite energy store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyStoreRecord {
    pub(in crate::energy) id: EnergyStoreId,
    pub(in crate::energy) definition: EnergyStoreDefinitionId,
    pub(in crate::energy) stored: Energy,
    pub(in crate::energy) embodied_material: Vec<ConsumedMaterialTrace>,
    pub(in crate::energy) created_at: SimulationTick,
}

impl EnergyStoreRecord {
    #[must_use]
    pub const fn id(&self) -> EnergyStoreId {
        self.id
    }

    #[must_use]
    pub const fn definition(&self) -> EnergyStoreDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn stored(&self) -> Energy {
        self.stored
    }

    /// Conserved matter physically embodied in this storage instance.
    #[must_use]
    pub fn embodied_mass(&self) -> Mass {
        checked_consumed_material_mass(&self.embodied_material).unwrap_or_else(|| {
            panic!(
                "validated energy store {} embodied trace mass overflowed",
                self.id.value()
            )
        })
    }

    /// Exact material/provenance traces transferred into this store at construction.
    #[must_use]
    pub fn embodied_material(&self) -> &[ConsumedMaterialTrace] {
        &self.embodied_material
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Complete owner-local payload for one prevalidated additive energy-store upgrade.
pub(in crate::energy) struct EnergyStoreUpgradeMutation {
    pub(in crate::energy) store: EnergyStoreId,
    pub(in crate::energy) expected_definition: EnergyStoreDefinitionId,
    pub(in crate::energy) target_definition: EnergyStoreDefinitionId,
    pub(in crate::energy) expected_embodied_mass: Mass,
    pub(in crate::energy) additions: Vec<ConsumedMaterialTrace>,
}
