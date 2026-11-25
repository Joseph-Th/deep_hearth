//! Typed excavation diagnostics for shared fixed-footing claims.

use crate::spatial::VoxelCoord;
use crate::world::FixedFootingOwner;

use super::{BlockExcavationCommitError, BlockExcavationError};

impl FixedFootingOwner {
    pub(super) fn admission_error(self, target: VoxelCoord) -> BlockExcavationError {
        match self {
            Self::StorageEnclosure(stockpile) => {
                BlockExcavationError::StorageEnclosureRequiresFloor { target, stockpile }
            }
            Self::OccupiedGroundStockpile(stockpile) => {
                BlockExcavationError::OccupiedGroundStockpileRequiresFloor { target, stockpile }
            }
            Self::LooseSurfaceDrop(resource) => {
                BlockExcavationError::LooseSurfaceDropRequiresFloor { target, resource }
            }
            Self::RootedForage => BlockExcavationError::RootedForageRequiresFloor { target },
            Self::EnergyStore(store) => {
                BlockExcavationError::EnergyStoreRequiresFloor { target, store }
            }
            Self::StationaryEquipment(equipment) => {
                BlockExcavationError::StationaryEquipmentRequiresFloor { target, equipment }
            }
            Self::NaturalSpring(store) => {
                BlockExcavationError::NaturalSpringRequiresFloor { target, store }
            }
            Self::OpeningResident => BlockExcavationError::OpeningResidentRequiresFloor { target },
            Self::GroundedStructure(element) => {
                BlockExcavationError::GroundedStructureRequiresFloor { target, element }
            }
        }
    }

    pub(super) fn commit_error(self, target: VoxelCoord) -> BlockExcavationCommitError {
        match self {
            Self::StorageEnclosure(stockpile) => {
                BlockExcavationCommitError::StorageEnclosureRequiresFloor { target, stockpile }
            }
            Self::OccupiedGroundStockpile(stockpile) => {
                BlockExcavationCommitError::OccupiedGroundStockpileRequiresFloor {
                    target,
                    stockpile,
                }
            }
            Self::LooseSurfaceDrop(resource) => {
                BlockExcavationCommitError::LooseSurfaceDropRequiresFloor { target, resource }
            }
            Self::RootedForage => BlockExcavationCommitError::RootedForageRequiresFloor { target },
            Self::EnergyStore(store) => {
                BlockExcavationCommitError::EnergyStoreRequiresFloor { target, store }
            }
            Self::StationaryEquipment(equipment) => {
                BlockExcavationCommitError::StationaryEquipmentRequiresFloor { target, equipment }
            }
            Self::NaturalSpring(store) => {
                BlockExcavationCommitError::NaturalSpringRequiresFloor { target, store }
            }
            Self::OpeningResident => {
                BlockExcavationCommitError::OpeningResidentRequiresFloor { target }
            }
            Self::GroundedStructure(element) => {
                BlockExcavationCommitError::GroundedStructureRequiresFloor { target, element }
            }
        }
    }
}
