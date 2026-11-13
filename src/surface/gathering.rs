//! Canonical admission for loose surface-resource gathering.

mod errors;
mod options;
mod runtime;

pub use errors::SurfaceGatheringError;
use errors::hand_gathering_temperature_blocker;
pub use options::{
    PlayerSurfaceFoodValue, PlayerSurfaceGatheringOption, player_surface_gathering_options,
};

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
    ValidatedInboundReservation, validate_inbound_reservation, validate_material_ingress,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, SurfaceGatheringMethodId, SurfaceGatheringWork,
    ValidatedPlayerWorkStart, validate_player_work_start, validate_player_work_start_control_rate,
};
use crate::logistics::validate_player_stockpile_access;
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

/// Material already known to the player's current gathering work, without
/// re-evaluating all nearby available choices or exposing unrelated sources.
#[must_use]
pub fn active_player_surface_gathering_commodity(
    state: &AppState,
) -> Option<crate::material::CommodityKey> {
    let PlayerWork::SurfaceGathering { work } = state.player_work().active()? else {
        return None;
    };
    state
        .surface()
        .get(work.resource())
        .map(|source| source.commodity())
}

/// A logistics-only capability fixture may gather without a survival actor.
/// A real actor whose survival record exists but is dead cannot observe or
/// reuse the old body's exact-local resources before respawn.
fn validate_gathering_player_observer(state: &AppState) -> Result<(), SurfaceGatheringError> {
    if state.logistics().player().is_none() {
        return Err(SurfaceGatheringError::PlayerNotLocated);
    }
    if state.player_observation_position().is_none() {
        return Err(SurfaceGatheringError::PlayerDead);
    }
    Ok(())
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

/// Starts one current actor-visible gathering option for a deliberate physical amount.
///
/// The projection is revision-bound so a caller cannot silently retarget an option after surface
/// custody changes. Exact method identity remains private to this owner boundary.
pub fn validate_start_player_surface_gathering_option(
    registries: &Registries,
    state: &AppState,
    option: PlayerSurfaceGatheringOption,
    mass: Mass,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    validate_player_gathering_option_with_mode(registries, state, option, mass, false)
}

/// Read-only feasibility for contextual choice, using the same admission
/// prechecks as the executable control-rate gathering action.
pub(crate) fn assess_player_surface_gathering_option_admission(
    registries: &Registries,
    state: &AppState,
    option: PlayerSurfaceGatheringOption,
    mass: Mass,
) -> Result<(), SurfaceGatheringError> {
    validate_player_gathering_option_with_mode(registries, state, option, mass, true).map(|_| ())
}

/// Admit a contextual gathering job on the current control frame.
/// Settlement still runs on the ordinary slow simulation tick.
pub(crate) fn validate_start_player_surface_gathering_option_control_rate(
    registries: &Registries,
    state: &AppState,
    option: PlayerSurfaceGatheringOption,
    mass: Mass,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    validate_player_gathering_option_with_mode(registries, state, option, mass, true)
}

fn validate_player_gathering_option_with_mode(
    registries: &Registries,
    state: &AppState,
    option: PlayerSurfaceGatheringOption,
    mass: Mass,
    preview_off_boundary: bool,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    // Obsolete actor snapshots cannot reveal source revision changes before
    // the same liveness check used by the local gathering catalog.
    validate_gathering_player_observer(state)?;
    let actual_surface_revision = state.surface().revision();
    if actual_surface_revision != option.expected_surface_revision {
        return Err(SurfaceGatheringError::StalePlayerOption {
            expected_surface_revision: option.expected_surface_revision,
            actual_surface_revision,
        });
    }
    validate_player_gathering_with_mode(
        registries,
        state,
        option.definition.id(),
        option.resource,
        mass,
        preview_off_boundary,
    )
}

/// Read-only capacity preflight for a proposed direct gathering amount. This invokes
/// inventory's exact reservation policy without reserving anything; full gathering
/// admission still checks storage, player attention, revisions, and source custody.
pub(crate) fn assess_player_surface_gathering_reservation(
    state: &AppState,
    mass: Mass,
) -> Result<(), SurfaceGatheringError> {
    validate_gathering_player_observer(state)?;
    let destination = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(SurfaceGatheringError::PlayerNotLocated)?;
    validate_inbound_reservation(state.inventory(), destination, mass)
        .map(|_| ())
        .map_err(map_reservation_error)
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
        match self {
            Self::StaleSurfaceRevision { expected, actual } => write!(
                formatter,
                "surface gathering revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "surface gathering inventory revision changed from {expected} to {actual} before commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "surface gathering logistics revision changed from {expected} to {actual} before commit"
            ),
            Self::ResourceChanged => {
                formatter.write_str("surface gathering resource changed before commit")
            }
            Self::PlayerWork(error) => write!(
                formatter,
                "surface gathering player-work commit failed: {error}"
            ),
        }
    }
}

