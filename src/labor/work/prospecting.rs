//! Durable geological-prospecting attention work.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::maintenance::Condition;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::ProspectingMethodId;

/// Durable bounded field-inspection work that will resolve one geological observation at completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProspectingWork {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    equipment: Option<EquipmentOperationTrace>,
    condition_after: Option<Condition>,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl ProspectingWork {
    pub(crate) const fn new(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
        equipment: Option<EquipmentOperationTrace>,
        condition_after: Option<Condition>,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            region,
            material,
            equipment,
            condition_after,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn method(self) -> ProspectingMethodId {
        self.method
    }

    #[must_use]
    pub const fn region(self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn equipment(self) -> Option<EquipmentId> {
        match self.equipment {
            Some(trace) => Some(trace.equipment()),
            None => None,
        }
    }

    #[must_use]
    pub const fn equipment_trace(self) -> Option<EquipmentOperationTrace> {
        self.equipment
    }

    #[must_use]
    pub const fn condition_after(self) -> Option<Condition> {
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
