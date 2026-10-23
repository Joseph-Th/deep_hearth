//! Runtime completion and interruption for admitted block excavation.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick};
use crate::equipment::EquipmentId;
use crate::inventory::{
    InventoryState, MaterialIngressEntry, MaterialIngressError, MaterialLotId,
    ValidatedInboundReservationRelease, ValidatedMaterialIngress, apply_material_ingress,
    validate_inbound_reservation_release, validate_reserved_material_ingress,
};
use crate::labor::{BlockExcavationOutputCustody, BlockExcavationWork};
use crate::maintenance::{Condition, calculate_condition_after_active_control_ticks};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceCreationError, SurfaceResourceId, SurfaceResourceKind, SurfaceResourceSpec,
    ValidatedSurfaceResourceCreation, validate_create_surface_resources_at,
};

enum BlockExcavationSettlementPlan {
    Carried(ValidatedMaterialIngress),
    Surface(ValidatedSurfaceResourceCreation),
}

#[must_use]
pub(crate) struct BlockExcavationTickPlan {
    work: BlockExcavationWork,
    settlement: BlockExcavationSettlementPlan,
    next_world_revision: u64,
    next_equipment_revision: u64,
}

impl BlockExcavationTickPlan {
    #[must_use]
    pub(crate) const fn mutates_inventory(&self) -> bool {
        matches!(self.settlement, BlockExcavationSettlementPlan::Carried(_))
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockExcavationOutputReceipt {
    Carried { lot: MaterialLotId },
    Surface { resource: SurfaceResourceId },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockExcavationOutcome {
    target: VoxelCoord,
    excavated_mass: Mass,
    remaining_mass: Mass,
    output: BlockExcavationOutputReceipt,
}

impl BlockExcavationOutcome {
    #[must_use]
    pub const fn target(self) -> VoxelCoord {
        self.target
    }

    #[must_use]
    pub const fn excavated_mass(self) -> Mass {
        self.excavated_mass
    }

    #[must_use]
    pub const fn remaining_mass(self) -> Mass {
        self.remaining_mass
    }

    pub const fn output(self) -> BlockExcavationOutputReceipt {
        self.output
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockExcavationTickError {
    MaterialLotId,
    InventoryRevision,
    SurfaceResourceId,
    SurfaceRevision,
    WorldRevision,
    EquipmentRevision,
}

pub(crate) fn decide_block_excavation_tick(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    next_tick: SimulationTick,
) -> Result<Option<BlockExcavationTickPlan>, BlockExcavationTickError> {
    let next_control = ControlTick::from_simulation_tick(next_tick)
        .unwrap_or_else(|| unreachable!("slow tick must have a control boundary"));
    decide_block_excavation_completion(
        registries,
        state,
        projected_inventory,
        next_control,
        next_tick,
    )
}

pub(crate) fn decide_block_excavation_completion(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    next_control: ControlTick,
    produced_at: SimulationTick,
) -> Result<Option<BlockExcavationTickPlan>, BlockExcavationTickError> {
    let Some(work) = state.player_work().block_excavation_due_at(next_control) else {
        return Ok(None);
    };
    assert_eq!(state.world().block_at(work.target()), Some(work.state()));
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), work.target()),
        Some(work.source_mass_before())
    );
    let block = registries
        .world()
        .block_for_state(work.state())
        .unwrap_or_else(|| panic!("runtime invariant broken: excavation block state disappeared"));
    let excavation = block.excavation().unwrap_or_else(|| {
        panic!("runtime invariant broken: active excavation block lost definition")
    });
    let lot = excavation.fragmented_lot(work.excavated_mass());
    let settlement = match work.output() {
        BlockExcavationOutputCustody::Carried { stockpile } => {
            let ingress = validate_reserved_material_ingress(
                registries,
                projected_inventory,
                stockpile,
                [MaterialIngressEntry::from_lot_spec(lot, produced_at)],
                produced_at,
                work.excavated_mass(),
            )
            .map_err(|error| match error {
                MaterialIngressError::LotIdExhausted => BlockExcavationTickError::MaterialLotId,
                MaterialIngressError::RevisionExhausted => {
                    BlockExcavationTickError::InventoryRevision
                }
                other @ (MaterialIngressError::Empty
                | MaterialIngressError::UnknownStockpile { .. }
                | MaterialIngressError::UnknownMaterial { .. }
                | MaterialIngressError::UnknownForm { .. }
                | MaterialIngressError::UnknownCompositionMaterial { .. }
                | MaterialIngressError::ZeroMass
                | MaterialIngressError::InvalidComposition { .. }
                | MaterialIngressError::CompositionMissingHost { .. }
                | MaterialIngressError::Storage(_)
                | MaterialIngressError::ProvenanceInFuture { .. }
                | MaterialIngressError::MassOverflow { .. }
                | MaterialIngressError::CapacityExceeded { .. }
                | MaterialIngressError::ReservationMismatch { .. }) => {
                    panic!("runtime invariant broken: admitted block excavation ingress failed: {other:?}")
                }
            })?;
            BlockExcavationSettlementPlan::Carried(ingress)
        }
        BlockExcavationOutputCustody::Surface { position } => {
            let spec = SurfaceResourceSpec::new(
                SurfaceResourceKind::Drop,
                position,
                excavation.fragmented_commodity(),
                work.excavated_mass(),
                excavation.temperature(),
                lot.composition().clone(),
            )
            .unwrap_or_else(|error| {
                panic!("runtime invariant broken: admitted excavation drop is invalid: {error}")
            });
            let creation = validate_create_surface_resources_at(
                registries,
                state,
                [spec],
                produced_at,
            )
            .map_err(|error| match error {
                SurfaceResourceCreationError::IdExhausted => {
                    BlockExcavationTickError::SurfaceResourceId
                }
                SurfaceResourceCreationError::RevisionExhausted => {
                    BlockExcavationTickError::SurfaceRevision
                }
                SurfaceResourceCreationError::Empty
                | SurfaceResourceCreationError::InvalidResource(_) => {
                    panic!("runtime invariant broken: admitted excavation surface creation failed: {error}")
                }
            })?;
            BlockExcavationSettlementPlan::Surface(creation)
        }
    };
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| panic!("runtime invariant broken: excavation equipment disappeared"));
    assert_eq!(equipment.definition(), work.equipment_trace().definition());
    assert_eq!(equipment.condition(), work.equipment_trace().condition());
    let next_world_revision = state
        .world()
        .revision()
        .checked_add(1)
        .ok_or(BlockExcavationTickError::WorldRevision)?;
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(BlockExcavationTickError::EquipmentRevision)?;
    Ok(Some(BlockExcavationTickPlan {
        work,
        settlement,
        next_world_revision,
        next_equipment_revision,
    }))
}

pub(crate) fn apply_block_excavation_tick(
    registries: &Registries,
    state: &mut AppState,
    plan: Option<BlockExcavationTickPlan>,
) -> Option<BlockExcavationOutcome> {
    let plan = plan?;
    let default_mass = registries
        .world()
        .represented_mass_for_state(plan.work.state())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: excavation state lost represented mass")
        });
    let remaining_mass = plan
        .work
        .source_mass_before()
        .checked_sub(plan.work.excavated_mass())
        .unwrap_or_else(|| panic!("runtime invariant broken: excavation exceeds source cell mass"));
    let output = match plan.settlement {
        BlockExcavationSettlementPlan::Carried(ingress) => {
            ingress.assert_matches_state(state.inventory());
            let lots = apply_material_ingress(state.inventory_state_mut(), ingress);
            let lot = *lots
                .first()
                .unwrap_or_else(|| panic!("block excavation ingress created no material lot"));
            BlockExcavationOutputReceipt::Carried { lot }
        }
        BlockExcavationSettlementPlan::Surface(creation) => {
            creation.precheck(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: excavation surface creation became stale: {error}"
                )
            });
            let resource = creation.first_resource_id();
            creation.apply(state);
            BlockExcavationOutputReceipt::Surface { resource }
        }
    };
    state.equipment_state_mut().apply_condition_change(
        plan.work.equipment(),
        plan.work.equipment_trace().condition(),
        plan.work.condition_after(),
        plan.next_equipment_revision,
    );
    let expected_world_revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        expected_world_revision,
        plan.next_world_revision,
        plan.work.target(),
        plan.work.state(),
        default_mass,
        remaining_mass,
    );
    Some(BlockExcavationOutcome {
        target: plan.work.target(),
        excavated_mass: plan.work.excavated_mass(),
        remaining_mass,
        output,
    })
}

