//! Typed block-excavation admission and stale-commit errors.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::energy::EnergyStoreId;
use crate::equipment::{EquipmentId, EquipmentProviderError};
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::logistics::PlayerEquipmentAccessError;
use crate::material::CommodityKey;
use crate::mining::{MiningMethodId, MiningPhysicsError};
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;
use crate::surface::SurfaceResourceId;

use super::super::PlayerBlockTargetError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockExcavationError {
    Target(PlayerBlockTargetError),
    NoTarget,
    TargetNotExcavatable {
        target: VoxelCoord,
    },
    NoActiveEquipment,
    ActiveSelectionNotEquipment {
        commodity: CommodityKey,
    },
    NoApplicableMethod {
        equipment: EquipmentId,
    },
    ZeroMass,
    MassExceedsCell {
        requested: Mass,
        available: Mass,
    },
    StorageEnclosureRequiresFloor {
        target: VoxelCoord,
        stockpile: StockpileId,
    },
    OccupiedGroundStockpileRequiresFloor {
        target: VoxelCoord,
        stockpile: StockpileId,
    },
    LooseSurfaceDropRequiresFloor {
        target: VoxelCoord,
        resource: SurfaceResourceId,
    },
    RootedForageRequiresFloor {
        target: VoxelCoord,
    },
    StandingTreeRequiresFloor {
        target: VoxelCoord,
    },
    EnergyStoreRequiresFloor {
        target: VoxelCoord,
        store: EnergyStoreId,
    },
    StationaryEquipmentRequiresFloor {
        target: VoxelCoord,
        equipment: EquipmentId,
    },
    NaturalSpringRequiresFloor {
        target: VoxelCoord,
        store: FluidStoreId,
    },
    OpeningResidentRequiresFloor {
        target: VoxelCoord,
    },
    GroundedStructureRequiresFloor {
        target: VoxelCoord,
        element: StructuralElementId,
    },
    UnknownMethod {
        method: MiningMethodId,
    },
    EquipmentAccess(PlayerEquipmentAccessError),
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    Physics(MiningPhysicsError),
    PlayerWork(PlayerWorkStartError),
    EquipmentRevisionExhausted,
    WorldRevisionExhausted,
    OverflowOutputRequiresFixedFloor,
    SurfaceRevisionExhausted,
    SurfaceResourceIdExhausted,
    CarriedStockpileMissing,
    ScheduleOverflow,
}

