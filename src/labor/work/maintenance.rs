//! Durable equipment-maintenance attention intervals.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::maintenance::Condition;

/// Durable direct-labor interval for an already-admitted equipment service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentMaintenanceWork {
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    admission_revision: u64,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EquipmentMaintenanceWork {
    pub(crate) const fn new(
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        admission_revision: u64,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            equipment,
            condition_after,
            admission_revision,
            started_at,
            completes_at,
        }
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
    pub const fn condition_before(self) -> Condition {
        self.equipment.condition()
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn admission_revision(self) -> u64 {
        self.admission_revision
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
