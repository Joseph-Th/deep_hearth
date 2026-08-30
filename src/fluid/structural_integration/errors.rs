//! Public validation and commit errors for fluid structural-load and support transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Force;
use crate::structural::{
    StructuralCommitError, StructuralElementId, StructuralLifecycle, StructuralMutationError,
};

use crate::fluid::{FluidDefinitionId, FluidStoreId};

/// Failure while deriving structure-owned load from supported finite fluid ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidStructuralLoadError {
    UnknownStore {
        store: FluidStoreId,
    },
    UnknownSupport {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    UnknownFluidDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
    SupportNotActiveForIncrease {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    Structure(StructuralMutationError),
}

impl Display for FluidStructuralLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown fluid store {}", store.value())
            }
            Self::UnknownSupport { store, element } => write!(
                formatter,
                "fluid store {} references missing structural support {}",
                store.value(),
                element.value()
            ),
            Self::UnknownFluidDefinition { store, definition } => write!(
                formatter,
                "fluid store {} references missing fluid definition {} while deriving structural load",
                store.value(),
                definition.value()
            ),
            Self::SupportNotActiveForIncrease { element, lifecycle } => write!(
                formatter,
                "aggregate fluid weight cannot increase while structural support {} is {lifecycle:?}",
                element.value()
            ),
            Self::AggregateMassOverflow { element } => write!(
                formatter,
                "supported fluid mass calculation overflowed on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "supported fluid weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN fluid load but supported fluid ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::Structure(error) => write!(formatter, "fluid structural load failed: {error}"),
        }
    }
}

impl Error for FluidStructuralLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::UnknownSupport { .. }
            | Self::UnknownFluidDefinition { .. }
            | Self::SupportNotActiveForIncrease { .. }
            | Self::AggregateMassOverflow { .. }
            | Self::WeightForceOverflow { .. }
            | Self::ExistingLoadMismatch { .. } => None,
        }
    }
}

/// Failure while assigning or removing a fluid store's structural support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidSupportError {
    UnknownStore {
        store: FluidStoreId,
    },
    AlreadyMounted {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    NotMounted {
        store: FluidStoreId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    FluidRevisionExhausted,
    Load(FluidStructuralLoadError),
}

impl Display for FluidSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown fluid store {}", store.value())
            }
            Self::AlreadyMounted { store, element } => write!(
                formatter,
                "fluid store {} is already supported by structural element {}",
                store.value(),
                element.value()
            ),
            Self::NotMounted { store } => write!(
                formatter,
                "fluid store {} has no structural support assignment to remove",
                store.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive a fluid store",
                element.value()
            ),
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
            Self::Load(error) => write!(formatter, "fluid store support load failed: {error}"),
        }
    }
}

impl Error for FluidSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::AlreadyMounted { .. }
            | Self::NotMounted { .. }
            | Self::TargetNotActive { .. }
            | Self::FluidRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound fluid-store support transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidSupportCommitError {
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: FluidStoreId,
    },
    SupportChanged {
        store: FluidStoreId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    Structure(StructuralCommitError),
}

impl Display for FluidSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "validated fluid support change expected fluid revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "fluid store {} disappeared before support commit",
                store.value()
            ),
            Self::SupportChanged {
                store,
                expected,
                actual,
            } => write!(
                formatter,
                "fluid store {} support changed from expected {expected:?} to {actual:?} before commit",
                store.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "fluid store support structural commit failed: {error}"
            ),
        }
    }
}

impl Error for FluidSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleFluidRevision { .. }
            | Self::UnknownStore { .. }
            | Self::SupportChanged { .. } => None,
        }
    }
}
