//! Timed conserved player excavation of targeted block-world matter.

mod errors;
mod runtime;
mod selection;

pub(crate) use runtime::{
    BlockExcavationCancellationPlan, BlockExcavationTickError, apply_block_excavation_cancellation,
    apply_block_excavation_tick, decide_block_excavation_cancellation,
    decide_block_excavation_completion, decide_block_excavation_tick,
};
pub use runtime::{BlockExcavationOutcome, BlockExcavationOutputReceipt};
pub(crate) use selection::validate_start_player_block_excavation_active;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::equipment::{EquipmentId, resolve_available_equipment_provider};
use crate::inventory::{
    StockpileId, ValidatedInboundReservation, validate_inbound_reservation,
    validate_stockpile_storage,
};
use crate::labor::{
    BlockExcavationOutputCustody, BlockExcavationWork, PlayerWork, ValidatedPlayerWorkStart,
    validate_player_work_start_control_rate,
};
use crate::logistics::validate_player_equipment_access;
use crate::mining::{MiningMethodId, resolve_mining_physics};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::structural::StructuralElementId;

use super::{BlockStateId, PlayerBlockTarget, raycast_player_block};

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

pub use errors::{BlockExcavationCommitError, BlockExcavationError};

/// One authoritative work reservation can remove a solid world cell entirely.
/// Residents inspect this read-only fact before moving onto future footing,
/// while trusted-load validation rejects forged overlapping custody.
pub(crate) fn full_block_excavation_reserved_at(state: &AppState, floor: VoxelCoord) -> bool {
    matches!(
        state.player_work().active(),
        Some(PlayerWork::BlockExcavation { work })
            if work.target() == floor && work.excavated_mass() == work.source_mass_before()
    )
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
            && let Some(owner) = TerrainFootingOwner::at(state, self.work.target())
        {
            return Err(owner.commit_error(self.work.target()));
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
    // Partial excavation retains a collidable block; complete removal must
    // honor existing material-backed stationary ownership of its footing.
    if request.mass == source_mass
        && let Some(owner) = TerrainFootingOwner::at(state, target.voxel())
    {
        return Err(owner.admission_error(target.voxel()));
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

/// Every installed floor owner shares one ordered obstruction assessment so
/// admission and stale-commit checks cannot disagree about who holds a cell.
enum TerrainFootingOwner {
    StorageEnclosure(StockpileId),
    EnergyStore(crate::energy::EnergyStoreId),
    StationaryEquipment(EquipmentId),
    OpeningResident,
    GroundedStructure(StructuralElementId),
}

impl TerrainFootingOwner {
    fn at(state: &AppState, floor: VoxelCoord) -> Option<Self> {
        storage_enclosure_resting_on(state, floor)
            .map(Self::StorageEnclosure)
            .or_else(|| stationary_energy_store_resting_on(state, floor).map(Self::EnergyStore))
            .or_else(|| {
                stationary_equipment_resting_on(state, floor).map(Self::StationaryEquipment)
            })
            .or_else(|| {
                crate::kobolds::opening_kobold_stands_on(state, floor)
                    .then_some(Self::OpeningResident)
            })
            .or_else(|| grounded_structure_resting_on(state, floor).map(Self::GroundedStructure))
    }

    fn admission_error(self, target: VoxelCoord) -> BlockExcavationError {
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
            Self::OpeningResident => BlockExcavationError::OpeningResidentRequiresFloor { target },
            Self::GroundedStructure(element) => {
                BlockExcavationError::GroundedStructureRequiresFloor { target, element }
            }
        }
    }

    fn commit_error(self, target: VoxelCoord) -> BlockExcavationCommitError {
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
            Self::OpeningResident => {
                BlockExcavationCommitError::OpeningResidentRequiresFloor { target }
            }
            Self::GroundedStructure(element) => {
                BlockExcavationCommitError::GroundedStructureRequiresFloor { target, element }
            }
        }
    }
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
