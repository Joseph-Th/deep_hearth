//! Control-rate automatic transfer of player-created loose drops into carried custody.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    MaterialIngressEntry, MaterialIngressError, MaterialLotId, StockpileId,
    ValidatedMaterialIngress, apply_material_ingress, validate_material_ingress,
};
use crate::labor::PlayerWork;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::{SurfaceResourceId, SurfaceResourceKind};

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

pub(crate) fn decide_automatic_surface_drop_pickup(
    registries: &Registries,
    state: &AppState,
) -> Option<SurfaceDropPickupPlan> {
    let player = state.logistics().player().copied()?;
    if !state
        .survival()
        .player()
        .is_some_and(|survival| survival.vitality() != Vitality::ZERO)
    {
        return None;
    }
    if matches!(state.player_work().active(), Some(PlayerWork::Step { .. })) {
        return None;
    }
    if !state.can_spend_inventory_revisions(1) || !state.can_spend_surface_revisions(1) {
        return None;
    }

    let destination = player.carried_stockpile();
    let picked_at = state.tick();
    for resource in state
        .surface()
        .available_resources_at(Some(player.position()))
    {
        if resource.kind() != SurfaceResourceKind::Drop {
            continue;
        }
        let picked_mass = resource.remaining_mass();
        let lot = MaterialLotSpec::with_composition(
            resource.commodity(),
            picked_mass,
            resource.temperature(),
            resource.composition().clone(),
        )
        .unwrap_or_else(|_| unreachable!("trusted surface drop remains a valid material parcel"));
        let entry = MaterialIngressEntry::from_lot_spec_with_ambient_exposure(
            lot,
            resource.created_at(),
            picked_at,
        )
        .unwrap_or_else(|| unreachable!("surface drop exposure cannot precede creation"));
        let ingress = match validate_material_ingress(
            registries,
            state.inventory(),
            destination,
            [entry],
            picked_at,
        ) {
            Ok(ingress) => ingress,
            Err(MaterialIngressError::CapacityExceeded { .. }) => continue,
            Err(MaterialIngressError::LotIdExhausted | MaterialIngressError::RevisionExhausted) => {
                return None;
            }
            Err(error) => {
                panic!("runtime invariant broken: local surface drop pickup failed: {error}")
            }
        };
        if !state.has_material_lot_id_headroom_from(ingress.next_lot_id(), 0) {
            return None;
        }
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
