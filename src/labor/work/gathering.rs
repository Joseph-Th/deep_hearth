//! Durable direct-labor interval for collecting finite loose surface matter.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::StockpileId;
use crate::surface::SurfaceResourceId;

use super::super::SurfaceGatheringMethodId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceGatheringWork {
    resource: SurfaceResourceId,
    method: SurfaceGatheringMethodId,
    destination: StockpileId,
    gathered_mass: Mass,
    source_mass_before: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl SurfaceGatheringWork {
    pub(crate) const fn new(
        resource: SurfaceResourceId,
        method: SurfaceGatheringMethodId,
        destination: StockpileId,
        gathered_mass: Mass,
        source_mass_before: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            resource,
            method,
            destination,
            gathered_mass,
            source_mass_before,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }
    #[must_use]
    pub const fn method(self) -> SurfaceGatheringMethodId {
        self.method
    }
    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }
    #[must_use]
    pub const fn gathered_mass(self) -> Mass {
        self.gathered_mass
    }
    #[must_use]
    pub const fn source_mass_before(self) -> Mass {
        self.source_mass_before
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
