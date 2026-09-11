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
