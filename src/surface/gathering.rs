//! Canonical admission for loose surface-resource gathering.

mod runtime;

pub use runtime::SurfaceGatheringOutcome;
pub(crate) use runtime::{
    SurfaceGatheringCancellationPlan, SurfaceGatheringTickError, SurfaceGatheringTickPlan,
    apply_surface_gathering_cancellation, apply_surface_gathering_tick,
    decide_surface_gathering_cancellation, decide_surface_gathering_tick,
};

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    InboundReservationError, MaterialIngressEntry, MaterialIngressError, StockpileId,
    StockpileStorageError, ValidatedInboundReservation, validate_inbound_reservation,
    validate_material_ingress,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, SurfaceGatheringMethodId,
    SurfaceGatheringWork, ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::material::MaterialLotSpec;
use crate::registry::Registries;

use super::{SurfaceResourceId, SurfaceResourceLifecycle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceGatheringRequest {
    method: SurfaceGatheringMethodId,
    resource: SurfaceResourceId,
    destination: StockpileId,
    mass: Mass,
}

impl SurfaceGatheringRequest {
    #[must_use]
    pub const fn new(
        method: SurfaceGatheringMethodId,
        resource: SurfaceResourceId,
        destination: StockpileId,
        mass: Mass,
    ) -> Self {
        Self {
            method,
            resource,
            destination,
            mass,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceGatheringError {
    PlayerNotLocated,
    UnknownMethod {
        method: SurfaceGatheringMethodId,
    },
    ResourceUnavailableAtPlayer {
        resource: SurfaceResourceId,
    },
    ZeroMass,
    BatchTooLarge {
        requested: Mass,
        maximum: Mass,
    },
    InsufficientResource {
        requested: Mass,
        remaining: Mass,
    },
    DestinationAccess(PlayerStockpileAccessError),
    DestinationMissing {
        stockpile: StockpileId,
    },
    DestinationMounted {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    MaterialLotIdExhausted,
    InventoryRevisionExhausted,
    SurfaceRevisionExhausted,
    CompletionTickOverflow,
    PlayerWork(PlayerWorkStartError),
}

impl Display for SurfaceGatheringError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "surface gathering unavailable: {self:?}")
    }
}

impl Error for SurfaceGatheringError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationAccess(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::PlayerNotLocated
            | Self::UnknownMethod { .. }
            | Self::ResourceUnavailableAtPlayer { .. }
            | Self::ZeroMass
            | Self::BatchTooLarge { .. }
            | Self::InsufficientResource { .. }
            | Self::DestinationMissing { .. }
            | Self::DestinationMounted { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::SurfaceRevisionExhausted
            | Self::CompletionTickOverflow => None,
        }
    }
}

fn map_ingress_error(error: MaterialIngressError) -> SurfaceGatheringError {
    match error {
        MaterialIngressError::Storage(error) => SurfaceGatheringError::DestinationStorage(error),
        MaterialIngressError::MassOverflow { stockpile } => {
            SurfaceGatheringError::DestinationMassOverflow { stockpile }
        }
        MaterialIngressError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => SurfaceGatheringError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialIngressError::LotIdExhausted => SurfaceGatheringError::MaterialLotIdExhausted,
        MaterialIngressError::RevisionExhausted => {
            SurfaceGatheringError::InventoryRevisionExhausted
        }
        MaterialIngressError::UnknownStockpile { stockpile } => {
            SurfaceGatheringError::DestinationMissing { stockpile }
        }
        MaterialIngressError::Empty
        | MaterialIngressError::UnknownMaterial { .. }
        | MaterialIngressError::UnknownForm { .. }
        | MaterialIngressError::UnknownCompositionMaterial { .. }
        | MaterialIngressError::ZeroMass
        | MaterialIngressError::InvalidComposition { .. }
        | MaterialIngressError::CompositionMissingHost { .. }
        | MaterialIngressError::ReservationMismatch { .. }
        | MaterialIngressError::ProvenanceInFuture { .. } => {
            unreachable!("trusted surface-resource material must remain valid inventory ingress")
        }
    }
}

fn map_reservation_error(error: InboundReservationError) -> SurfaceGatheringError {
    match error {
        InboundReservationError::UnknownStockpile { stockpile } => {
            SurfaceGatheringError::DestinationMissing { stockpile }
        }
        InboundReservationError::MassOverflow { stockpile } => {
            SurfaceGatheringError::DestinationMassOverflow { stockpile }
        }
        InboundReservationError::CapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => SurfaceGatheringError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        InboundReservationError::RevisionExhausted => {
            SurfaceGatheringError::InventoryRevisionExhausted
        }
    }
}

#[must_use]
pub struct ValidatedSurfaceGatheringStart {
    expected_surface_revision: u64,
    expected_logistics_revision: u64,
    reservation: ValidatedInboundReservation,
    work: SurfaceGatheringWork,
    player_work: ValidatedPlayerWorkStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceGatheringCommitError {
    StaleSurfaceRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    ResourceChanged,
    PlayerWork(PlayerWorkCommitError),
}

impl Display for SurfaceGatheringCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "surface gathering start became stale: {self:?}")
    }
}

impl Error for SurfaceGatheringCommitError {}

