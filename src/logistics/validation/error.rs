//! Stable typed diagnostics for invalid persisted logistics state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::{StockpileId, StorageDefinitionId};
use crate::material::CommodityKey;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;

use crate::logistics::PlayerQuickAccessSlot;

/// Persistent-state validation failure for player logistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogisticsValidationError {
    UninitializedRevisionNonzero {
        revision: u64,
    },
    InitializedRevisionZero,
    UnknownCarriedStockpile {
        stockpile: StockpileId,
    },
    CarriedStockpileMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    CarriedStockpileEnclosed {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    UnknownLocatedStockpile {
        stockpile: StockpileId,
    },
    StockpilePositionIndexInconsistent,
    LocatedStockpileAlsoCarried {
        stockpile: StockpileId,
    },
    StockpileOutsideSupport {
        stockpile: StockpileId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    StationaryEnclosureIntersectsSolidTerrain {
        voxel: VoxelCoord,
    },
    LocatedFluidStoreIntersectsSolidTerrain {
        voxel: VoxelCoord,
    },
    UnknownLocatedEquipment {
        equipment: EquipmentId,
    },
    EquipmentPositionIndexInconsistent,
    CarriedEquipmentWithoutPlayer {
        equipment: EquipmentId,
    },
    CarriedEquipmentNotStrictlyOrdered {
        previous: EquipmentId,
        current: EquipmentId,
    },
    UnknownCarriedEquipment {
        equipment: EquipmentId,
    },
    CarriedEquipmentAlsoLocated {
        equipment: EquipmentId,
    },
    CarriedEquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    CarriedEquipmentRequiresSupport {
        equipment: EquipmentId,
    },
    QuickAccessSlotOutOfRange {
        slot: u8,
    },
    QuickAccessSelectionWithoutPlayer {
        slot: PlayerQuickAccessSlot,
    },
    QuickAccessEquipmentNotCarried {
        slot: PlayerQuickAccessSlot,
        equipment: EquipmentId,
    },
    QuickAccessMaterialUnsupported {
        slot: PlayerQuickAccessSlot,
        commodity: CommodityKey,
    },
    DuplicateQuickAccessSelection {
        first: PlayerQuickAccessSlot,
        second: PlayerQuickAccessSlot,
    },
    EquipmentOutsideSupport {
        equipment: EquipmentId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    UnknownLocatedEnergyStore {
        store: EnergyStoreId,
    },
    EnergyStorePositionIndexInconsistent,
    UnknownLocatedFluidStore {
        store: FluidStoreId,
    },
    FluidStorePositionIndexInconsistent,
    FluidStoreOutsideSupport {
        store: FluidStoreId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    TraversalEdgeNotCanonical {
        first: VoxelCoord,
        second: VoxelCoord,
    },
    TraversalEdgesNotStrictlyOrdered {
        previous_first: VoxelCoord,
        previous_second: VoxelCoord,
        current_first: VoxelCoord,
        current_second: VoxelCoord,
    },
    TraversalAdjacencyIndexInconsistent,
    PlayerBodyIntersectsSolid {
        voxel: VoxelCoord,
    },
    PlayerBodyIntersectsTempleCaste {
        voxel: VoxelCoord,
    },
    PlayerBodyIntersectsStructure {
        voxel: VoxelCoord,
    },
    PlayerBodyOutsideContinuousWorld,
    PlayerRespawnPositionOutsideContinuousWorld {
        position: VoxelCoord,
    },
    PlayerHorizontalVelocityExceedsMaximumSpeed {
        x_nanometers_per_second: i64,
        z_nanometers_per_second: i64,
        maximum_nanometers_per_second: u64,
    },
    PlayerVerticalVelocityExceedsMaximum {
        nanometers_per_second: i64,
        maximum_nanometers_per_second: u64,
    },
    PlayerLocomotionExertionRemainderInvalid,
    PlayerPendingLocomotionExertionExceedsEnvelope,
    PlayerPendingFallVitalityLossInvalid {
        loss_ppm: u32,
    },
    MultiplePendingDirectActions,
    PendingSecondaryUseWithoutPlayer,
    PendingSecondaryUsePlayerDead,
    PendingSecondaryUseRequestedInFuture,
    PendingSecondaryUseFromPriorSimulationInterval,
    PendingSecondaryUseDirectionInvalid,
    PendingSecondaryUseSurfaceRequestInvalid,
    PendingPrimaryBreakWithoutPlayer,
    PendingPrimaryBreakPlayerDead,
    PendingPrimaryBreakRequestedInFuture,
    PendingPrimaryBreakFromPriorSimulationInterval,
    PendingPrimaryBreakElementInvalid,
    PendingPrimaryBreakDirectionInvalid,
}

impl Display for LogisticsValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UninitializedRevisionNonzero { revision } => write!(
                formatter,
                "uninitialized player logistics has nonzero revision {revision}"
            ),
            Self::InitializedRevisionZero => {
                formatter.write_str("initialized player logistics has zero revision")
            }
            Self::UnknownCarriedStockpile { stockpile } => write!(
                formatter,
                "player logistics references missing carried stockpile {}",
                stockpile.value()
            ),
            Self::CarriedStockpileMounted { stockpile, element } => write!(
                formatter,
                "player-carried stockpile {} is also mounted to structural element {}",
                stockpile.value(),
                element.value()
            ),
            Self::UnknownLocatedEnergyStore { store } => write!(
                formatter,
                "energy-store location references missing store {}",
                store.value()
            ),
            Self::EnergyStorePositionIndexInconsistent => {
                formatter.write_str("energy-store spatial lookup disagrees with location custody")
            }
            Self::UnknownLocatedFluidStore { store } => write!(
                formatter,
                "fluid-store location references missing store {}",
                store.value()
            ),
            Self::FluidStorePositionIndexInconsistent => {
                formatter.write_str("fluid-store spatial lookup disagrees with location custody")
            }
            Self::FluidStoreOutsideSupport {
                store,
                position,
                element,
            } => write!(
                formatter,
                "fluid store {} at voxel ({},{},{}) lies outside structural support {} bounds",
                store.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::UnknownLocatedEquipment { equipment } => write!(
                formatter,
                "equipment location references missing equipment {}",
                equipment.value()
            ),
            Self::EquipmentPositionIndexInconsistent => {
                formatter.write_str("equipment spatial lookup disagrees with location custody")
            }
            Self::CarriedEquipmentWithoutPlayer { equipment } => write!(
                formatter,
                "equipment {} is marked player-carried without a logistics player",
                equipment.value()
            ),
            Self::CarriedEquipmentNotStrictlyOrdered { previous, current } => write!(
                formatter,
                "carried equipment IDs are not strictly ordered: {} then {}",
                previous.value(),
                current.value()
            ),
            Self::UnknownCarriedEquipment { equipment } => write!(
                formatter,
                "player logistics references missing carried equipment {}",
                equipment.value()
            ),
            Self::CarriedEquipmentAlsoLocated { equipment } => write!(
                formatter,
                "equipment {} is both player-carried and separately located",
                equipment.value()
            ),
            Self::CarriedEquipmentMounted { equipment, element } => write!(
                formatter,
                "player-carried equipment {} is mounted to structural element {}",
                equipment.value(),
                element.value()
            ),
            Self::CarriedEquipmentRequiresSupport { equipment } => write!(
                formatter,
                "equipment {} requires structural support and cannot be player-carried",
                equipment.value()
            ),
            Self::QuickAccessSlotOutOfRange { slot } => write!(
                formatter,
                "active quick-access slot {slot} is outside the nine-slot bar"
            ),
            Self::QuickAccessSelectionWithoutPlayer { slot } => write!(
                formatter,
                "quick-access slot {} has a selection without an admitted logistics player",
                slot.value()
            ),
            Self::QuickAccessEquipmentNotCarried { slot, equipment } => write!(
                formatter,
                "quick-access slot {} references equipment {} outside player-carried custody",
                slot.value(),
                equipment.value()
            ),
            Self::QuickAccessMaterialUnsupported { slot, commodity } => write!(
                formatter,
                "quick-access slot {} references material {} form {} which is not an authored commodity",
                slot.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::DuplicateQuickAccessSelection { first, second } => write!(
                formatter,
                "quick-access slots {} and {} reference the same semantic selection",
                first.value(),
                second.value()
            ),
            Self::EquipmentOutsideSupport {
                equipment,
                position,
                element,
            } => write!(
                formatter,
                "equipment {} at voxel ({},{},{}) lies outside structural support {} bounds",
                equipment.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::CarriedStockpileEnclosed {
                stockpile,
                definition,
            } => write!(
                formatter,
                "player-carried stockpile {} also owns stationary storage enclosure {}",
                stockpile.value(),
                definition.value()
            ),
            Self::UnknownLocatedStockpile { stockpile } => write!(
                formatter,
                "stockpile location references missing stockpile {}",
                stockpile.value()
            ),
            Self::StockpilePositionIndexInconsistent => {
                formatter.write_str("stockpile spatial lookup disagrees with location custody")
            }
            Self::LocatedStockpileAlsoCarried { stockpile } => write!(
                formatter,
                "stockpile {} is both player-carried and separately located",
                stockpile.value()
            ),
            Self::StockpileOutsideSupport {
                stockpile,
                position,
                element,
            } => write!(
                formatter,
                "stockpile {} at voxel ({},{},{}) lies outside structural support {} bounds",
                stockpile.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::StationaryEnclosureIntersectsSolidTerrain { voxel } => write!(
                formatter,
                "completed stationary enclosure intersects solid terrain at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::LocatedFluidStoreIntersectsSolidTerrain { voxel } => write!(
                formatter,
                "located fluid vessel intersects solid terrain at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::TraversalEdgeNotCanonical { first, second } => write!(
                formatter,
                "traversal edge ({},{},{}) <-> ({},{},{}) is not a canonical face-adjacent edge",
                first.x(),
                first.y(),
                first.z(),
                second.x(),
                second.y(),
                second.z()
            ),
            Self::TraversalEdgesNotStrictlyOrdered {
                previous_first,
                previous_second,
                current_first,
                current_second,
            } => write!(
                formatter,
                "traversal edges are not strictly ordered: ({},{},{})<->({},{},{}) then ({},{},{})<->({},{},{})",
                previous_first.x(),
                previous_first.y(),
                previous_first.z(),
                previous_second.x(),
                previous_second.y(),
                previous_second.z(),
                current_first.x(),
                current_first.y(),
                current_first.z(),
                current_second.x(),
                current_second.y(),
                current_second.z()
            ),
            Self::TraversalAdjacencyIndexInconsistent => {
                formatter.write_str("traversal adjacency disagrees with authoritative opening edges")
            }
            Self::PlayerBodyIntersectsSolid { voxel } => write!(
                formatter,
                "player body intersects solid world voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::PlayerBodyIntersectsTempleCaste { voxel } => write!(
                formatter,
                "player body intersects temple resident at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::PlayerBodyIntersectsStructure { voxel } => write!(
                formatter,
                "player body intersects completed structural matter at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::PlayerBodyOutsideContinuousWorld => formatter
                .write_str("player body extent exceeds representable continuous world coordinates"),
            Self::PlayerRespawnPositionOutsideContinuousWorld { position } => write!(
                formatter,
                "player respawn position ({},{},{}) exceeds representable continuous world coordinates",
                position.x(),
                position.y(),
                position.z()
            ),
            Self::PlayerHorizontalVelocityExceedsMaximumSpeed {
                x_nanometers_per_second,
                z_nanometers_per_second,
                maximum_nanometers_per_second,
            } => write!(
                formatter,
                "player horizontal velocity ({x_nanometers_per_second},{z_nanometers_per_second}) nm/s exceeds authored maximum locomotion speed {maximum_nanometers_per_second} nm/s",
            ),
            Self::PlayerVerticalVelocityExceedsMaximum {
                nanometers_per_second,
                maximum_nanometers_per_second,
            } => write!(
                formatter,
                "player vertical velocity {nanometers_per_second} nm/s exceeds authored body limit {maximum_nanometers_per_second} nm/s",
            ),
            Self::PlayerLocomotionExertionRemainderInvalid => formatter.write_str(
                "player locomotion exertion fractional carry is outside one control interval",
            ),
            Self::PlayerPendingLocomotionExertionExceedsEnvelope => formatter.write_str(
                "player pending locomotion exertion exceeds the canonical control-interval accumulation envelope",
            ),
            Self::PlayerPendingFallVitalityLossInvalid { loss_ppm } => write!(
                formatter,
                "player pending fall vitality loss {loss_ppm} ppm exceeds the normalized vitality range"
            ),
            Self::MultiplePendingDirectActions => formatter.write_str(
                "player logistics contains more than one buffered direct action",
            ),
            Self::PendingSecondaryUseWithoutPlayer => formatter
                .write_str("buffered contextual use exists without an admitted logistics player"),
            Self::PendingSecondaryUsePlayerDead => {
                formatter.write_str("dead player cannot retain buffered contextual use")
            }
            Self::PendingSecondaryUseRequestedInFuture => formatter
                .write_str("buffered contextual use claims a request control tick in the future"),
            Self::PendingSecondaryUseFromPriorSimulationInterval => formatter.write_str(
                "buffered contextual use survived beyond the simulation interval in which it was requested",
            ),
            Self::PendingSecondaryUseDirectionInvalid => formatter
                .write_str("buffered contextual use contains an invalid fixed-point view direction"),
            Self::PendingSecondaryUseSurfaceRequestInvalid => formatter
                .write_str("buffered explicit surface gathering has a zero resource ID or mass"),
            Self::PendingPrimaryBreakWithoutPlayer => formatter
                .write_str("buffered structural break exists without an admitted logistics player"),
            Self::PendingPrimaryBreakPlayerDead => {
                formatter.write_str("dead player cannot retain buffered structural break")
            }
            Self::PendingPrimaryBreakRequestedInFuture => formatter
                .write_str("buffered structural break claims a request control tick in the future"),
            Self::PendingPrimaryBreakFromPriorSimulationInterval => formatter.write_str(
                "buffered structural break survived beyond the simulation interval in which it was requested",
            ),
            Self::PendingPrimaryBreakElementInvalid => formatter
                .write_str("buffered structural break contains an invalid structural element identity"),
            Self::PendingPrimaryBreakDirectionInvalid => formatter
                .write_str("buffered structural break contains an invalid fixed-point view direction"),
        }
    }
}

impl Error for LogisticsValidationError {}
