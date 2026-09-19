//! Aggregate durable player-work identity and owner-local variant queries.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;

use super::{
    DrinkingWork, EatingWork, EquipmentMaintenanceWork, ManualPowerWork, ProspectingWork,
    StorageEnclosureDismantlingWork,
};

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

impl PlayerWork {
    /// Future inventory-owner revisions still owed by this direct-work record after admission.
    ///
    /// Manual production and mining keep their delayed owner effects in their own durable job
    /// records, so they deliberately contribute no direct-work demand here.
    #[must_use]
    pub(in crate::labor) const fn future_inventory_revision_demand(self) -> u64 {
        match self {
            Self::StorageEnclosureDismantling { .. } => 2,
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::ManualPower { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::EquipmentMaintenance { .. } => 0,
        }
    }

    /// Future energy-owner revisions still owed directly by this work record after admission.
    #[must_use]
    pub(in crate::labor) const fn future_energy_revision_demand(self) -> u64 {
        match self {
            Self::ManualPower { .. } => 1,
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::EquipmentMaintenance { .. }
            | Self::StorageEnclosureDismantling { .. } => 0,
        }
    }

    /// Future equipment-owner revisions still owed directly by this work record after admission.
    ///
    /// Mining wear is scheduled by `MiningState`, while manual production wear is scheduled by
    /// `ProductionState`; counting either here would reserve the same completion twice.
    #[must_use]
    pub(in crate::labor) const fn future_equipment_revision_demand(self) -> u64 {
        match self {
            Self::ManualPower { .. } | Self::EquipmentMaintenance { .. } => 1,
            Self::Prospecting { work } if work.equipment().is_some() => 1,
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::StorageEnclosureDismantling { .. } => 0,
        }
    }

    #[must_use]
    pub(crate) const fn prospecting(self) -> Option<ProspectingWork> {
        match self {
            Self::Prospecting { work } => Some(work),
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::ManualPower { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::EquipmentMaintenance { .. }
            | Self::StorageEnclosureDismantling { .. } => None,
        }
    }

    #[must_use]
    pub(crate) const fn manual_power(self) -> Option<ManualPowerWork> {
        match self {
            Self::ManualPower { work } => Some(work),
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::EquipmentMaintenance { .. }
            | Self::StorageEnclosureDismantling { .. } => None,
        }
    }

    #[must_use]
    pub(crate) const fn inline_schedule(self) -> Option<(SimulationTick, SimulationTick)> {
        match self {
            Self::ManualPower { work } => Some((work.started_at(), work.completes_at())),
            Self::Prospecting { work } => Some((work.started_at(), work.completes_at())),
            Self::Eating { work } => Some((work.started_at(), work.completes_at())),
            Self::Drinking { work } => Some((work.started_at(), work.completes_at())),
            Self::EquipmentMaintenance { work } => Some((work.started_at(), work.completes_at())),
            Self::StorageEnclosureDismantling { work } => {
                Some((work.started_at(), work.completes_at()))
            }
            Self::ManualProduction { .. } | Self::Mining { .. } => None,
        }
    }

    #[must_use]
    pub(crate) const fn equipment_maintenance(self) -> Option<EquipmentMaintenanceWork> {
        match self {
            Self::EquipmentMaintenance { work } => Some(work),
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::ManualPower { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::StorageEnclosureDismantling { .. } => None,
        }
    }

    #[must_use]
    pub(crate) const fn storage_dismantling(self) -> Option<StorageEnclosureDismantlingWork> {
        match self {
            Self::StorageEnclosureDismantling { work } => Some(work),
            Self::ManualProduction { .. }
            | Self::Mining { .. }
            | Self::ManualPower { .. }
            | Self::Prospecting { .. }
            | Self::Eating { .. }
            | Self::Drinking { .. }
            | Self::EquipmentMaintenance { .. } => None,
        }
    }
}
