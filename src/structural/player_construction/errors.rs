//! Typed admission and commit failures for player structural construction.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError, StructuralConstructionMethodId};
use crate::spatial::VoxelCoord;

use super::super::{
    StructuralConstructionBindingError, StructuralConstructionCommitError,
    StructuralConstructionError, StructuralElementId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralConstructionError {
    PlayerNotAdmitted,
    PlayerDead,
    UnknownMethod {
        method: StructuralConstructionMethodId,
    },
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
    },
    ElementUnsupported {
        element: StructuralElementId,
    },
    ElementOutOfReach {
        element: StructuralElementId,
    },
    MemberTooHeavy {
        required: Mass,
        maximum: Mass,
    },
    CompletionTickOverflow,
    UnexpectedMaterialSelection,
    MaterialStateChanged {
        element: StructuralElementId,
    },
    StructureRevisionExhausted,
    Binding(StructuralConstructionBindingError),
    Material(StructuralConstructionError),
    Work(PlayerWorkStartError),
}

impl Display for PlayerStructuralConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("structural construction requires an admitted player"),
            Self::PlayerDead => formatter.write_str("dead player cannot construct structure"),
            Self::UnknownMethod { method } => write!(formatter, "unknown structural construction method {}", method.value()),
            Self::UnknownElement { element } => write!(formatter, "unknown structural element {}", element.value()),
            Self::ElementNotPlanned { element } => write!(formatter, "structural element {} is not awaiting construction", element.value()),
            Self::ElementUnsupported { element } => write!(formatter, "structural element {} is neither grounded nor attached to an active support", element.value()),
            Self::ElementOutOfReach { element } => write!(formatter, "structural element {} is outside player interaction reach", element.value()),
            Self::MemberTooHeavy { required, maximum } => write!(
                formatter,
                "structural member requires {} mg but the construction method permits at most {} mg",
                required.milligrams(), maximum.milligrams()
            ),
            Self::CompletionTickOverflow => formatter.write_str("structural construction completion tick overflowed"),
            Self::UnexpectedMaterialSelection => formatter.write_str("already-materialized structural construction cannot accept another material selection"),
            Self::MaterialStateChanged { element } => write!(formatter, "structural element {} material state is inconsistent with its geometry", element.value()),
            Self::StructureRevisionExhausted => formatter.write_str("structural revision space is exhausted for construction"),
            Self::Binding(error) => write!(formatter, "structural construction material selection is invalid: {error}"),
            Self::Material(error) => write!(formatter, "structural construction materialization failed: {error}"),
            Self::Work(error) => write!(formatter, "structural construction cannot reserve player work: {error}"),
        }
    }
}

impl Error for PlayerStructuralConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Binding(error) => Some(error),
            Self::Material(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::PlayerDead
            | Self::UnknownMethod { .. }
            | Self::UnknownElement { .. }
            | Self::ElementNotPlanned { .. }
            | Self::ElementUnsupported { .. }
            | Self::ElementOutOfReach { .. }
            | Self::MemberTooHeavy { .. }
            | Self::CompletionTickOverflow
            | Self::UnexpectedMaterialSelection
            | Self::MaterialStateChanged { .. }
            | Self::StructureRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralConstructionCommitError {
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogistics {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
    Material(StructuralConstructionCommitError),
    Work(PlayerWorkCommitError),
}

impl Display for PlayerStructuralConstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before structural construction commit"
            ),
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "structural construction logistics revision changed from {expected} to {actual} before commit"
            ),
            Self::PlayerMoved {
                expected,
                actual: Some(actual),
            } => write!(
                formatter,
                "player moved from expected construction position ({},{},{}) to ({},{},{}) before commit",
                expected.x(),
                expected.y(),
                expected.z(),
                actual.x(),
                actual.y(),
                actual.z()
            ),
            Self::PlayerMoved {
                expected,
                actual: None,
            } => write!(
                formatter,
                "player disappeared from expected construction position ({},{},{}) before commit",
                expected.x(),
                expected.y(),
                expected.z()
            ),
            Self::Material(error) => write!(
                formatter,
                "structural construction material commit failed: {error}"
            ),
            Self::Work(error) => write!(
                formatter,
                "structural construction player-work commit failed: {error}"
            ),
        }
    }
}

impl Error for PlayerStructuralConstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Material(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::StaleWorldRevision { .. }
            | Self::StaleLogistics { .. }
            | Self::PlayerMoved { .. } => None,
        }
    }
}
