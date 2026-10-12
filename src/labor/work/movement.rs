//! Durable direct-labor interval for one adjacent open-space player step.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::spatial::VoxelCoord;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerStepWork {
    origin: VoxelCoord,
    destination: VoxelCoord,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl PlayerStepWork {
    pub(crate) const fn new(
        origin: VoxelCoord,
        destination: VoxelCoord,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            origin,
            destination,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn origin(self) -> VoxelCoord {
        self.origin
    }

    #[must_use]
    pub const fn destination(self) -> VoxelCoord {
        self.destination
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