impl Display for BlockExcavationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Target(error) => write!(formatter, "block target failed: {error}"),
            Self::NoTarget => {
                formatter.write_str("no excavatable block lies under the player view")
            }
            Self::TargetNotExcavatable { target } => write!(
                formatter,
                "target block at ({},{},{}) has no excavation definition",
                target.x(),
                target.y(),
                target.z()
            ),
            Self::NoActiveEquipment => {
                formatter.write_str("block excavation requires active player-carried equipment")
            }
            Self::ActiveSelectionNotEquipment { commodity } => write!(
                formatter,
                "active material {} form {} cannot excavate blocks without selected equipment",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::NoApplicableMethod { equipment } => write!(
                formatter,
                "active equipment {} provides no applicable block-excavation method",
                equipment.value()
            ),
            Self::ZeroMass => formatter.write_str("block excavation mass must be nonzero"),
            Self::MassExceedsCell {
                requested,
                available,
            } => write!(
                formatter,
                "block excavation requests {} mg but the targeted cell contains {} mg",
                requested.milligrams(),
                available.milligrams()
            ),
            Self::StorageEnclosureRequiresFloor { target, stockpile } => write!(
                formatter,
                "cannot remove block at ({},{},{}): storage enclosure {} still rests above it",
                target.x(),
                target.y(),
                target.z(),
                stockpile.value()
            ),
            Self::OccupiedGroundStockpileRequiresFloor { target, stockpile } => write!(
                formatter,
                "cannot remove block at ({},{},{}): populated ground cache {} still rests above it",
                target.x(), target.y(), target.z(), stockpile.value()
            ),
            Self::LooseSurfaceDropRequiresFloor { target, resource } => write!(
                formatter,
                "cannot remove block at ({},{},{}): loose parcel {} still rests above it",
                target.x(), target.y(), target.z(), resource.value()
            ),
            Self::RootedForageRequiresFloor { target } => write!(
                formatter,
                "cannot remove block at ({},{},{}): rooted forage still depends on this ground",
                target.x(), target.y(), target.z()
            ),
            Self::StandingTreeRequiresFloor { target } => write!(
                formatter,
                "cannot remove block at ({},{},{}): a rooted tree still needs this floor",
                target.x(),
                target.y(),
                target.z()
            ),
            Self::EnergyStoreRequiresFloor { target, store } => write!(
                formatter,
                "cannot remove block at ({},{},{}): stationary energy store {} still rests above it",
                target.x(),
                target.y(),
                target.z(),
                store.value()
            ),
            Self::StationaryEquipmentRequiresFloor { target, equipment } => write!(
                formatter,
                "cannot remove block at ({},{},{}): stationary equipment {} still rests above it",
                target.x(),
                target.y(),
                target.z(),
                equipment.value()
            ),
            Self::NaturalSpringRequiresFloor { target, store } => write!(
                formatter,
                "cannot remove block at ({},{},{}): natural spring {} requires its stone hollow",
                target.x(),
                target.y(),
                target.z(),
                store.value()
            ),
            Self::OpeningResidentRequiresFloor { target } => write!(
                formatter,
                "cannot remove block at ({},{},{}): an opening resident still stands above it",
                target.x(),
                target.y(),
                target.z()
            ),
            Self::GroundedStructureRequiresFloor { target, element } => write!(
                formatter,
                "cannot remove block at ({},{},{}): grounded member {} bears on it; dismantle the member first",
                target.x(),
                target.y(),
                target.z(),
                element.value()
            ),
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown mining method {}", method.value())
            }
            Self::EquipmentAccess(error) => {
                write!(formatter, "excavation equipment access failed: {error}")
            }
            Self::Equipment(error) => write!(formatter, "excavation equipment failed: {error}"),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "equipment {} is structurally mounted and cannot be used for direct block excavation",
                equipment.value()
            ),
            Self::Physics(error) => write!(formatter, "block excavation physics failed: {error}"),
            Self::PlayerWork(error) => write!(formatter, "block excavation labor failed: {error}"),
            Self::EquipmentRevisionExhausted => formatter
                .write_str("equipment cannot reserve excavation wear within revision capacity"),
            Self::WorldRevisionExhausted => formatter
                .write_str("world cannot reserve excavation completion within revision capacity"),
            Self::OverflowOutputRequiresFixedFloor => formatter.write_str(
                "excavated overflow cannot become a ground drop without fixed footing beneath the player",
            ),
            Self::SurfaceRevisionExhausted => formatter.write_str(
                "surface state cannot reserve overflow excavation output within revision capacity",
            ),
            Self::SurfaceResourceIdExhausted => formatter.write_str(
                "surface resource identity space cannot reserve overflow excavation output",
            ),
            Self::CarriedStockpileMissing => {
                formatter.write_str("player carried stockpile disappeared before excavation start")
            }
            Self::ScheduleOverflow => {
                formatter.write_str("block excavation completion exceeds authoritative time")
            }
        }
    }
}

