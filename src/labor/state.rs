//! Persistent revisioned ownership of the locally controlled player's active work.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::work::{
    EquipmentMaintenanceWork, ManualPowerWork, PlayerWork, ProspectingWork,
    StorageEnclosureDismantlingWork,
};

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
            _ => None,
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
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn get_manual_power_energy_occupant(
        &self,
        store: EnergyStoreId,
    ) -> Option<ManualPowerWork> {
        match self.active {
            Some(PlayerWork::ManualPower { work }) if work.destination() == store => Some(work),
            _ => None,
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
            _ => None,
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
            _ => None,
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
