//! Local support owners that prevent complete excavation of a material-backed floor.

use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;

use super::{BlockExcavationCommitError, BlockExcavationError};

/// Every installed floor owner shares one ordered obstruction assessment so
/// admission and stale-commit checks cannot disagree about who holds a cell.
pub(super) enum TerrainFootingOwner {
    StorageEnclosure(StockpileId),
    EnergyStore(crate::energy::EnergyStoreId),
    StationaryEquipment(EquipmentId),
    NaturalSpring(crate::fluid::FluidStoreId),
    OpeningResident,
    GroundedStructure(StructuralElementId),
}

impl TerrainFootingOwner {
    pub(super) fn at(state: &AppState, floor: VoxelCoord) -> Option<Self> {
        storage_enclosure_resting_on(state, floor)
            .map(Self::StorageEnclosure)
            .or_else(|| stationary_energy_store_resting_on(state, floor).map(Self::EnergyStore))
            .or_else(|| {
                stationary_equipment_resting_on(state, floor).map(Self::StationaryEquipment)
            })
            .or_else(|| natural_spring_resting_on(state, floor).map(Self::NaturalSpring))
            .or_else(|| {
                crate::kobolds::opening_kobold_stands_on(state, floor)
                    .then_some(Self::OpeningResident)
            })
            .or_else(|| grounded_structure_resting_on(state, floor).map(Self::GroundedStructure))
    }

    pub(super) fn admission_error(self, target: VoxelCoord) -> BlockExcavationError {
        match self {
            Self::StorageEnclosure(stockpile) => {
                BlockExcavationError::StorageEnclosureRequiresFloor { target, stockpile }
            }
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

/// Natural water is a stationary hollow, not a vessel that can fall after the
/// last supporting block disappears. Ordinary placed fluid stores do not claim
/// terrain footing, because their support is owned by their installation path.
fn natural_spring_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<crate::fluid::FluidStoreId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state
        .logistics()
        .fluid_stores_at(above)
        .find(|&store| state.fluid().is_natural_spring(store))
}

/// The stationary index is authoritative for exact-cell enclosure custody. Loose material and
/// empty ground stockpiles do not require a full-cube terrain floor.
fn storage_enclosure_resting_on(state: &AppState, floor: VoxelCoord) -> Option<StockpileId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().stockpiles_at(above).find(|&id| {
        state
            .inventory()
            .get_stockpile(id)
            .is_some_and(|stockpile| stockpile.enclosure().is_some())
    })
}

/// A material-built energy store cannot stay suspended when its original
/// terrain footing is fully removed. Unembodied capability fixtures do not
/// claim this physical support. Use the indexed local position, not all stores.
fn stationary_energy_store_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<crate::energy::EnergyStoreId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().energy_stores_at(above).find(|&id| {
        state
            .energy()
            .get_store(id)
            .is_some_and(|store| !store.embodied_material().is_empty())
    })
}

/// Unmounted, material-built equipment has no simulated falling motion after
/// it is assembled or dropped. Once mounted, its structural member owns the
/// support instead of the ground directly beneath its current voxel.
fn stationary_equipment_resting_on(state: &AppState, floor: VoxelCoord) -> Option<EquipmentId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state.logistics().equipment_at(above).find(|&id| {
        state
            .equipment()
            .get_equipment(id)
            .is_some_and(|equipment| {
                equipment.supported_by().is_none() && !equipment.embodied_material().is_empty()
            })
    })
}

/// Ground anchors remain attached to their actual supporting terrain until
/// dismantled. An empty blueprint does not own physical matter and cannot
/// reserve a permanent veto over earthworks.
fn grounded_structure_resting_on(
    state: &AppState,
    floor: VoxelCoord,
) -> Option<StructuralElementId> {
    let above = VoxelCoord::new(floor.x(), floor.y().checked_add(1)?, floor.z());
    state
        .structures()
        .elements_covering(above)
        .filter(|member| {
            member.is_grounded()
                && !member.embodied_material().is_empty()
                && member.bounds().min().y() == above.y()
        })
        .map(|member| member.id())
        .min()
}