impl Error for BlockExcavationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Target(error) => Some(error),
            Self::EquipmentAccess(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Physics(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::NoTarget
            | Self::TargetNotExcavatable { .. }
            | Self::NoActiveEquipment
            | Self::ActiveSelectionNotEquipment { .. }
            | Self::NoApplicableMethod { .. }
            | Self::ZeroMass
            | Self::MassExceedsCell { .. }
            | Self::StorageEnclosureRequiresFloor { .. }
            | Self::OccupiedGroundStockpileRequiresFloor { .. }
            | Self::LooseSurfaceDropRequiresFloor { .. }
            | Self::RootedForageRequiresFloor { .. }
            | Self::StandingTreeRequiresFloor { .. }
            | Self::EnergyStoreRequiresFloor { .. }
            | Self::StationaryEquipmentRequiresFloor { .. }
            | Self::NaturalSpringRequiresFloor { .. }
            | Self::OpeningResidentRequiresFloor { .. }
            | Self::GroundedStructureRequiresFloor { .. }
            | Self::UnknownMethod { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentRevisionExhausted
            | Self::WorldRevisionExhausted
            | Self::OverflowOutputRequiresFixedFloor
            | Self::SurfaceRevisionExhausted
            | Self::SurfaceResourceIdExhausted
            | Self::CarriedStockpileMissing
            | Self::ScheduleOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockExcavationCommitError {
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurfaceRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurfaceIdentity {
        expected: u32,
        actual: u32,
    },
    TargetChanged,
    StorageEnclosureRequiresFloor {
        target: VoxelCoord,
        stockpile: StockpileId,
    },
    OccupiedGroundStockpileRequiresFloor {
        target: VoxelCoord,
        stockpile: StockpileId,
    },
    LooseSurfaceDropRequiresFloor {
        target: VoxelCoord,
        resource: SurfaceResourceId,
    },
    RootedForageRequiresFloor {
        target: VoxelCoord,
    },
    EnergyStoreRequiresFloor {
        target: VoxelCoord,
        store: EnergyStoreId,
    },
    StationaryEquipmentRequiresFloor {
        target: VoxelCoord,
        equipment: EquipmentId,
    },
    NaturalSpringRequiresFloor {
        target: VoxelCoord,
        store: FluidStoreId,
    },
    OpeningResidentRequiresFloor {
        target: VoxelCoord,
    },
    GroundedStructureRequiresFloor {
        target: VoxelCoord,
        element: StructuralElementId,
    },
    EquipmentChanged,
    PlayerWork(PlayerWorkCommitError),
}

impl Display for BlockExcavationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before block excavation start"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structure revision changed from {expected} to {actual} before overflow excavation start"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before block excavation start"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "equipment revision changed from {expected} to {actual} before block excavation start"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "inventory revision changed from {expected} to {actual} before block excavation start"
            ),
            Self::StaleSurfaceRevision { expected, actual } => write!(
                formatter,
                "surface revision changed from {expected} to {actual} before block excavation start"
            ),
            Self::StaleSurfaceIdentity { expected, actual } => write!(
                formatter,
                "surface resource identity cursor changed from {expected} to {actual} before block excavation start"
            ),
            Self::TargetChanged => {
                formatter.write_str("block excavation target changed before start")
            }
            Self::StorageEnclosureRequiresFloor { target, stockpile } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): enclosure {} was placed above it",
                target.x(),
                target.y(),
                target.z(),
                stockpile.value()
            ),
            Self::OccupiedGroundStockpileRequiresFloor { target, stockpile } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): cache {} holds material above it",
                target.x(),
                target.y(),
                target.z(),
                stockpile.value()
            ),
            Self::LooseSurfaceDropRequiresFloor { target, resource } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): parcel {} was left above it",
                target.x(),
                target.y(),
                target.z(),
                resource.value()
            ),
            Self::RootedForageRequiresFloor { target } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): living forage roots remain above it",
                target.x(),
                target.y(),
                target.z()
            ),
            Self::EnergyStoreRequiresFloor { target, store } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): stationary energy store {} was placed above it",
                target.x(),
                target.y(),
                target.z(),
                store.value()
            ),
            Self::StationaryEquipmentRequiresFloor { target, equipment } => write!(
                formatter,
                "cannot remove block at ({},{},{}): stationary equipment {} still rests above it",
                target.x(),
                target.y(),
                target.z(),
                equipment.value()
            ),
            Self::NaturalSpringRequiresFloor { target, store } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): natural spring {} requires its hollow",
                target.x(),
                target.y(),
                target.z(),
                store.value()
            ),
            Self::OpeningResidentRequiresFloor { target } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): an opening resident stands above it",
                target.x(),
                target.y(),
                target.z()
            ),
            Self::GroundedStructureRequiresFloor { target, element } => write!(
                formatter,
                "block excavation cannot remove floor ({},{},{}): grounded member {} bears on it",
                target.x(),
                target.y(),
                target.z(),
                element.value()
            ),
            Self::EquipmentChanged => {
                formatter.write_str("block excavation equipment changed before start")
            }
            Self::PlayerWork(error) => {
                write!(
                    formatter,
                    "block excavation player-work start failed: {error}"
                )
            }
        }
    }
}

impl Error for BlockExcavationCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerWork(error) => Some(error),
            Self::StaleWorldRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::StaleLogisticsRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleSurfaceRevision { .. }
            | Self::StaleSurfaceIdentity { .. }
            | Self::TargetChanged
            | Self::StorageEnclosureRequiresFloor { .. }
            | Self::OccupiedGroundStockpileRequiresFloor { .. }
            | Self::LooseSurfaceDropRequiresFloor { .. }
            | Self::RootedForageRequiresFloor { .. }
            | Self::EnergyStoreRequiresFloor { .. }
            | Self::StationaryEquipmentRequiresFloor { .. }
            | Self::NaturalSpringRequiresFloor { .. }
            | Self::OpeningResidentRequiresFloor { .. }
            | Self::GroundedStructureRequiresFloor { .. }
            | Self::EquipmentChanged => None,
        }
    }
}
