//! Persistent exclusive ownership of the locally controlled player's active work.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Volume};
use crate::core::time::SimulationTick;
use crate::energy::{EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{StockpileId, StorageDefinitionId};
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

/// Durable direct-labor interval for dismantling one installed storage enclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEnclosureDismantlingWork {
    target: StockpileId,
    recovery_destination: StockpileId,
    definition: StorageDefinitionId,
    enclosure_created_at: SimulationTick,
    recovered_mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl StorageEnclosureDismantlingWork {
    pub(crate) const fn new(
        target: StockpileId,
        recovery_destination: StockpileId,
        definition: StorageDefinitionId,
        enclosure_created_at: SimulationTick,
        recovered_mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            target,
            recovery_destination,
            definition,
            enclosure_created_at,
            recovered_mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn target(self) -> StockpileId {
        self.target
    }
    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }
    #[must_use]
    pub const fn definition(self) -> StorageDefinitionId {
        self.definition
    }
    #[must_use]
    pub const fn enclosure_created_at(self) -> SimulationTick {
        self.enclosure_created_at
    }
    #[must_use]
    pub const fn recovered_mass(self) -> Mass {
        self.recovered_mass
    }
    #[must_use]
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }
    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }

    #[must_use]
    pub fn occupies_stockpile(self, stockpile: StockpileId) -> bool {
        self.target == stockpile || self.recovery_destination == stockpile
    }
}

/// Durable direct-labor interval for an already-admitted equipment service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentMaintenanceWork {
    equipment: EquipmentOperationTrace,
    condition_after: Condition,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EquipmentMaintenanceWork {
    pub(crate) const fn new(
        equipment: EquipmentOperationTrace,
        condition_after: Condition,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            equipment,
            condition_after,
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
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Durable attention interval occupied by one already-admitted direct meal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EatingWork {
    mass: Mass,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl EatingWork {
    pub(crate) const fn new(
        mass: Mass,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            mass,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
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

/// Durable attention interval occupied by one already-admitted direct drink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrinkingWork {
    volume: Volume,
    started_at: SimulationTick,
    completes_at: SimulationTick,
}

impl DrinkingWork {
    pub(crate) const fn new(
        volume: Volume,
        started_at: SimulationTick,
        completes_at: SimulationTick,
    ) -> Self {
        Self {
            volume,
            started_at,
            completes_at,
        }
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
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
    ManualProduction {
        job: ProductionJobId,
    },
    Mining {
        job: MiningJobId,
    },
    ManualPower {
        work: ManualPowerWork,
    },
    Prospecting {
        work: ProspectingWork,
    },
    Eating {
        work: EatingWork,
    },
    Drinking {
        work: DrinkingWork,
    },
    EquipmentMaintenance {
        work: EquipmentMaintenanceWork,
    },
    StorageEnclosureDismantling {
        work: StorageEnclosureDismantlingWork,
    },
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
    pub(crate) fn get_prospecting_equipment_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<ProspectingWork> {
        match self.active {
            Some(PlayerWork::Prospecting { work }) if work.equipment() == Some(equipment) => {
                Some(work)
            }
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | Some(PlayerWork::ManualPower { work: _ })
            | Some(PlayerWork::Prospecting { work: _ })
            | Some(PlayerWork::Eating { work: _ })
            | Some(PlayerWork::Drinking { work: _ })
            | Some(PlayerWork::EquipmentMaintenance { work: _ })
            | Some(PlayerWork::StorageEnclosureDismantling { work: _ })
            | None => None,
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
            Some(PlayerWork::Eating { work }) => {
                work.started_at() <= current && work.completes_at() > current
            }
            Some(PlayerWork::Drinking { work }) => {
                work.started_at() <= current && work.completes_at() > current
            }
            Some(PlayerWork::EquipmentMaintenance { work }) => {
                work.started_at() <= current && work.completes_at() > current
            }
            Some(PlayerWork::StorageEnclosureDismantling { work }) => {
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
            | Some(PlayerWork::Eating { work: _ })
            | Some(PlayerWork::Drinking { work: _ })
            | Some(PlayerWork::EquipmentMaintenance { work: _ })
            | Some(PlayerWork::StorageEnclosureDismantling { work: _ })
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
            | Some(PlayerWork::Eating { work: _ })
            | Some(PlayerWork::Drinking { work: _ })
            | Some(PlayerWork::EquipmentMaintenance { work: _ })
            | Some(PlayerWork::StorageEnclosureDismantling { work: _ })
            | None => None,
        }
    }

    #[must_use]
    pub(crate) fn get_equipment_maintenance_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<EquipmentMaintenanceWork> {
        match self.active {
            Some(PlayerWork::EquipmentMaintenance { work }) if work.equipment() == equipment => {
                Some(work)
            }
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | Some(PlayerWork::ManualPower { work: _ })
            | Some(PlayerWork::Prospecting { work: _ })
            | Some(PlayerWork::Eating { work: _ })
            | Some(PlayerWork::Drinking { work: _ })
            | Some(PlayerWork::EquipmentMaintenance { work: _ })
            | Some(PlayerWork::StorageEnclosureDismantling { work: _ })
            | None => None,
        }
    }

    #[must_use]
    pub(crate) fn get_storage_dismantling_stockpile_occupant(
        &self,
        stockpile: StockpileId,
    ) -> Option<StorageEnclosureDismantlingWork> {
        match self.active {
            Some(PlayerWork::StorageEnclosureDismantling { work })
                if work.occupies_stockpile(stockpile) =>
            {
                Some(work)
            }
            Some(PlayerWork::ManualProduction { job: _ })
            | Some(PlayerWork::Mining { job: _ })
            | Some(PlayerWork::ManualPower { work: _ })
            | Some(PlayerWork::Prospecting { work: _ })
            | Some(PlayerWork::Eating { work: _ })
            | Some(PlayerWork::Drinking { work: _ })
            | Some(PlayerWork::EquipmentMaintenance { work: _ })
            | Some(PlayerWork::StorageEnclosureDismantling { work: _ })
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
