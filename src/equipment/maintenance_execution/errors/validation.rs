//! Validation and admission failures for resolved equipment maintenance.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{SimulationTick, TickSpan};
use crate::labor::PlayerWorkStartError;
use crate::logistics::{PlayerEquipmentAccessError, PlayerStockpileAccessError};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::super::super::definitions::EquipmentDefinitionId;
use super::super::super::state::EquipmentId;
use super::material::EquipmentMaintenanceMaterialError;

/// Failure while validating an already physically resolved equipment maintenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceError {
    EquipmentAccess(PlayerEquipmentAccessError),
    MaterialSourceAccess(PlayerStockpileAccessError),
    SpentDestinationAccess(PlayerStockpileAccessError),
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    StaleEquipmentResolution {
        equipment: EquipmentId,
        expected_revision: u64,
        actual_revision: u64,
    },
    ConditionChangedSinceResolution {
        equipment: EquipmentId,
        expected: Condition,
        actual: Condition,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    EquipmentUnderMaintenance {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    ConditionNotImproved {
        equipment: EquipmentId,
        before: Condition,
        after: Condition,
    },
    ImpureReplacementMaterial {
        commodity: CommodityKey,
    },
    EquipmentRevisionExhausted,
    CompletionTickOverflow {
        current: SimulationTick,
        duration: TickSpan,
    },
    PlayerWork(PlayerWorkStartError),
    Material(EquipmentMaintenanceMaterialError),
}

impl Display for EquipmentMaintenanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EquipmentAccess(error) => {
                write!(formatter, "equipment maintenance access failed: {error}")
            }
            Self::MaterialSourceAccess(error) => write!(
                formatter,
                "equipment maintenance replacement-source access failed: {error}"
            ),
            Self::SpentDestinationAccess(error) => write!(
                formatter,
                "equipment maintenance spent-destination access failed: {error}"
            ),
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {} during maintenance validation",
                equipment.value(),
                definition.value()
            ),
            Self::StaleEquipmentResolution {
                equipment,
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "equipment {} changed from maintenance-resolution revision {expected_revision} to {actual_revision} before transaction validation",
                equipment.value()
            ),
            Self::ConditionChangedSinceResolution {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from maintenance-resolution {} ppm to {} ppm before transaction validation",
                equipment.value(),
                expected.parts_per_million(),
                actual.parts_per_million()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be serviced",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be serviced",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be serviced",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by geological sampling until tick {} and cannot be serviced",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is already under maintenance until tick {}",
                equipment.value(),
                completes_at.value()
            ),
            Self::ConditionNotImproved {
                equipment,
                before,
                after,
            } => write!(
                formatter,
                "equipment {} maintenance must improve condition above {} ppm; resolved outcome is {} ppm",
                equipment.value(),
                before.parts_per_million(),
                after.parts_per_million()
            ),
            Self::ImpureReplacementMaterial { commodity } => write!(
                formatter,
                "equipment maintenance replacement commodity {} must be pure authored material",
                commodity.value()
            ),
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted during maintenance")
            }
            Self::CompletionTickOverflow { current, duration } => write!(
                formatter,
                "equipment maintenance starting at tick {} cannot schedule {} active ticks",
                current.value(),
                duration.value()
            ),
            Self::PlayerWork(error) => write!(
                formatter,
                "equipment maintenance labor cannot start: {error}"
            ),
            Self::Material(error) => write!(
                formatter,
                "equipment maintenance material transaction is invalid: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EquipmentAccess(error) => Some(error),
            Self::MaterialSourceAccess(error) => Some(error),
            Self::SpentDestinationAccess(error) => Some(error),
            Self::Material(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::UnknownEquipment {
                equipment: _equipment,
            } => None,
            Self::UnknownDefinition {
                equipment: _equipment,
                definition: _definition,
            } => None,
            Self::StaleEquipmentResolution {
                equipment: _equipment,
                expected_revision: _expected_revision,
                actual_revision: _actual_revision,
            } => None,
            Self::ConditionChangedSinceResolution {
                equipment: _equipment,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::EquipmentBusy {
                equipment: _equipment,
                job: _job,
                release: _release,
            } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
            Self::EquipmentBusyProspecting {
                equipment: _equipment,
                completes_at: _completes_at,
            } => None,
            Self::EquipmentUnderMaintenance {
                equipment: _equipment,
                completes_at: _completes_at,
            } => None,
            Self::ConditionNotImproved {
                equipment: _equipment,
                before: _before,
                after: _after,
            } => None,
            Self::ImpureReplacementMaterial {
                commodity: _commodity,
            } => None,
            Self::EquipmentRevisionExhausted => None,
            Self::CompletionTickOverflow { .. } => None,
        }
    }
}
