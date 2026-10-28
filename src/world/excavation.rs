//! Timed conserved player excavation of targeted block-world matter.

mod runtime;
mod selection;

pub(crate) use runtime::{
    BlockExcavationCancellationPlan, BlockExcavationTickError, apply_block_excavation_cancellation,
    apply_block_excavation_tick, decide_block_excavation_cancellation,
    decide_block_excavation_completion, decide_block_excavation_tick,
};
pub use runtime::{BlockExcavationOutcome, BlockExcavationOutputReceipt};
pub(crate) use selection::validate_start_player_block_excavation_active;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_available_equipment_provider};
use crate::inventory::{
    StockpileId, ValidatedInboundReservation, validate_inbound_reservation,
    validate_stockpile_storage,
};
use crate::labor::{
    BlockExcavationOutputCustody, BlockExcavationWork, PlayerWork, PlayerWorkCommitError,
    PlayerWorkStartError, ValidatedPlayerWorkStart, validate_player_work_start_control_rate,
};
use crate::logistics::{PlayerEquipmentAccessError, validate_player_equipment_access};
use crate::material::CommodityKey;
use crate::mining::{MiningMethodId, MiningPhysicsError, resolve_mining_physics};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};

use super::{BlockStateId, PlayerBlockTarget, PlayerBlockTargetError, raycast_player_block};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockExcavationRequest {
    direction: ViewDirection,
    method: MiningMethodId,
    equipment: EquipmentId,
    mass: Mass,
}

impl BlockExcavationRequest {
    #[must_use]
    pub const fn new(
        direction: ViewDirection,
        method: MiningMethodId,
        equipment: EquipmentId,
        mass: Mass,
    ) -> Self {
        Self {
            direction,
            method,
            equipment,
            mass,
        }
    }
}

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
    StandingTreeRequiresFloor {
        target: VoxelCoord,
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
            Self::StandingTreeRequiresFloor { target } => write!(
                formatter,
                "cannot remove block at ({},{},{}): a rooted tree still needs this floor",
                target.x(),
                target.y(),
                target.z()
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
            | Self::StandingTreeRequiresFloor { .. }
            | Self::UnknownMethod { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentRevisionExhausted
            | Self::WorldRevisionExhausted
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
            | Self::StaleLogisticsRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleSurfaceRevision { .. }
            | Self::StaleSurfaceIdentity { .. }
            | Self::TargetChanged
            | Self::StorageEnclosureRequiresFloor { .. }
            | Self::EquipmentChanged => None,
        }
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockExcavationStartOutcome {
    target: VoxelCoord,
    excavated_mass: Mass,
    output: BlockExcavationOutputCustody,
    completes_at: ControlTick,
}

impl BlockExcavationStartOutcome {
    #[must_use]
    pub const fn target(self) -> VoxelCoord {
        self.target
    }

    #[must_use]
    pub const fn excavated_mass(self) -> Mass {
        self.excavated_mass
    }

    #[must_use]
    pub const fn output(self) -> BlockExcavationOutputCustody {
        self.output
    }

    #[must_use]
    pub const fn completes_at(self) -> ControlTick {
        self.completes_at
    }
}

#[must_use]
pub struct ValidatedPlayerBlockExcavationStart {
    expected_world_revision: u64,
    expected_logistics_revision: u64,
    expected_equipment_revision: u64,
    expected_state: BlockStateId,
    reservation: Option<ValidatedInboundReservation>,
    expected_surface_revision: Option<u64>,
    expected_surface_resource_id: Option<u32>,
    work: BlockExcavationWork,
    player_work: ValidatedPlayerWorkStart,
}

impl ValidatedPlayerBlockExcavationStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<BlockExcavationStartOutcome, BlockExcavationCommitError> {
        let actual_world = state.world().revision();
        if actual_world != self.expected_world_revision {
            return Err(BlockExcavationCommitError::StaleWorldRevision {
                expected: self.expected_world_revision,
                actual: actual_world,
            });
        }
        let actual_logistics = state.logistics().revision();
        if actual_logistics != self.expected_logistics_revision {
            return Err(BlockExcavationCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics,
            });
        }
        let actual_equipment = state.equipment().revision();
        if actual_equipment != self.expected_equipment_revision {
            return Err(BlockExcavationCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_equipment,
            });
        }
        if let Some(reservation) = &self.reservation {
            let actual_inventory = state.inventory().revision();
            if actual_inventory != reservation.expected_revision() {
                return Err(BlockExcavationCommitError::StaleInventoryRevision {
                    expected: reservation.expected_revision(),
                    actual: actual_inventory,
                });
            }
        }
        if let Some(expected) = self.expected_surface_revision {
            let actual = state.surface().revision();
            if actual != expected {
                return Err(BlockExcavationCommitError::StaleSurfaceRevision { expected, actual });
            }
        }
        if let Some(expected) = self.expected_surface_resource_id {
            let actual = state.surface().next_resource_id();
            if actual != expected {
                return Err(BlockExcavationCommitError::StaleSurfaceIdentity { expected, actual });
            }
        }
        if state.world().block_at(self.work.target()) != Some(self.expected_state) {
            return Err(BlockExcavationCommitError::TargetChanged);
        }
        if self.work.excavated_mass() == self.work.source_mass_before()
            && let Some(stockpile) = storage_enclosure_resting_on(state, self.work.target())
        {
            return Err(BlockExcavationCommitError::StorageEnclosureRequiresFloor {
                target: self.work.target(),
                stockpile,
            });
        }
        let Some(equipment) = state.equipment().get_equipment(self.work.equipment()) else {
            return Err(BlockExcavationCommitError::EquipmentChanged);
        };
        if equipment.definition() != self.work.equipment_trace().definition()
            || equipment.condition() != self.work.equipment_trace().condition()
        {
            return Err(BlockExcavationCommitError::EquipmentChanged);
        }
        self.player_work
            .precheck(state)
            .map_err(BlockExcavationCommitError::PlayerWork)?;
        if let Some(reservation) = self.reservation {
            reservation.assert_matches_state(state.inventory());
            reservation.apply(state.inventory_state_mut());
        }
        self.player_work.apply(state);
        Ok(BlockExcavationStartOutcome {
            target: self.work.target(),
            excavated_mass: self.work.excavated_mass(),
            output: self.work.output(),
            completes_at: self.work.completes_at(),
        })
    }
}

