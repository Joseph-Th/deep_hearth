//! Durable direct-labor power work.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::energy::{EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::maintenance::Condition;
use crate::survival::SurvivalExertion;

use super::super::ManualPowerMethodId;

/// Durable direct-labor work order that converts player effort into finite mechanical energy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualPowerWork {
    method: ManualPowerMethodId,
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    output: ReleasedEnergyTrace,
    exertion: SurvivalExertion,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl ManualPowerWork {
    pub(crate) const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        output: ReleasedEnergyTrace,
        exertion: SurvivalExertion,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            equipment,
            condition_after,
            output,
            exertion,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn method(self) -> ManualPowerMethodId {
        self.method
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
    pub const fn destination(self) -> EnergyStoreId {
        self.output.destination()
    }

    #[must_use]
    pub const fn output(self) -> ReleasedEnergyTrace {
        self.output
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
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