impl ValidatedSurfaceGatheringStart {
    pub fn commit(self, state: &mut AppState) -> Result<(), SurfaceGatheringCommitError> {
        let actual_surface = state.surface().revision();
        if actual_surface != self.expected_surface_revision {
            return Err(SurfaceGatheringCommitError::StaleSurfaceRevision {
                expected: self.expected_surface_revision,
                actual: actual_surface,
            });
        }
        let actual_inventory = state.inventory().revision();
        if actual_inventory != self.reservation.expected_revision() {
            return Err(SurfaceGatheringCommitError::StaleInventoryRevision {
                expected: self.reservation.expected_revision(),
                actual: actual_inventory,
            });
        }
        let actual_logistics = state.logistics().revision();
        if actual_logistics != self.expected_logistics_revision {
            return Err(SurfaceGatheringCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics,
            });
        }
        let resource = state
            .surface()
            .get(self.work.resource())
            .ok_or(SurfaceGatheringCommitError::ResourceChanged)?;
        if resource.lifecycle() != SurfaceResourceLifecycle::Available
            || resource.remaining_mass() != self.work.source_mass_before()
        {
            return Err(SurfaceGatheringCommitError::ResourceChanged);
        }
        self.player_work
            .precheck(state)
            .map_err(SurfaceGatheringCommitError::PlayerWork)?;
        self.reservation.apply(state.inventory_state_mut());
        self.player_work.apply(state);
        Ok(())
    }
}

pub fn validate_start_surface_gathering(
    registries: &Registries,
    state: &AppState,
    request: SurfaceGatheringRequest,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    let method = registries
        .labor()
        .get_surface_gathering(request.method)
        .copied()
        .ok_or(SurfaceGatheringError::UnknownMethod {
            method: request.method,
        })?;
    if state.logistics().player().is_none() {
        return Err(SurfaceGatheringError::PlayerNotLocated);
    }
    let resource = state
        .available_surface_resources()
        .find(|resource| resource.id() == request.resource)
        .ok_or(SurfaceGatheringError::ResourceUnavailableAtPlayer {
            resource: request.resource,
        })?;
    if request.mass.is_zero() {
        return Err(SurfaceGatheringError::ZeroMass);
    }
    if request.mass > method.maximum_batch_mass() {
        return Err(SurfaceGatheringError::BatchTooLarge {
            requested: request.mass,
            maximum: method.maximum_batch_mass(),
        });
    }
    if request.mass > resource.remaining_mass() {
        return Err(SurfaceGatheringError::InsufficientResource {
            requested: request.mass,
            remaining: resource.remaining_mass(),
        });
    }
    validate_player_stockpile_access(state, request.destination)
        .map_err(SurfaceGatheringError::DestinationAccess)?;
    let destination = state.inventory().get_stockpile(request.destination).ok_or(
        SurfaceGatheringError::DestinationMissing {
            stockpile: request.destination,
        },
    )?;
    if destination.supported_by().is_some() {
        return Err(SurfaceGatheringError::DestinationMounted {
            stockpile: request.destination,
        });
    }
    if !state.can_spend_inventory_revisions(2) {
        return Err(SurfaceGatheringError::InventoryRevisionExhausted);
    }
    if !state.has_material_lot_id_headroom_from(state.inventory().next_lot_id(), 1) {
        return Err(SurfaceGatheringError::MaterialLotIdExhausted);
    }
    // Admission owes one deferred surface mutation when the gathering completes. Existing active
    // work is included by the shared headroom calculation, so this prospective step cannot consume
    // capacity already promised elsewhere.
    if !state.can_spend_surface_revisions(1) {
        return Err(SurfaceGatheringError::SurfaceRevisionExhausted);
    }
    let duration = method
        .duration_for_mass(request.mass)
        .unwrap_or_else(|| unreachable!("validated gathering mass has an authored duration"));
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(SurfaceGatheringError::CompletionTickOverflow)?;
    let lot = MaterialLotSpec::with_composition(
        resource.commodity(),
        request.mass,
        resource.temperature(),
        resource.composition().clone(),
    )
    .unwrap_or_else(|_| unreachable!("trusted surface material yields a valid inventory parcel"));
    let entry = MaterialIngressEntry::from_lot_spec_with_ambient_exposure(
        lot,
        resource.generated_at(),
        state.tick(),
    )
    .unwrap_or_else(|| unreachable!("surface resource exposure cannot precede generation"));
    let _ = validate_material_ingress(
        registries,
        state.inventory(),
        request.destination,
        [entry],
        state.tick(),
    )
    .map_err(map_ingress_error)?;
    let reservation =
        validate_inbound_reservation(state.inventory(), request.destination, request.mass)
            .map_err(map_reservation_error)?;
    let work = SurfaceGatheringWork::new(
        request.resource,
        request.method,
        request.destination,
        request.mass,
        resource.remaining_mass(),
        state.tick(),
        completes_at,
    );
    let player_work = validate_player_work_start(
        registries,
        state,
        PlayerWork::SurfaceGathering { work },
        duration,
        method.exertion(),
    )
    .map_err(SurfaceGatheringError::PlayerWork)?;
    Ok(ValidatedSurfaceGatheringStart {
        expected_surface_revision: state.surface().revision(),
        expected_logistics_revision: state.logistics().revision(),
        reservation,
        work,
        player_work,
    })
}
