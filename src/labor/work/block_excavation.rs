//! Durable direct block-excavation attention work.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::ControlTick;
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::mining::MiningMethodId;
use crate::spatial::VoxelCoord;
use crate::world::BlockStateId;

/// Durable custody destination for one excavation batch's fragmented output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BlockExcavationOutputCustody {
    Carried { stockpile: StockpileId },
    Surface { position: VoxelCoord },
}

impl BlockExcavationOutputCustody {
    #[must_use]
    pub const fn carried_stockpile(self) -> Option<StockpileId> {
        match self {
            Self::Carried { stockpile } => Some(stockpile),
            Self::Surface { .. } => None,
        }
    }

    #[must_use]
    pub const fn surface_position(self) -> Option<VoxelCoord> {
        match self {
            Self::Surface { position } => Some(position),
            Self::Carried { .. } => None,
        }
    }
}

/// One bounded hand-excavation batch against a targeted world cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockExcavationWork {
    target: VoxelCoord,
    state: BlockStateId,
    source_mass_before: Mass,
    excavated_mass: Mass,
    method: MiningMethodId,
    output: BlockExcavationOutputCustody,
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    started_at: ControlTick,
    completes_at: ControlTick,
}

impl BlockExcavationWork {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        target: VoxelCoord,
        state: BlockStateId,
        source_mass_before: Mass,
        excavated_mass: Mass,
        method: MiningMethodId,
        output: BlockExcavationOutputCustody,
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        started_at: ControlTick,
        completes_at: ControlTick,
    ) -> Self {
        Self {
            target,
            state,
            source_mass_before,
            excavated_mass,
            method,
            output,
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
    pub const fn output(self) -> BlockExcavationOutputCustody {
        self.output
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
    pub const fn started_at(self) -> ControlTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> ControlTick {
        self.completes_at
    }

    #[must_use]
    pub const fn duration_control_ticks(self) -> Option<u64> {
        self.completes_at.checked_steps_since(self.started_at)
    }

    #[must_use]
    pub fn progress_ppm(self, current: ControlTick) -> u32 {
        let duration = self
            .duration_control_ticks()
            .unwrap_or_else(|| unreachable!("trusted excavation schedule has positive duration"));
        let elapsed = current
            .checked_steps_since(self.started_at)
            .unwrap_or(0)
            .min(duration);
        let progress = u128::from(elapsed).saturating_mul(u128::from(
            crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION,
        )) / u128::from(duration);
        u32::try_from(progress).unwrap_or(crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION)
    }
}