impl Error for SurfaceGatheringCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerWork(error) => Some(error),
            Self::StaleSurfaceRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleLogisticsRevision { .. }
            | Self::ResourceChanged => None,
        }
    }
}

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
    validate_start_surface_gathering_with_mode(registries, state, request, false)
}

fn validate_start_surface_gathering_with_mode(
    registries: &Registries,
    state: &AppState,
    request: SurfaceGatheringRequest,
    preview_off_boundary: bool,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    let method = registries
        .labor()
        .get_surface_gathering(request.method)
        .copied()
        .ok_or(SurfaceGatheringError::UnknownMethod {
            method: request.method,
        })?;
    validate_gathering_player_observer(state)?;
    let resource = state
        .available_surface_resources()
        .find(|resource| resource.id() == request.resource)
        .ok_or(SurfaceGatheringError::ResourceUnavailableAtPlayer {
            resource: request.resource,
        })?;
    if resource.commodity() != method.commodity() {
        return Err(SurfaceGatheringError::CommodityMismatch {
            method: request.method,
            expected: method.commodity(),
            actual: resource.commodity(),
        });
    }
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
    // Natural opportunities and loose drops use the same bare-hand gathering
    // method. Neither deliberate work nor an authored direct-use gesture may
    // bypass the temperature gate on control-rate ground pickup.
    if let Some(blocker) = hand_gathering_temperature_blocker(resource.id(), resource.temperature())
    {
        return Err(blocker);
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
        .checked_add_executable_span(duration)
        .ok_or(SurfaceGatheringError::CompletionTickOverflow)?;
    let entry = MaterialIngressEntry::from_surface_parcel(
        request.mass,
        resource.material_profile().clone(),
        resource.material_provenance(),
        resource
            .ambient_storage_history_at(state.tick())
            .unwrap_or_else(|| unreachable!("surface resource exposure cannot precede creation")),
    );
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
    let proposed_work = PlayerWork::SurfaceGathering { work };
    let player_work = if preview_off_boundary && !state.control_tick().is_simulation_boundary() {
        validate_player_work_start_control_rate(
            registries,
            state,
            proposed_work,
            duration,
            method.exertion(),
        )
    } else {
        validate_player_work_start(
            registries,
            state,
            proposed_work,
            duration,
            method.exertion(),
        )
    }
    .map_err(SurfaceGatheringError::PlayerWork)?;
    Ok(ValidatedSurfaceGatheringStart {
        expected_surface_revision: state.surface().revision(),
        expected_logistics_revision: state.logistics().revision(),
        reservation,
        work,
        player_work,
    })
}

/// Begins hand gathering into the admitted player's carried inventory from an explicit method ID.
///
/// Ordinary player-facing callers should prefer [`player_surface_gathering_options`] plus
/// [`validate_start_player_surface_gathering_option`] so authored method identity remains private.
/// This lower-level wrapper remains available to subsystem composition and controlled tests; carried
/// custody is still derived from authoritative logistics rather than becoming a routing decision.
pub fn validate_start_player_surface_gathering(
    registries: &Registries,
    state: &AppState,
    method: SurfaceGatheringMethodId,
    resource: SurfaceResourceId,
    mass: Mass,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    validate_player_gathering_with_mode(registries, state, method, resource, mass, false)
}

fn validate_player_gathering_with_mode(
    registries: &Registries,
    state: &AppState,
    method: SurfaceGatheringMethodId,
    resource: SurfaceResourceId,
    mass: Mass,
    preview_off_boundary: bool,
) -> Result<ValidatedSurfaceGatheringStart, SurfaceGatheringError> {
    let destination = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(SurfaceGatheringError::PlayerNotLocated)?;
    validate_start_surface_gathering_with_mode(
        registries,
        state,
        SurfaceGatheringRequest::new(method, resource, destination, mass),
        preview_off_boundary,
    )
}
