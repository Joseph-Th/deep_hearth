//! Bootstrap placement for finite fluid stores without implying transport or pumping.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidStorePlacementError {
    UnknownStore {
        store: FluidStoreId,
    },
    AlreadyLocated {
        store: FluidStoreId,
        position: VoxelCoord,
    },
    OutsideStructuralSupport {
        store: FluidStoreId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    LogisticsRevisionExhausted,
}

impl Display for FluidStorePlacementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown fluid store {}", store.value())
            }
            Self::AlreadyLocated { store, position } => write!(
                formatter,
                "fluid store {} is already located at voxel ({},{},{})",
                store.value(),
                position.x(),
                position.y(),
                position.z()
            ),
            Self::OutsideStructuralSupport {
                store,
                position,
                element,
            } => write!(
                formatter,
                "fluid store {} cannot be placed at voxel ({},{},{}) outside structural support {} bounds",
                store.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for FluidStorePlacementError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidStorePlacementCommitError {
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: FluidStoreId,
    },
    AlreadyLocated {
        store: FluidStoreId,
    },
    OutsideStructuralSupport {
        store: FluidStoreId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
}

impl Display for FluidStorePlacementCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "fluid-store placement expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "fluid-store placement expected fluid revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "fluid store {} disappeared before placement commit",
                store.value()
            ),
            Self::AlreadyLocated { store } => write!(
                formatter,
                "fluid store {} gained a location before placement commit",
                store.value()
            ),
            Self::OutsideStructuralSupport {
                store,
                position,
                element,
            } => write!(
                formatter,
                "fluid store {} cannot commit location ({},{},{}) outside structural support {} bounds",
                store.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
        }
    }
}

impl Error for FluidStorePlacementCommitError {}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedFluidStorePlacement {
    store: FluidStoreId,
    position: VoxelCoord,
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_fluid_revision: u64,
}

impl ValidatedFluidStorePlacement {
    pub fn commit(self, state: &mut AppState) -> Result<(), FluidStorePlacementCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(FluidStorePlacementCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_fluid_revision = state.fluid().revision();
        if actual_fluid_revision != self.expected_fluid_revision {
            return Err(FluidStorePlacementCommitError::StaleFluidRevision {
                expected: self.expected_fluid_revision,
                actual: actual_fluid_revision,
            });
        }
        if state.logistics().fluid_store_position(self.store).is_some() {
            return Err(FluidStorePlacementCommitError::AlreadyLocated { store: self.store });
        }
        let record = state
            .fluid()
            .get_store(self.store)
            .ok_or(FluidStorePlacementCommitError::UnknownStore { store: self.store })?;
        if let Some(element) = record.supported_by()
            && state
                .structures()
                .get_element(element)
                .is_some_and(|support| !support.bounds().has_voxel(self.position))
        {
            return Err(FluidStorePlacementCommitError::OutsideStructuralSupport {
                store: self.store,
                position: self.position,
                element,
            });
        }
        state.logistics_state_mut().apply_fluid_store_placement(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            self.store,
            self.position,
        );
        Ok(())
    }
}

/// Binds one existing finite fluid store to a world voxel without moving fluid or creating matter.
pub fn validate_place_fluid_store(
    state: &AppState,
    store: FluidStoreId,
    position: VoxelCoord,
) -> Result<ValidatedFluidStorePlacement, FluidStorePlacementError> {
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(FluidStorePlacementError::UnknownStore { store })?;
    if let Some(existing) = state.logistics().fluid_store_position(store) {
        return Err(FluidStorePlacementError::AlreadyLocated {
            store,
            position: existing,
        });
    }
    if let Some(element) = record.supported_by()
        && state
            .structures()
            .get_element(element)
            .is_some_and(|support| !support.bounds().has_voxel(position))
    {
        return Err(FluidStorePlacementError::OutsideStructuralSupport {
            store,
            position,
            element,
        });
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(FluidStorePlacementError::LogisticsRevisionExhausted)?;
    Ok(ValidatedFluidStorePlacement {
        store,
        position,
        expected_logistics_revision,
        next_logistics_revision,
        expected_fluid_revision: state.fluid().revision(),
    })
}
