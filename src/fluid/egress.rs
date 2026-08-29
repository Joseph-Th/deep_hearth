//! Exact finite-fluid withdrawal into another authoritative owner.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Volume;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralMutationOutcome};

use super::state::{FluidContents, FluidStoreRecord};
use super::structural_integration::{
    FluidContentsChange, FluidStructuralLoadError, ValidatedFluidStructuralLoad,
    validate_fluid_contents_changes,
};
use super::{FluidDefinitionId, FluidStoreId};

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
            Self::StaleRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::SourceChanged { store: _store } => None,
        }
    }
}

/// Consumed proof that exact finite fluid may leave storage for another authoritative owner.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedFluidEgress {
    expected_revision: u64,
    next_revision: u64,
    store: FluidStoreId,
    before: FluidContents,
    after: Option<FluidContents>,
    volume: Volume,
    structural: Option<ValidatedFluidStructuralLoad>,
}

impl ValidatedFluidEgress {
    pub(crate) const fn volume(&self) -> Volume {
        self.volume
    }

    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<Option<StructuralMutationOutcome>, FluidEgressCommitError> {
        let actual_revision = state.fluid().revision();
        if actual_revision != self.expected_revision {
            return Err(FluidEgressCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        if state
            .fluid()
            .get_store(self.store)
            .and_then(FluidStoreRecord::contents)
            != Some(self.before)
        {
            return Err(FluidEgressCommitError::SourceChanged { store: self.store });
        }
        let structural = match self.structural {
            Some(structural) => structural
                .commit(state)
                .map_err(FluidEgressCommitError::Structure)?,
            None => None,
        };
        state
            .fluid_state_mut()
            .apply_egress_contents(self.store, self.after, self.next_revision);
        Ok(structural)
    }
}

pub(crate) fn validate_fluid_egress(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    volume: Volume,
) -> Result<ValidatedFluidEgress, FluidEgressError> {
    if volume.is_zero() {
        return Err(FluidEgressError::ZeroVolume);
    }
    let fluid = state.fluid();
    let record = fluid
        .get_store(store)
        .ok_or(FluidEgressError::UnknownStore { store })?;
    let before = record
        .contents()
        .ok_or(FluidEgressError::EmptyStore { store })?;
    if registries.fluid().get_fluid(before.fluid()).is_none() {
        return Err(FluidEgressError::UnknownFluidDefinition {
            definition: before.fluid(),
        });
    }
    let remaining =
        before
            .volume()
            .checked_sub(volume)
            .ok_or(FluidEgressError::InsufficientVolume {
                store,
                available: before.volume(),
                requested: volume,
            })?;
    let after = if remaining.is_zero() {
        None
    } else {
        Some(FluidContents {
            fluid: before.fluid(),
            volume: remaining,
            temperature: before.temperature(),
        })
    };
    let structural = validate_fluid_contents_changes(
        registries,
        state,
        [FluidContentsChange::new(store, after)],
    )
    .map_err(FluidEgressError::StructuralLoad)?;
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(FluidEgressError::RevisionExhausted)?;
    Ok(ValidatedFluidEgress {
        expected_revision: fluid.revision(),
        next_revision,
        store,
        before,
        after,
        volume,
        structural,
    })
}
