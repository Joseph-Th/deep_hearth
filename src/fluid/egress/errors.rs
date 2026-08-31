//! Diagnostics for exact finite-fluid withdrawal and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Volume;
use crate::structural::StructuralCommitError;

use crate::fluid::structural_integration::FluidStructuralLoadError;
use crate::fluid::{FluidDefinitionId, FluidStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FluidEgressError {
    UnknownStore {
        store: FluidStoreId,
    },
    EmptyStore {
        store: FluidStoreId,
    },
    UnknownFluidDefinition {
        definition: FluidDefinitionId,
    },
    ZeroVolume,
    InsufficientVolume {
        store: FluidStoreId,
        available: Volume,
        requested: Volume,
    },
    RevisionExhausted,
    StructuralLoad(FluidStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FluidEgressCommitError {
    StaleRevision { expected: u64, actual: u64 },
    SourceChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
}

impl Display for FluidEgressCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "validated fluid egress expected revision {expected} but current revision is {actual}"
            ),
            Self::SourceChanged { store } => write!(
                formatter,
                "fluid egress source {} changed without the validated owner revision",
                store.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "fluid egress structural commit failed: {error}")
            }
        }
    }
}

impl Error for FluidEgressCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleRevision { .. } | Self::SourceChanged { .. } => None,
        }
    }
}
