//! Persistent exclusive ownership of the locally controlled player's active work.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::energy::{EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::maintenance::Condition;
use crate::material::MaterialId;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;
use crate::spatial::VoxelBounds;

use super::{ManualPowerMethodId, ProspectingMethodId};

/// Durable direct-labor work order that converts player effort into finite mechanical energy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualPowerWork {
    method: ManualPowerMethodId,
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    output: ReleasedEnergyTrace,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

/// Durable bounded field-inspection work that will resolve one geological observation at completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProspectingWork {
    method: ProspectingMethodId,
    region: VoxelBounds,
    material: MaterialId,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl ProspectingWork {
    pub(crate) const fn new(
        method: ProspectingMethodId,
        region: VoxelBounds,
        material: MaterialId,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            region,
            material,
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
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

impl ManualPowerWork {
    pub(crate) const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        output: ReleasedEnergyTrace,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            method,
            equipment,
            condition_after,
            output,
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
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable activity currently monopolizing the local player's labor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PlayerWork {
    ManualProduction { job: ProductionJobId },
    Mining { job: MiningJobId },
    ManualPower { work: ManualPowerWork },
    Prospecting { work: ProspectingWork },
}

/// Single-player labor owner with an explicit revision for cross-system transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerWorkState {
    revision: u64,
    active: Option<PlayerWork>,
}

impl PlayerWorkState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            active: None,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn active(&self) -> Option<PlayerWork> {
        self.active
    }

    #[must_use]
    pub(crate) fn has_valid_inline_schedule(&self, current: SimulationTick) -> bool {
        match self.active {
            Some(PlayerWork::ManualPower { work }) => {
                work.started_at() <= current && work.completes_at() > current
            }
            Some(PlayerWork::Prospecting { work }) => {
                work.started_at() <= current && work.completes_at() > current
            }
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | None => true,
        }
    }

    #[must_use]
    pub(crate) fn get_manual_power_equipment_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<ManualPowerWork> {
        match self.active {
            Some(PlayerWork::ManualPower { work }) if work.equipment() == equipment => Some(work),
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | Some(PlayerWork::ManualPower { work: _ })
            | Some(PlayerWork::Prospecting { work: _ })
            | None => None,
        }
    }

    #[must_use]
    pub(crate) fn get_manual_power_energy_occupant(
        &self,
        store: EnergyStoreId,
    ) -> Option<ManualPowerWork> {
        match self.active {
            Some(PlayerWork::ManualPower { work }) if work.destination() == store => Some(work),
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | Some(PlayerWork::ManualPower { work: _ })
            | Some(PlayerWork::Prospecting { work: _ })
            | None => None,
        }
    }

    pub(crate) fn apply_start(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        work: PlayerWork,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert!(self.active.is_none());
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        self.active = Some(work);
        self.revision = next_revision;
    }

    pub(crate) fn apply_release(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        work: PlayerWork,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(self.active, Some(work));
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        self.active = None;
        self.revision = next_revision;
    }
}
