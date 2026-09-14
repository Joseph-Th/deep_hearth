//! Typed failures for runtime equipment-provider resolution.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::{EquipmentDefinitionId, EquipmentId};

/// Failure to resolve a runtime equipment provider or establish its current availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentProviderError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    StructuralSupportRequired {
        equipment: EquipmentId,
    },
    UnknownStructuralSupport {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    StructuralSupportNotActive {
        equipment: EquipmentId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    MaintenanceInProgress {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    ProspectingInProgress {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    ProductionInProgress {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    MiningInProgress {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    ManualPowerInProgress {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
}

impl Display for EquipmentProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
            Self::StructuralSupportRequired { equipment } => write!(
                formatter,
                "equipment {} requires an active structural support before it can authorize work",
                equipment.value()
            ),
            Self::UnknownStructuralSupport { equipment, element } => write!(
                formatter,
                "equipment {} references missing structural support {}",
                equipment.value(),
                element.value()
            ),
            Self::MaintenanceInProgress {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is under maintenance until tick {} and cannot authorize a new operation",
                equipment.value(),
                completes_at.value()
            ),
            Self::ProspectingInProgress {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by geological sampling until tick {} and cannot authorize a new operation",
                equipment.value(),
                completes_at.value()
            ),
            Self::ProductionInProgress {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot authorize a new operation",
                equipment.value(),
                job.value()
            ),
            Self::MiningInProgress { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot authorize a new operation",
                equipment.value(),
                job.value()
            ),
            Self::ManualPowerInProgress {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation until tick {} and cannot authorize a new operation",
                equipment.value(),
                completes_at.value()
            ),
            Self::StructuralSupportNotActive {
                equipment,
                element,
                lifecycle,
            } => write!(
                formatter,
                "equipment {} structural support {} is {lifecycle:?} and cannot authorize a new operation",
                equipment.value(),
                element.value()
            ),
        }
    }
}

impl Error for EquipmentProviderError {}
