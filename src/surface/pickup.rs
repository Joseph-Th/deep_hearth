//! Control-rate automatic transfer of player-created loose drops into carried custody.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick, SimulationTick};
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    ValidatedMaterialIngress, apply_material_ingress, validate_material_ingress,
};
use crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

use super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS;
use super::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};

#[must_use]
pub(crate) struct SurfaceDropPickupPlan {
    expected_surface_revision: u64,
    next_surface_revision: u64,
    resource: SurfaceResourceId,
    picked_mass: Mass,
    picked_at: SimulationTick,
    ingress: ValidatedMaterialIngress,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDropPickupOutcome {
    resource: SurfaceResourceId,
    destination: StockpileId,
    lot: MaterialLotId,
    picked_mass: Mass,
}

impl SurfaceDropPickupOutcome {
    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn lot(self) -> MaterialLotId {
        self.lot
    }

    #[must_use]
    pub const fn picked_mass(self) -> Mass {
        self.picked_mass
    }
}

/// Shares the authoritative pickup eligibility gate with neutral slow stepping.
fn available_pickup_destination(state: &AppState) -> Option<(StockpileId, VoxelCoord, Mass)> {
    let player = state.logistics().player().copied()?;
    if !state
        .survival()
        .player()
        .is_some_and(|survival| survival.vitality() != Vitality::ZERO)
        || !state.can_spend_inventory_revisions(1)
        || !state.can_spend_surface_revisions(1)
    {
        return None;
    }
    let destination = player.carried_stockpile();
    let capacity = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| unreachable!("admitted player carried stockpile exists"))
        .available_capacity();
    (!capacity.is_zero()).then_some((destination, player.position(), capacity))
}

fn pickup_eligible_at(resource: &SurfaceResourceRecord, at: ControlTick) -> bool {
    // Automatic pickup is bare-handed. A hot parcel must remain on the ground
    // rather than entering a pack just because the actor walks through it.
    // The same eligibility gate controls neutral fast-forward and actual pickup.
    resource.temperature() <= MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE
        && resource
            .automatic_pickup_delay_started_at()
            .is_none_or(|started| {
                started
                    .checked_add_steps(PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS)
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: surface pickup delay exceeds control clock"
                        )
                    })
                    <= at
            })
}

/// A full pack, dead player, unallocatable parcel, or drop whose pickup delay
/// extends beyond the next slow boundary cannot produce a control-rate pickup
/// during a skipped interval. A merge into an existing lot remains possible
/// even if no new lot identities can be allocated.
pub(crate) fn automatic_surface_pickup_requires_control_stepping(
    registries: &Registries,
    state: &AppState,
) -> bool {
    let Some((destination, position, available_capacity)) = available_pickup_destination(state)
    else {
        return false;
    };
    let remaining = CONTROL_TICKS_PER_SIMULATION_TICK
        - state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK;
    let Some(next_boundary) = state.control_tick().checked_add_steps(remaining) else {
        return true;
    };
    let mut drops = state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && pickup_eligible_at(resource, next_boundary)
        });
    // With one available identity after reserving all future obligations,
    // any currently available local drop can be picked up on a control step.
    if state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1) {
        return drops.next().is_some();
    }
    // At identity exhaustion only parcels which coalesce with an existing lot
    // can transfer. This must use inventory's actual merge admission policy.
    drops.any(|resource| {
        validate_pickup_ingress(
            registries,
            state,
            destination,
            available_capacity,
            resource,
            state.tick(),
        )
        .is_some()
    })
}

fn validate_pickup_ingress(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    available_capacity: Mass,
    resource: &SurfaceResourceRecord,
    picked_at: SimulationTick,
) -> Option<ValidatedMaterialIngress> {
    let picked_mass = resource.remaining_mass().min(available_capacity);
    let entry = MaterialIngressEntry::from_surface_parcel(
        picked_mass,
        resource.material_profile().clone(),
        resource.material_provenance(),
        resource
            .ambient_storage_history_at(picked_at)
            .unwrap_or_else(|| unreachable!("surface drop exposure cannot precede creation")),
    );
    let ingress = match validate_material_ingress(
        registries,
        state.inventory(),
        destination,
        [entry],
        picked_at,
    ) {
        Ok(ingress) => ingress,
        Err(MaterialIngressError::CapacityExceeded { .. }) => {
            unreachable!("pickup mass is bounded by current carried available capacity")
        }
        Err(MaterialIngressError::LotIdExhausted | MaterialIngressError::RevisionExhausted) => {
            return None;
        }
        Err(error) => {
            panic!("runtime invariant broken: local surface drop pickup failed: {error}")
        }
    };
    state
        .has_material_lot_id_headroom_from(ingress.next_lot_id(), 0)
        .then_some(ingress)
}

pub(crate) fn decide_automatic_surface_drop_pickup(
    registries: &Registries,
    state: &AppState,
) -> Option<SurfaceDropPickupPlan> {
    let (destination, position, available_capacity) = available_pickup_destination(state)?;
    let picked_at = state.tick();
    for resource in state.surface().available_resources_at(Some(position)) {
        if resource.kind() != SurfaceResourceKind::Drop {
            continue;
        }
        if !pickup_eligible_at(resource, state.control_tick()) {
            continue;
        }
        let Some(ingress) = validate_pickup_ingress(
            registries,
            state,
            destination,
            available_capacity,
            resource,
            picked_at,
        ) else {
            // An unallocatable parcel cannot starve a later mergeable drop.
            continue;
        };
        let picked_mass = resource.remaining_mass().min(available_capacity);
        let next_surface_revision = state.surface().revision().checked_add(1)?;
        return Some(SurfaceDropPickupPlan {
            expected_surface_revision: state.surface().revision(),
            next_surface_revision,
            resource: resource.id(),
            picked_mass,
            picked_at,
            ingress,
        });
    }
    None
}

pub(crate) fn apply_automatic_surface_drop_pickup(
    state: &mut AppState,
    plan: SurfaceDropPickupPlan,
) -> SurfaceDropPickupOutcome {
    assert_eq!(state.surface().revision(), plan.expected_surface_revision);
    plan.ingress.assert_matches_state(state.inventory());
    let destination = plan.ingress.destination();
    let lot = apply_material_ingress(state.inventory_state_mut(), plan.ingress)
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("one surface drop pickup creates one inventory landing"));
    state.surface_state_mut().apply_gathering(
        plan.resource,
        plan.picked_mass,
        plan.picked_at,
        plan.next_surface_revision,
    );
    SurfaceDropPickupOutcome {
        resource: plan.resource,
        destination,
        lot,
        picked_mass: plan.picked_mass,
    }
}
