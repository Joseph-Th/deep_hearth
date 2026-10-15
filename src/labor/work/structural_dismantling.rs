//! Durable direct player work for recovering one simple structural member.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::StockpileId;
use crate::structural::StructuralElementId;

use super::super::StructuralConstructionMethodId;

/// Exact identity and recovery obligation for one admitted structural dismantling interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralDismantlingWork {
    element: StructuralElementId,
    recovery_destination: StockpileId,
    method: StructuralConstructionMethodId,
    recovered_mass: Mass,
    recovery_parcels: u64,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl StructuralDismantlingWork {
    pub(crate) const fn new(
        element: StructuralElementId,
        recovery_destination: StockpileId,
        method: StructuralConstructionMethodId,
        recovered_mass: Mass,
        recovery_parcels: u64,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            element,
            recovery_destination,
            method,
            recovered_mass,
            recovery_parcels,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }

    #[must_use]
    pub const fn method(self) -> StructuralConstructionMethodId {
        self.method
    }

    #[must_use]
    pub const fn recovered_mass(self) -> Mass {
        self.recovered_mass
    }

    #[must_use]
    pub const fn recovery_parcels(self) -> u64 {
        self.recovery_parcels
    }

    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}
