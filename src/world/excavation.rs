//! Timed conserved player excavation of targeted block-world matter.

mod runtime;

pub use runtime::BlockExcavationOutcome;
pub(crate) use runtime::{
    BlockExcavationCancellationPlan, BlockExcavationTickError, apply_block_excavation_cancellation,
    apply_block_excavation_tick, decide_block_excavation_cancellation,
    decide_block_excavation_tick,
};

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityValue, CapabilityValueKind};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentProviderError, resolve_available_equipment_provider};
use crate::inventory::{
    InboundReservationError, StockpileStorageError, ValidatedInboundReservation,
    validate_inbound_reservation, validate_stockpile_storage,
};
use crate::labor::{
    BlockExcavationWork, PlayerWork, PlayerWorkCommitError, PlayerWorkStartError,
    ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::logistics::{PlayerEquipmentAccessError, validate_player_equipment_access};
use crate::material::MaterialLotSpec;
use crate::mining::{MiningMethodId, MiningPhysicsError, resolve_mining_physics};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};

use super::{BlockStateId, PlayerBlockTargetError, raycast_player_block};

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
    TargetNotExcavatable { target: VoxelCoord },
    ZeroMass,
    MassExceedsCell { requested: Mass, available: Mass },
    UnknownMethod { method: MiningMethodId },
    EquipmentAccess(PlayerEquipmentAccessError),
    Equipment(EquipmentProviderError),
    EquipmentMounted { equipment: EquipmentId },
    Physics(MiningPhysicsError),
    PlayerWork(PlayerWorkStartError),
    InventoryRevisionExhausted,
    EquipmentRevisionExhausted,
    WorldRevisionExhausted,
    MaterialLotIdExhausted,
    CarriedStockpileMissing,
    CarriedMassOverflow,
    CarriedCapacityExceeded { requested: Mass, available: Mass },
    CarriedStorage(StockpileStorageError),
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
            Self::InventoryRevisionExhausted => formatter.write_str(
                "inventory cannot reserve and settle excavated material within revision capacity",
            ),
            Self::EquipmentRevisionExhausted => formatter
                .write_str("equipment cannot reserve excavation wear within revision capacity"),
            Self::WorldRevisionExhausted => formatter
                .write_str("world cannot reserve excavation completion within revision capacity"),
            Self::MaterialLotIdExhausted => {
                formatter.write_str("material-lot identity space cannot reserve excavated output")
            }
            Self::CarriedStockpileMissing => {
                formatter.write_str("player carried stockpile disappeared before excavation start")
            }
            Self::CarriedMassOverflow => {
                formatter.write_str("player carried stockpile mass accounting overflowed")
            }
            Self::CarriedCapacityExceeded {
                requested,
                available,
            } => write!(
                formatter,
                "excavated output requires {} mg of carried capacity but only {} mg remains",
                requested.milligrams(),
                available.milligrams()
            ),
            Self::CarriedStorage(error) => {
                write!(
                    formatter,
                    "player carried storage rejects excavated material: {error}"
                )
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
            Self::CarriedStorage(error) => Some(error),
            Self::NoTarget
            | Self::TargetNotExcavatable { .. }
            | Self::ZeroMass
            | Self::MassExceedsCell { .. }
            | Self::UnknownMethod { .. }
            | Self::EquipmentMounted { .. }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentRevisionExhausted
            | Self::WorldRevisionExhausted
            | Self::MaterialLotIdExhausted
            | Self::CarriedStockpileMissing
            | Self::CarriedMassOverflow
            | Self::CarriedCapacityExceeded { .. }
            | Self::ScheduleOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockExcavationCommitError {
    StaleWorldRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleEquipmentRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    TargetChanged,
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
            Self::TargetChanged => {
                formatter.write_str("block excavation target changed before start")
            }
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
            | Self::TargetChanged
            | Self::EquipmentChanged => None,
        }
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockExcavationStartOutcome {
    target: VoxelCoord,
    excavated_mass: Mass,
    completes_at: SimulationTick,
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
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

#[must_use]
pub struct ValidatedPlayerBlockExcavationStart {
    expected_world_revision: u64,
    expected_logistics_revision: u64,
    expected_equipment_revision: u64,
    expected_state: BlockStateId,
    reservation: ValidatedInboundReservation,
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
        let actual_inventory = state.inventory().revision();
        if actual_inventory != self.reservation.expected_revision() {
            return Err(BlockExcavationCommitError::StaleInventoryRevision {
                expected: self.reservation.expected_revision(),
                actual: actual_inventory,
            });
        }
        if state.world().block_at(self.work.target()) != Some(self.expected_state) {
            return Err(BlockExcavationCommitError::TargetChanged);
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
        self.reservation.assert_matches_state(state.inventory());
        self.reservation.apply(state.inventory_state_mut());
        self.player_work.apply(state);
        Ok(BlockExcavationStartOutcome {
            target: self.work.target(),
            excavated_mass: self.work.excavated_mass(),
            completes_at: self.work.completes_at(),
        })
    }
}

fn map_reservation_error(error: InboundReservationError) -> BlockExcavationError {
    match error {
        InboundReservationError::RevisionExhausted => {
            BlockExcavationError::InventoryRevisionExhausted
        }
        InboundReservationError::CapacityExceeded {
            requested,
            capacity,
            committed,
            ..
        } => BlockExcavationError::CarriedCapacityExceeded {
            requested,
            available: capacity.checked_sub(committed).unwrap_or(Mass::ZERO),
        },
        InboundReservationError::UnknownStockpile { .. } => {
            BlockExcavationError::CarriedStockpileMissing
        }
        InboundReservationError::MassOverflow { .. } => BlockExcavationError::CarriedMassOverflow,
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
        .tick()
        .checked_add_span(physics.duration())
        .ok_or(BlockExcavationError::ScheduleOverflow)?;
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("targeting requires admitted player"));
    if !state.can_spend_inventory_revisions(2) {
        return Err(BlockExcavationError::InventoryRevisionExhausted);
    }
    if !state.can_spend_equipment_revisions(1) {
        return Err(BlockExcavationError::EquipmentRevisionExhausted);
    }
    if !state.can_spend_world_revisions(1) {
        return Err(BlockExcavationError::WorldRevisionExhausted);
    }
    if !state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1) {
        return Err(BlockExcavationError::MaterialLotIdExhausted);
    }
    let destination = state
        .inventory()
        .get_stockpile(player.carried_stockpile())
        .ok_or(BlockExcavationError::CarriedStockpileMissing)?;
    let output = MaterialLotSpec::new(
        excavation.fragmented_commodity(),
        request.mass,
        excavation.temperature(),
    );
    validate_stockpile_storage(
        registries,
        destination,
        player.carried_stockpile(),
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .map_err(BlockExcavationError::CarriedStorage)?;
    let reservation =
        validate_inbound_reservation(state.inventory(), player.carried_stockpile(), request.mass)
            .map_err(map_reservation_error)?;
    let work = BlockExcavationWork::new(
        target.voxel(),
        target.state(),
        source_mass,
        request.mass,
        request.method,
        player.carried_stockpile(),
        provider.validated_use().trace(),
        physics.condition_after(),
        state.tick(),
        completes_at,
    );
    let player_work = validate_player_work_start(
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
        work,
        player_work,
    })
}

/// Validates one familiar primary-action excavation batch using the largest currently useful
/// batch supported by the selected tool and targeted cell.
///
/// This is a convenience admission surface for held direct input. Exact planning callers retain
/// [`BlockExcavationRequest`] when they need to choose a smaller explicit mass.
pub(crate) fn validate_start_player_block_excavation_available(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
    method: MiningMethodId,
    equipment: EquipmentId,
) -> Result<ValidatedPlayerBlockExcavationStart, BlockExcavationError> {
    let target = raycast_player_block(registries, state, direction)
        .map_err(BlockExcavationError::Target)?
        .ok_or(BlockExcavationError::NoTarget)?;
    let block = registries
        .world()
        .block_for_state(target.state())
        .unwrap_or_else(|| unreachable!("targeted block state is registered"));
    let _excavation = block
        .excavation()
        .ok_or(BlockExcavationError::TargetNotExcavatable {
            target: target.voxel(),
        })?;
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), target.voxel())
        .unwrap_or_else(|| unreachable!("target ray established generated world truth"));
    let method_definition = registries
        .mining()
        .get_method(method)
        .ok_or(BlockExcavationError::UnknownMethod { method })?;
    validate_player_equipment_access(state, equipment)
        .map_err(BlockExcavationError::EquipmentAccess)?;
    let provider = resolve_available_equipment_provider(registries, state, equipment)
        .map_err(BlockExcavationError::Equipment)?;
    if state
        .equipment()
        .get_equipment(equipment)
        .is_some_and(|record| record.supported_by().is_some())
    {
        return Err(BlockExcavationError::EquipmentMounted { equipment });
    }
    let batch_capability = method_definition.max_batch_mass_capability();
    let Some(batch_value) = provider.get_capability(batch_capability) else {
        return Err(BlockExcavationError::Physics(
            MiningPhysicsError::MissingCapability {
                capability: batch_capability,
            },
        ));
    };
    let CapabilityValue::Mass(maximum_batch) = batch_value else {
        return Err(BlockExcavationError::Physics(
            MiningPhysicsError::CapabilityKindMismatch {
                capability: batch_capability,
                expected: CapabilityValueKind::Mass,
                found: batch_value.kind(),
            },
        ));
    };
    let mass = std::cmp::min(maximum_batch, source_mass);
    validate_start_player_block_excavation(
        registries,
        state,
        BlockExcavationRequest::new(direction, method, equipment, mass),
    )
}
