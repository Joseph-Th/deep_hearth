//! Durable storage-enclosure dismantling attention work.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::{StockpileId, StorageDefinitionId};

/// Durable direct-labor interval for dismantling one installed storage enclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEnclosureDismantlingWork {
    target: StockpileId,
    recovery_destination: StockpileId,
    definition: StorageDefinitionId,
    enclosure_created_at: SimulationTick,
    recovered_mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl StorageEnclosureDismantlingWork {
    pub(crate) const fn new(
        target: StockpileId,
        recovery_destination: StockpileId,
        definition: StorageDefinitionId,
        enclosure_created_at: SimulationTick,
        recovered_mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            target,
            recovery_destination,
            definition,
            enclosure_created_at,
            recovered_mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn target(self) -> StockpileId {
        self.target
    }

    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }

    #[must_use]
    pub const fn definition(self) -> StorageDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn enclosure_created_at(self) -> SimulationTick {
        self.enclosure_created_at
    }

    #[must_use]
    pub const fn recovered_mass(self) -> Mass {
        self.recovered_mass
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }

    #[must_use]
    pub fn occupies_stockpile(self, stockpile: StockpileId) -> bool {
        self.target == stockpile || self.recovery_destination == stockpile
    }
}
