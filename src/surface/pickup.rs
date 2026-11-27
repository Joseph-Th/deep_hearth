//! Control-rate automatic transfer of player-created loose drops into carried custody.

use crate::content::{FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick, TickSpan};
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    ValidatedMaterialIngress, apply_material_ingress, validate_material_ingress,
};
use crate::labor::PlayerWork;
use crate::material::{CommodityKey, MAX_HAND_SAFE_MATERIAL_TEMPERATURE};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

use super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS;
use super::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord,
    resource_reserved_for_player_gathering,
};

#[must_use]
pub(crate) struct SurfaceDropPickupPlan {
    expected_surface_revision: u64,
    next_surface_revision: u64,
    resource: SurfaceResourceId,
    commodity: CommodityKey,
    picked_mass: Mass,
    picked_at: SimulationTick,
    ingress: ValidatedMaterialIngress,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDropPickupOutcome {
    resource: SurfaceResourceId,
    commodity: CommodityKey,
    destination: StockpileId,
    lot: MaterialLotId,
    picked_mass: Mass,
}

impl SurfaceDropPickupOutcome {
    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }

    /// The commodity transferred at admission, even if a later same-frame action
    /// consumes or drops the resulting inventory lot.
    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
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
    if state.player_work().active().is_some() {
        // All exclusive labor occupies the actor's hands and attention.
        // Automatic pickup cannot move matter into a pack while the player
        // is chopping, crafting, building, servicing tools, or consuming.
        return None;
    }
    if !state.can_spend_inventory_revisions(1) || !state.can_spend_surface_revisions(1) {
        return None;
    }
    pickup_physical_destination(state)
}

