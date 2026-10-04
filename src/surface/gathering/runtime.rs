//! Runtime completion and interruption for admitted surface gathering.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    InboundReservationReleaseError, InventoryState, MaterialIngressEntry, MaterialIngressError,
    MaterialLotId, StockpileId, ValidatedInboundReservationRelease, ValidatedMaterialIngress,
    apply_material_ingress, validate_inbound_reservation_release,
    validate_reserved_material_ingress,
};
use crate::labor::SurfaceGatheringWork;
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::super::{SurfaceResourceId, SurfaceResourceLifecycle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceGatheringTickError {
    MaterialLotIds,
    InventoryRevision,
    SurfaceRevision,
}

#[must_use]
pub(crate) struct SurfaceGatheringTickPlan {
    expected_surface_revision: u64,
    work: SurfaceGatheringWork,
    ingress: ValidatedMaterialIngress,
    next_surface_revision: u64,
}

#[must_use]
pub(crate) struct SurfaceGatheringCancellationPlan {
    reservation_release: ValidatedInboundReservationRelease,
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceGatheringOutcome {
    resource: SurfaceResourceId,
    destination: StockpileId,
    lot: MaterialLotId,
    gathered_mass: Mass,
    remaining_mass: Mass,
}

impl SurfaceGatheringOutcome {
    #[must_use]
    pub const fn resource(&self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn lot(&self) -> MaterialLotId {
        self.lot
    }

    #[must_use]
    pub const fn gathered_mass(&self) -> Mass {
        self.gathered_mass
    }

    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }
}

pub(crate) fn decide_surface_gathering_cancellation(
    projected_inventory: &InventoryState,
    work: SurfaceGatheringWork,
) -> Result<SurfaceGatheringCancellationPlan, SurfaceGatheringTickError> {
    let reservation_release = validate_inbound_reservation_release(
        projected_inventory,
        work.destination(),
        work.gathered_mass(),
    )
    .map_err(|error| match error {
        InboundReservationReleaseError::RevisionExhausted => {
            SurfaceGatheringTickError::InventoryRevision
        }
    })?;
    Ok(SurfaceGatheringCancellationPlan {
        reservation_release,
    })
}

pub(crate) fn apply_surface_gathering_cancellation(
    state: &mut AppState,
    plan: SurfaceGatheringCancellationPlan,
) {
    plan.reservation_release.apply(state.inventory_state_mut());
}

pub(crate) fn decide_surface_gathering_tick(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    next_tick: SimulationTick,
) -> Result<Option<SurfaceGatheringTickPlan>, SurfaceGatheringTickError> {
    let Some(work) = state.player_work().surface_gathering_due_at(next_tick) else {
        return Ok(None);
    };
    let resource = state.surface().get(work.resource()).unwrap_or_else(|| {
        panic!("runtime invariant broken: active gathering resource disappeared")
    });
    assert_eq!(resource.lifecycle(), SurfaceResourceLifecycle::Available);
    assert_eq!(resource.remaining_mass(), work.source_mass_before());
    assert!(resource.remaining_mass() >= work.gathered_mass());
    let lot = MaterialLotSpec::with_composition(
        resource.commodity(),
        work.gathered_mass(),
        resource.temperature(),
        resource.composition().clone(),
    )
    .unwrap_or_else(|_| unreachable!("trusted surface material remains valid at completion"));
    let ingress = validate_reserved_material_ingress(
        registries,
        projected_inventory,
        work.destination(),
        [MaterialIngressEntry::from_lot_spec_with_ambient_exposure(
            lot,
            resource.generated_at(),
            next_tick,
        )
        .unwrap_or_else(|| unreachable!("surface resource exposure cannot precede generation"))],
        next_tick,
        work.gathered_mass(),
    )
    .map_err(|error| match error {
        MaterialIngressError::LotIdExhausted => SurfaceGatheringTickError::MaterialLotIds,
        MaterialIngressError::RevisionExhausted => SurfaceGatheringTickError::InventoryRevision,
        other @ MaterialIngressError::Empty
        | other @ MaterialIngressError::UnknownStockpile { .. }
        | other @ MaterialIngressError::UnknownMaterial { .. }
        | other @ MaterialIngressError::UnknownForm { .. }
        | other @ MaterialIngressError::UnknownCompositionMaterial { .. }
        | other @ MaterialIngressError::ZeroMass
        | other @ MaterialIngressError::InvalidComposition { .. }
        | other @ MaterialIngressError::CompositionMissingHost { .. }
        | other @ MaterialIngressError::Storage(_)
        | other @ MaterialIngressError::ProvenanceInFuture { .. }
        | other @ MaterialIngressError::MassOverflow { .. }
        | other @ MaterialIngressError::CapacityExceeded { .. }
        | other @ MaterialIngressError::ReservationMismatch { .. } => panic!(
            "runtime invariant broken: admitted surface gathering ingress became invalid: {other:?}"
        ),
    })?;
    let next_surface_revision = state
        .surface()
        .revision()
        .checked_add(1)
        .ok_or(SurfaceGatheringTickError::SurfaceRevision)?;
    Ok(Some(SurfaceGatheringTickPlan {
        expected_surface_revision: state.surface().revision(),
        work,
        ingress,
        next_surface_revision,
    }))
}

pub(crate) fn apply_surface_gathering_tick(
    state: &mut AppState,
    plan: Option<SurfaceGatheringTickPlan>,
) -> Option<SurfaceGatheringOutcome> {
    let plan = plan?;
    assert_eq!(state.surface().revision(), plan.expected_surface_revision);
    plan.ingress.assert_matches_state(state.inventory());
    let lots = apply_material_ingress(state.inventory_state_mut(), plan.ingress);
    let lot = lots
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("surface gathering returns exactly one parcel"));
    state.surface_state_mut().apply_gathering(
        plan.work.resource(),
        plan.work.gathered_mass(),
        plan.work.completes_at(),
        plan.next_surface_revision,
    );
    let remaining_mass = state
        .surface()
        .get(plan.work.resource())
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| unreachable!("surface gathering retains depleted resource record"));
    Some(SurfaceGatheringOutcome {
        resource: plan.work.resource(),
        destination: plan.work.destination(),
        lot,
        gathered_mass: plan.work.gathered_mass(),
        remaining_mass,
    })
}