#[must_use]
pub(crate) struct BlockExcavationCancellationPlan {
    reservation_release: Option<ValidatedInboundReservationRelease>,
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    next_equipment_revision: Option<u64>,
}

impl BlockExcavationCancellationPlan {
    #[must_use]
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        if self.next_equipment_revision.is_some() {
            1
        } else {
            0
        }
    }

    #[must_use]
    pub(crate) const fn mutates_inventory(&self) -> bool {
        self.reservation_release.is_some()
    }
}

pub(crate) fn decide_block_excavation_cancellation(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    work: BlockExcavationWork,
    next_tick: SimulationTick,
) -> Result<BlockExcavationCancellationPlan, BlockExcavationTickError> {
    let reservation_release = match work.output().carried_stockpile() {
        Some(stockpile) => Some(
            validate_inbound_reservation_release(
                projected_inventory,
                stockpile,
                work.excavated_mass(),
            )
            .map_err(|_| BlockExcavationTickError::InventoryRevision)?,
        ),
        None => None,
    };
    let next_control = ControlTick::from_simulation_tick(next_tick)
        .unwrap_or_else(|| unreachable!("fatal slow tick must have a control boundary"));
    let elapsed_control_ticks = next_control
        .checked_steps_since(work.started_at())
        .unwrap_or_else(|| panic!("fatal tick cannot precede excavation start"));
    let method = registries
        .mining()
        .get_method(work.method())
        .unwrap_or_else(|| panic!("runtime invariant broken: excavation method disappeared"));
    let condition_before = work.equipment_trace().condition();
    let condition_after = calculate_condition_after_active_control_ticks(
        method.condition_wear_ppm_per_active_tick(),
        condition_before,
        elapsed_control_ticks,
    );
    let next_equipment_revision = if condition_after == condition_before {
        None
    } else {
        Some(
            state
                .equipment()
                .revision()
                .checked_add(1)
                .ok_or(BlockExcavationTickError::EquipmentRevision)?,
        )
    };
    Ok(BlockExcavationCancellationPlan {
        reservation_release,
        equipment: work.equipment(),
        condition_before,
        condition_after,
        next_equipment_revision,
    })
}

pub(crate) fn apply_block_excavation_cancellation(
    state: &mut AppState,
    plan: BlockExcavationCancellationPlan,
) {
    if let Some(release) = plan.reservation_release {
        release.apply(state.inventory_state_mut());
    }
    if let Some(next_revision) = plan.next_equipment_revision {
        state.equipment_state_mut().apply_condition_change(
            plan.equipment,
            plan.condition_before,
            plan.condition_after,
            next_revision,
        );
    }
}
