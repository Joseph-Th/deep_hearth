//! Durable direct block-excavation attention work.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::mining::MiningMethodId;
use crate::spatial::VoxelCoord;
use crate::world::BlockStateId;

/// One bounded hand-excavation batch against a targeted world cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockExcavationWork {
    target: VoxelCoord,
    state: BlockStateId,
    source_mass_before: Mass,
    excavated_mass: Mass,
    method: MiningMethodId,
    destination: StockpileId,
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl BlockExcavationWork {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        target: VoxelCoord,
        state: BlockStateId,
        source_mass_before: Mass,
        excavated_mass: Mass,
        method: MiningMethodId,
        destination: StockpileId,
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            target,
            state,
            source_mass_before,
            excavated_mass,
            method,
            destination,
            equipment,
            condition_after,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn target(self) -> VoxelCoord {
        self.target
    }

    #[must_use]
    pub const fn state(self) -> BlockStateId {
        self.state
    }

    #[must_use]
    pub const fn source_mass_before(self) -> Mass {
        self.source_mass_before
    }

    #[must_use]
    pub const fn excavated_mass(self) -> Mass {
        self.excavated_mass
    }

    #[must_use]
    pub const fn method(self) -> MiningMethodId {
        self.method
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment.equipment()
    }

    #[must_use]
    pub const fn equipment_trace(self) -> EquipmentOperationTrace {
        self.equipment
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
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
