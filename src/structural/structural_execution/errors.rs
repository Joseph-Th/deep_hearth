//! Structural validation and commit failures kept separate from mutation planning mechanics.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Force, Mass};
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;

use super::super::analysis::StructuralAnalysisError;
use super::super::state::{StructuralElementId, StructuralLoadKind};

/// Failure while validating a structural mutation before any authoritative state changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMutationError {
    UnknownElement {
        element: StructuralElementId,
    },
    UnknownSupport {
        support: StructuralElementId,
    },
    ElementFailed {
        element: StructuralElementId,
    },
    ElementSupportsEquipment {
        element: StructuralElementId,
        equipment: EquipmentId,
    },
    ElementSupportsStockpile {
        element: StructuralElementId,
        stockpile: StockpileId,
    },
    ElementSupportsFluidStore {
        element: StructuralElementId,
        store: FluidStoreId,
    },
    ElementOwnsMatter {
        element: StructuralElementId,
        mass: Mass,
    },
    LoadOwnedBySubsystem {
        kind: StructuralLoadKind,
    },
    LoadUnchanged {
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    },
    LoadTargetsRemovedElement {
        element: StructuralElementId,
        kind: StructuralLoadKind,
    },
    SupportFailed {
        support: StructuralElementId,
    },
    GroundedElementCannotHaveSupport {
        element: StructuralElementId,
    },
    SelfSupport {
        element: StructuralElementId,
    },
    SupportOutOfContact {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    DuplicateSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    MissingSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SupportCycle {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
    },
    ActivationUnsupported {
        element: StructuralElementId,
    },
    ActivationUnmaterialized {
        element: StructuralElementId,
    },
    RevisionExhausted,
    Analysis(StructuralAnalysisError),
}

impl Display for StructuralMutationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::UnknownSupport { support } => {
                write!(formatter, "unknown structural support {}", support.value())
            }
            Self::ElementFailed { element } => write!(
                formatter,
                "failed structural element {} cannot be reconfigured",
                element.value()
            ),
            Self::ElementSupportsEquipment { element, equipment } => write!(
                formatter,
                "structural element {} cannot be removed while it supports equipment {}",
                element.value(),
                equipment.value()
            ),
            Self::ElementSupportsStockpile { element, stockpile } => write!(
                formatter,
                "structural element {} cannot be removed while it supports stockpile {}",
                element.value(),
                stockpile.value()
            ),
            Self::ElementSupportsFluidStore { element, store } => write!(
                formatter,
                "structural element {} cannot be removed while it supports fluid store {}",
                element.value(),
                store.value()
            ),
            Self::ElementOwnsMatter { element, mass } => write!(
                formatter,
                "structural element {} owns {} mg of embodied matter and cannot be generically removed; demolition and recovery are not implemented",
                element.value(),
                mass.milligrams()
            ),
            Self::LoadOwnedBySubsystem { kind } => write!(
                formatter,
                "structural {kind:?} load contribution is owned by its source subsystem and cannot be set directly"
            ),
            Self::LoadUnchanged {
                element,
                kind,
                load,
            } => write!(
                formatter,
                "structural {kind:?} load on element {} is already {} mN",
                element.value(),
                load.millinewtons()
            ),
            Self::LoadTargetsRemovedElement { element, kind } => write!(
                formatter,
                "structural {kind:?} load cannot target element {} while that element is removed by the same mutation",
                element.value()
            ),
            Self::SupportFailed { support } => write!(
                formatter,
                "failed structural element {} cannot provide new support",
                support.value()
            ),
            Self::GroundedElementCannotHaveSupport { element } => write!(
                formatter,
                "ground-anchored structural element {} cannot also route load through a member support",
                element.value()
            ),
            Self::SelfSupport { element } => write!(
                formatter,
                "structural element {} cannot support itself",
                element.value()
            ),
            Self::SupportOutOfContact { element, support } => write!(
                formatter,
                "structural support edge {} -> {} cannot cross empty space; the member bounds do not touch or overlap",
                element.value(),
                support.value()
            ),
            Self::DuplicateSupport { element, support } => write!(
                formatter,
                "structural support edge {} -> {} already exists",
                element.value(),
                support.value()
            ),
            Self::MissingSupport { element, support } => write!(
                formatter,
                "structural support edge {} -> {} does not exist",
                element.value(),
                support.value()
            ),
            Self::SupportCycle { element, support } => write!(
                formatter,
                "structural support edge {} -> {} would create a cycle",
                element.value(),
                support.value()
            ),
            Self::ElementNotPlanned { element } => write!(
                formatter,
                "structural element {} is not in planned lifecycle",
                element.value()
            ),
            Self::ActivationUnsupported { element } => write!(
                formatter,
                "structural element {} cannot activate without an active support or ground anchor",
                element.value()
            ),
            Self::ActivationUnmaterialized { element } => write!(
                formatter,
                "structural element {} cannot activate before construction matter is committed",
                element.value()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
            Self::Analysis(error) => write!(formatter, "structural analysis failed: {error}"),
        }
    }
}

impl Error for StructuralMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Analysis(error) => Some(error),
            Self::UnknownElement { .. }
            | Self::UnknownSupport { .. }
            | Self::ElementFailed { .. }
            | Self::ElementSupportsEquipment { .. }
            | Self::ElementSupportsStockpile { .. }
            | Self::ElementSupportsFluidStore { .. }
            | Self::ElementOwnsMatter { .. }
            | Self::LoadOwnedBySubsystem { .. }
            | Self::LoadUnchanged { .. }
            | Self::LoadTargetsRemovedElement { .. }
            | Self::SupportFailed { .. }
            | Self::GroundedElementCannotHaveSupport { .. }
            | Self::SelfSupport { .. }
            | Self::SupportOutOfContact { .. }
            | Self::DuplicateSupport { .. }
            | Self::MissingSupport { .. }
            | Self::SupportCycle { .. }
            | Self::ElementNotPlanned { .. }
            | Self::ActivationUnsupported { .. }
            | Self::ActivationUnmaterialized { .. }
            | Self::RevisionExhausted => None,
        }
    }
}

/// A validated structural token can no longer commit because authoritative state changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralCommitError {
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    StateChanged {
        element: StructuralElementId,
    },
    SupportStateChanged {
        element: StructuralElementId,
        support: StructuralElementId,
    },
}

impl Display for StructuralCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "structural mutation expected revision {expected} but current revision is {actual}"
            ),
            Self::StateChanged { element } => write!(
                formatter,
                "structural element {} changed after validation",
                element.value()
            ),
            Self::SupportStateChanged { element, support } => write!(
                formatter,
                "structural support edge {} -> {} changed after validation",
                element.value(),
                support.value()
            ),
        }
    }
}

impl Error for StructuralCommitError {}
