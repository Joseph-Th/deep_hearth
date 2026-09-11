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
        self.active
            .and_then(PlayerWork::prospecting)
            .filter(|work| work.equipment() == Some(equipment))
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn active(&self) -> Option<PlayerWork> {
        self.active
    }

    /// Returns the admitted enclosure-dismantling work that completes on `tick`, if any.
    #[must_use]
    pub(crate) fn storage_dismantling_due_at(
        &self,
        tick: SimulationTick,
    ) -> Option<StorageEnclosureDismantlingWork> {
        self.active
            .and_then(PlayerWork::storage_dismantling)
            .filter(|work| work.completes_at() == tick)
    }

    /// Returns admitted equipment-maintenance work that completes on `tick`, if any.
    #[must_use]
    pub(crate) fn equipment_maintenance_due_at(
        &self,
        tick: SimulationTick,
    ) -> Option<EquipmentMaintenanceWork> {
        self.active
            .and_then(PlayerWork::equipment_maintenance)
            .filter(|work| work.completes_at() == tick)
    }

    #[must_use]
    pub(crate) fn has_valid_inline_schedule(&self, current: SimulationTick) -> bool {
        self.active
            .and_then(PlayerWork::inline_schedule)
            .is_none_or(|(started_at, completes_at)| {
                started_at <= current && completes_at > current
            })
    }

    #[must_use]
    pub(crate) fn get_manual_power_equipment_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<ManualPowerWork> {
        self.active
            .and_then(PlayerWork::manual_power)
            .filter(|work| work.equipment() == equipment)
    }

    #[must_use]
    pub(crate) fn get_manual_power_energy_occupant(
        &self,
        store: EnergyStoreId,
    ) -> Option<ManualPowerWork> {
        self.active
            .and_then(PlayerWork::manual_power)
            .filter(|work| work.destination() == store)
    }

    #[must_use]
    pub(crate) fn get_equipment_maintenance_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<EquipmentMaintenanceWork> {
        self.active
            .and_then(PlayerWork::equipment_maintenance)
            .filter(|work| work.equipment() == equipment)
    }

    #[must_use]
    pub(crate) fn get_storage_dismantling_stockpile_occupant(
        &self,
        stockpile: StockpileId,
    ) -> Option<StorageEnclosureDismantlingWork> {
        self.active
            .and_then(PlayerWork::storage_dismantling)
            .filter(|work| work.occupies_stockpile(stockpile))
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
