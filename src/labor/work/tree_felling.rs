//! Durable interval during which player attention is devoted to chopping one rooted tree.

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::spatial::VoxelCoord;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeFellingWork {
    root: VoxelCoord,
    equipment: EquipmentId,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl TreeFellingWork {
    pub(crate) const fn new(
        root: VoxelCoord,
        equipment: EquipmentId,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            root,
            equipment,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn root(self) -> VoxelCoord {
        self.root
    }
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
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
