//! Typed admission and commit errors for physical structural recovery.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError, StructuralConstructionMethodId};
use crate::spatial::VoxelCoord;

use super::StructuralElementId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralDismantlingError {
    PlayerNotAdmitted,
    PlayerDead,
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotRecoverable {
        element: StructuralElementId,
    },
    ElementOutOfReach {
        element: StructuralElementId,
    },
    ElementHasDependents {
        element: StructuralElementId,
    },
    ElementHasExternalLoad {
        element: StructuralElementId,
    },
    ElementSupportsEquipment {
        element: StructuralElementId,
    },
    ElementSupportsStockpile {
        element: StructuralElementId,
    },
    ElementSupportsFluidStore {
        element: StructuralElementId,
    },
    UnknownMethod {
        method: StructuralConstructionMethodId,
    },
    MemberTooHeavy {
        required: Mass,
        maximum: Mass,
    },
    CompletionTickOverflow,
    RecoveryAccess(crate::logistics::PlayerStockpileAccessError),
    RecoveryIngress,
    RecoveryReservation,
    MaterialLotIdExhausted,
    InventoryRevisionExhausted,
    StructureRevisionExhausted,
    Work(PlayerWorkStartError),
}

impl Display for PlayerStructuralDismantlingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("structural dismantling requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot dismantle structure"),
            Self::UnknownElement { element } => write!(
                formatter,
                "structural dismantling member {} does not exist",
                element.value()
            ),
            Self::ElementNotRecoverable { element } => write!(
                formatter,
                "structural member {} has no embodied material to recover",
                element.value()
            ),
            Self::ElementOutOfReach { element } => write!(
                formatter,
                "structural member {} is outside player interaction reach",
                element.value()
            ),
            Self::ElementHasDependents { element } => write!(
                formatter,
                "structural member {} still supports dependent members",
                element.value()
            ),
            Self::ElementHasExternalLoad { element } => write!(
                formatter,
                "structural member {} still carries external loads",
                element.value()
            ),
            Self::ElementSupportsEquipment { element } => write!(
                formatter,
                "structural member {} still supports mounted equipment",
                element.value()
            ),
            Self::ElementSupportsStockpile { element } => write!(
                formatter,
                "structural member {} still supports a stockpile",
                element.value()
            ),
            Self::ElementSupportsFluidStore { element } => write!(
                formatter,
                "structural member {} still supports a fluid store",
                element.value()
            ),
            Self::UnknownMethod { method } => write!(
                formatter,
                "unknown structural dismantling method {}",
                method.value()
            ),
            Self::MemberTooHeavy { required, maximum } => write!(
                formatter,
                "structural member requires {} mg of recovery work but the method permits at most {} mg",
                required.milligrams(),
                maximum.milligrams()
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("structural dismantling completion tick overflowed")
            }
            Self::RecoveryAccess(error) => write!(
                formatter,
                "structural dismantling recovery stockpile is inaccessible: {error}"
            ),
            Self::RecoveryIngress => formatter.write_str(
                "structural dismantling recovery material cannot enter the selected stockpile",
            ),
            Self::RecoveryReservation => formatter
                .write_str("structural dismantling cannot reserve capacity for recovered material"),
            Self::MaterialLotIdExhausted => formatter
                .write_str("material lot identity space is exhausted for structural dismantling"),
            Self::InventoryRevisionExhausted => formatter
                .write_str("inventory revision space is exhausted for structural dismantling"),
            Self::StructureRevisionExhausted => {
                formatter.write_str("structural revision space is exhausted for dismantling")
            }
            Self::Work(error) => write!(
                formatter,
                "structural dismantling cannot begin player work: {error}"
            ),
        }
    }
}

impl Error for PlayerStructuralDismantlingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RecoveryAccess(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::PlayerDead
            | Self::UnknownElement { .. }
            | Self::ElementNotRecoverable { .. }
            | Self::ElementOutOfReach { .. }
            | Self::ElementHasDependents { .. }
            | Self::ElementHasExternalLoad { .. }
            | Self::ElementSupportsEquipment { .. }
            | Self::ElementSupportsStockpile { .. }
            | Self::ElementSupportsFluidStore { .. }
            | Self::UnknownMethod { .. }
            | Self::MemberTooHeavy { .. }
            | Self::CompletionTickOverflow
            | Self::RecoveryIngress
            | Self::RecoveryReservation
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::StructureRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralDismantlingCommitError {
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
    ElementChanged {
        element: StructuralElementId,
    },
    Work(PlayerWorkCommitError),
}

impl Display for PlayerStructuralDismantlingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structural dismantling expected structure revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "structural dismantling expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "structural dismantling expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::PlayerMoved { expected, actual } => match actual {
                Some(actual) => write!(
                    formatter,
                    "structural dismantling player moved from ({},{},{}) to ({},{},{})",
                    expected.x(),
                    expected.y(),
                    expected.z(),
                    actual.x(),
                    actual.y(),
                    actual.z()
                ),
                None => write!(
                    formatter,
                    "structural dismantling player disappeared from ({},{},{})",
                    expected.x(),
                    expected.y(),
                    expected.z()
                ),
            },
            Self::ElementChanged { element } => write!(
                formatter,
                "structural dismantling member {} changed before commit",
                element.value()
            ),
            Self::Work(error) => write!(
                formatter,
                "structural dismantling player-work authorization became stale: {error}"
            ),
        }
    }
}

impl Error for PlayerStructuralDismantlingCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleStructureRevision { .. }
            | Self::StaleLogisticsRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::PlayerMoved { .. }
            | Self::ElementChanged { .. } => None,
        }
    }
}