/// Physical eligibility independent of current inventory revision headroom.
/// Fast-forward must consider headroom released by the upcoming settlement.
fn pickup_physical_destination(state: &AppState) -> Option<(StockpileId, VoxelCoord, Mass)> {
    let player = state.logistics().player().copied()?;
    if !state
        .survival()
        .player()
        .is_some_and(|survival| survival.vitality() != Vitality::ZERO)
        // Loose-drop collection is an unprotected hand action. A committed
        // melee windup occupies that hand just as it does for explicit
        // ground transfers; automatic collection cannot bypass the stance.
        || state.combat().has_prepared_strike()
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

/// Timber piles are deliberate loading sites, not incidental pocket pickups.
/// Apply the same rule to felled and player-dropped logs, including remnants.
pub(super) fn requires_deliberate_pickup(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
}

fn pickup_eligible_at(state: &AppState, resource: &SurfaceResourceRecord, at: ControlTick) -> bool {
    // Automatic pickup is bare-handed. A hot parcel must remain on the ground
    // rather than entering a pack just because the actor walks through it.
    // An active hand-gathering job owns its exact source mass until it
    // completes. Both exclusions govern slow stepping and actual pickup.
    !requires_deliberate_pickup(resource)
        && !resource_reserved_for_player_gathering(state, resource.id())
        && resource.temperature() <= MAX_HAND_SAFE_MATERIAL_TEMPERATURE
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

/// Read-only pickup preflight using the same inventory admission as control-time pickup.
/// A full pack is not the only blocker: exhausted lot identities prevent
/// unmergeable parcels from entering even when physical mass capacity remains.
pub(super) fn automatic_surface_drop_pickup_preflight_ready(
    registries: &Registries,
    state: &AppState,
    resource: &SurfaceResourceRecord,
) -> bool {
    let Some((destination, position, available_capacity)) = available_pickup_destination(state)
    else {
        return false;
    };
    resource.kind() == SurfaceResourceKind::Drop
        && resource.position() == position
        && pickup_eligible_at(state, resource, state.control_tick())
        && validate_pickup_ingress(
            registries,
            state,
            destination,
            available_capacity,
            resource,
            state.tick(),
        )
        .is_some()
}

/// Work can release the player's hands on the next slow boundary, before
/// that boundary's final control-frame pickup. Inline work has its own
/// schedule; production and mining keep theirs with the owning job.
///
/// Production can also release attention before its due tick when a physical
/// provider becomes unavailable. Use the production owner's suspension rule;
/// unsettled mining may likewise release attention outside its original schedule.
fn work_may_release_at_next_boundary(state: &AppState, next: SimulationTick) -> bool {
    let Some(work) = state.player_work().active() else {
        return false;
    };
    if let Some((_, completes_at)) = work.inline_schedule() {
        return completes_at == next;
    }
    match work {
        PlayerWork::ManualProduction { job } => {
            state.production().get_job(job).is_some_and(|record| {
                record.completes_at() <= next
                    || record.is_suspended()
                    || crate::production::current_physical_suspension_reason(state, record)
                        .is_some()
            })
        }
        PlayerWork::Mining { job } => state
            .mining()
            .get_job(job)
            .is_some_and(|record| record.completes_at() <= next || record.is_awaiting_settlement()),
        PlayerWork::BlockExcavation { .. } => false,
        PlayerWork::TreeFelling { .. }
        | PlayerWork::SurfaceGathering { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }
        | PlayerWork::Eating { .. }
        | PlayerWork::Drinking { .. }
        | PlayerWork::EquipmentMaintenance { .. }
        | PlayerWork::StorageEnclosureDismantling { .. }
        | PlayerWork::StructuralConstruction { .. }
        | PlayerWork::StructuralDismantling { .. } => {
            unreachable!("all other exclusive work has an inline completion schedule")
        }
    }
}

/// Inventory ingress and released reservations can make a previously blocked
/// parcel collectible on the boundary's final control sample. Direct work
/// has already passed the release check; autonomous jobs retain their own
/// due and settlement indexes. No compatible-lot guess can prove a skip safe
/// until those owners have settled.
fn boundary_inventory_may_change(
    registries: &Registries,
    state: &AppState,
    next: SimulationTick,
) -> bool {
    state.player_work().future_inventory_revision_demand() != 0
        || state.production().earliest_due_tick() == Some(next)
        || state
            .mining()
            .settlement_candidates_at(next)
            .next()
            .is_some()
        || !crate::production::resumed_jobs_completing_next_tick(registries, state, next)
            .is_ok_and(|jobs| jobs.is_empty())
}

/// A full pack, dead player, or drop whose pickup delay extends beyond the
/// next boundary cannot produce a control-rate pickup during a skipped interval.
/// At lot or inventory-revision exhaustion, allow skipping only if no eligible
/// parcel can enter now or become collectible through scheduled inventory ingress.
pub(crate) fn automatic_surface_pickup_requires_control_stepping(
    registries: &Registries,
    state: &AppState,
) -> bool {
    let Some(next) = state.tick().checked_add_executable_span(TickSpan::new(1)) else {
        return false;
    };
    if state.player_work().active().is_some() && !work_may_release_at_next_boundary(state, next) {
        return false;
    }
    let Some((destination, position, available_capacity)) = pickup_physical_destination(state)
    else {
        return false;
    };
    if !state.can_spend_surface_revisions(1) {
        return false;
    }
    // A reserved loose parcel cannot be picked up during hand gathering.
    // On the due boundary the final control sample releases its reservation
    // and must be retained to collect any unclaimed remainder immediately.
    if let Some(PlayerWork::SurfaceGathering { work }) = state.player_work().active()
        && let Some(resource) = state.surface().get(work.resource())
        && resource.kind() == SurfaceResourceKind::Drop
        && !requires_deliberate_pickup(resource)
        && resource.remaining_mass() > work.gathered_mass()
        && resource.temperature() <= MAX_HAND_SAFE_MATERIAL_TEMPERATURE
        && work.completes_at() == next
    {
        return true;
    }
    let next_boundary = ControlTick::from_simulation_tick(next)
        .expect("executable simulation tick has a control boundary");
    let mut drops = state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && pickup_eligible_at(state, resource, next_boundary)
        })
        .peekable();
    if drops.peek().is_none() {
        return false;
    }
    if state.can_spend_inventory_revisions(1) {
        // With one available identity after reserving all future obligations,
        // any eligible local drop can be picked up on a control step.
        if state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1) {
            return true;
        }
        // At identity exhaustion, ask the actual merge admission policy.
        if drops.any(|resource| {
            validate_pickup_ingress(
                registries,
                state,
                destination,
                available_capacity,
                resource,
                state.tick(),
            )
            .is_some()
        }) {
            return true;
        }
    }
    boundary_inventory_may_change(registries, state, next)
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
        if !pickup_eligible_at(state, resource, state.control_tick()) {
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
            commodity: resource.commodity(),
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
        commodity: plan.commodity,
        destination,
        lot,
        picked_mass: plan.picked_mass,
    }
}