pub fn validate_start_player_block_excavation(
    registries: &Registries,
    state: &AppState,
    request: BlockExcavationRequest,
) -> Result<ValidatedPlayerBlockExcavationStart, BlockExcavationError> {
    if request.mass.is_zero() {
        return Err(BlockExcavationError::ZeroMass);
    }
    let target = raycast_player_block(registries, state, request.direction)
        .map_err(BlockExcavationError::Target)?
        .ok_or(BlockExcavationError::NoTarget)?;
    validate_start_player_block_excavation_targeted(registries, state, request, target)
}

/// The target was already resolved against this immutable state; never retrace it for each
/// supported mining method when choosing an admissible direct-action plan.
fn validate_start_player_block_excavation_targeted(
    registries: &Registries,
    state: &AppState,
    request: BlockExcavationRequest,
    target: PlayerBlockTarget,
) -> Result<ValidatedPlayerBlockExcavationStart, BlockExcavationError> {
    let block = registries
        .world()
        .block_for_state(target.state())
        .unwrap_or_else(|| unreachable!("targeted block state is registered"));
    let excavation = block
        .excavation()
        .ok_or(BlockExcavationError::TargetNotExcavatable {
            target: target.voxel(),
        })?;
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target.voxel())
        .unwrap_or_else(|| unreachable!("target ray established generated world truth"));
    if request.mass > source_mass {
        return Err(BlockExcavationError::MassExceedsCell {
            requested: request.mass,
            available: source_mass,
        });
    }
    // Direct enclosure placement demands a solid floor. Removing its last represented
    // milligram must not silently leave a completed storage enclosure suspended in air.
    // Partial excavation does not remove the collidable supporting cell.
    if request.mass == source_mass
        && let Some(stockpile) = storage_enclosure_resting_on(state, target.voxel())
    {
        return Err(BlockExcavationError::StorageEnclosureRequiresFloor {
            target: target.voxel(),
            stockpile,
        });
    }
    if request.mass == source_mass
        && target.voxel().y().checked_add(1).is_some_and(|y| {
            state
                .world()
                .tree_at(VoxelCoord::new(target.voxel().x(), y, target.voxel().z()))
                .is_some()
        })
    {
        return Err(BlockExcavationError::StandingTreeRequiresFloor {
            target: target.voxel(),
        });
    }
    let method = registries.mining().get_method(request.method).ok_or(
        BlockExcavationError::UnknownMethod {
            method: request.method,
        },
    )?;
    validate_player_equipment_access(state, request.equipment)
        .map_err(BlockExcavationError::EquipmentAccess)?;
    let provider = resolve_available_equipment_provider(registries, state, request.equipment)
        .map_err(BlockExcavationError::Equipment)?;
    if state
        .equipment()
        .get_equipment(request.equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(BlockExcavationError::EquipmentMounted {
            equipment: request.equipment,
        });
    }
    let physics = resolve_mining_physics(
        registries.core().physical_tick_duration(),
        method,
        provider.definition(),
        provider.condition(),
        excavation.hardness(),
        request.mass,
    )
    .map_err(BlockExcavationError::Physics)?;
    let completes_at = state
        .control_tick()
        .checked_add_simulation_span(physics.duration())
        .ok_or(BlockExcavationError::ScheduleOverflow)?;
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("targeting requires admitted player"));
    if !state.can_spend_equipment_revisions(1) {
        return Err(BlockExcavationError::EquipmentRevisionExhausted);
    }
    if !state.can_spend_world_revisions(1) {
        return Err(BlockExcavationError::WorldRevisionExhausted);
    }
    let destination = state
        .inventory()
        .get_stockpile(player.carried_stockpile())
        .ok_or(BlockExcavationError::CarriedStockpileMissing)?;
    let output = excavation.fragmented_lot(request.mass);
    let carried_storage_valid = validate_stockpile_storage(
        registries,
        destination,
        player.carried_stockpile(),
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .is_ok();
    let reservation = (carried_storage_valid
        && state.can_spend_inventory_revisions(2)
        && state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1))
    .then(|| {
        validate_inbound_reservation(state.inventory(), player.carried_stockpile(), request.mass)
            .ok()
    })
    .flatten();
    let (output, expected_surface_revision, expected_surface_resource_id) = if reservation.is_some()
    {
        (
            BlockExcavationOutputCustody::Carried {
                stockpile: player.carried_stockpile(),
            },
            None,
            None,
        )
    } else {
        if !state.can_spend_surface_revisions(1) {
            return Err(BlockExcavationError::SurfaceRevisionExhausted);
        }
        if !state.has_surface_resource_id_headroom(1) {
            return Err(BlockExcavationError::SurfaceResourceIdExhausted);
        }
        (
            BlockExcavationOutputCustody::Surface {
                position: player.position(),
            },
            Some(state.surface().revision()),
            Some(state.surface().next_resource_id()),
        )
    };
    let work = BlockExcavationWork::new(
        target.voxel(),
        target.state(),
        source_mass,
        request.mass,
        request.method,
        output,
        provider.validated_use().trace(),
        physics.condition_after(),
        state.control_tick(),
        completes_at,
    );
    let player_work = validate_player_work_start_control_rate(
        registries,
        state,
        PlayerWork::BlockExcavation { work },
        physics.duration(),
        method.exertion(),
    )
    .map_err(BlockExcavationError::PlayerWork)?;
    Ok(ValidatedPlayerBlockExcavationStart {
        expected_world_revision: state.world().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_equipment_revision: state.equipment().revision(),
        expected_state: target.state(),
        reservation,
        expected_surface_revision,
        expected_surface_resource_id,
        work,
        player_work,
    })
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
