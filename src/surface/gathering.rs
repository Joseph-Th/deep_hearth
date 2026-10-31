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

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{
    InboundReservationError, MaterialIngressEntry, MaterialIngressError, StockpileId,
    StockpileStorageError, ValidatedInboundReservation, validate_inbound_reservation,
    validate_material_ingress,
};
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, SurfaceGatheringDefinition,
    SurfaceGatheringMethodId, SurfaceGatheringWork, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::logistics::{PlayerStockpileAccessError, validate_player_stockpile_access};
use crate::material::{CommodityKey, MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE};
use crate::registry::Registries;
use crate::survival::{
    FoodCategory, FoodFreshness, FoodFreshnessError, SurvivalExertion, freshness_from_history,
};

use super::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

/// Actor-facing option for gathering one currently visible loose natural opportunity.
///
/// The authored method identity remains private. Callers can compare the physical commitment and
/// choose a resource/amount without depending on internal method IDs; canonical gathering
/// validation still owns admission, capacity, survival, reservations, and completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSurfaceGatheringOption {
    expected_surface_revision: u64,
    resource: SurfaceResourceId,
    source_mass: Mass,
    temperature: Temperature,
    definition: SurfaceGatheringDefinition,
}

/// Food value of one exact-local, ordinary direct gathering batch, before accepting the work.
///
/// This is not a guarantee of carrying space, future freshness, or eating the food.
/// The present freshness is evaluated from the actual local source history. The separate
/// shelf-life baseline describes a newly gathered fresh batch under ambient storage.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerSurfaceFoodValue {
    batch_mass: Mass,
    dietary_energy: Energy,
    category: FoodCategory,
    hydration_multiplier_ppm: u32,
    fresh_ambient_shelf_life: TickSpan,
    current_freshness: FoodFreshness,
}

impl PlayerSurfaceFoodValue {
    #[must_use]
    pub const fn batch_mass(self) -> Mass {
        self.batch_mass
    }

    #[must_use]
    pub const fn dietary_energy(self) -> Energy {
        self.dietary_energy
    }

    #[must_use]
    pub const fn category(self) -> FoodCategory {
        self.category
    }

    #[must_use]
    pub const fn hydration_multiplier_ppm(self) -> u32 {
        self.hydration_multiplier_ppm
    }

    #[must_use]
    pub const fn fresh_ambient_shelf_life(self) -> TickSpan {
        self.fresh_ambient_shelf_life
    }

    #[must_use]
    pub const fn current_freshness(self) -> FoodFreshness {
        self.current_freshness
    }
}

impl PlayerSurfaceGatheringOption {
    /// Current physical temperature of the observed local source.
    #[must_use]
    pub const fn temperature(self) -> Temperature {
        self.temperature
    }

    /// An immediately knowable blocker for unprotected hand gathering.
    /// The authoritative work admission checks the same rule again.
    #[must_use]
    pub fn hand_handling_blocker(self) -> Option<SurfaceGatheringError> {
        hand_gathering_temperature_blocker(self.resource, self.temperature)
    }

    /// Compares edible value using only the current local source, authored food definition,
    /// and source's physical aging history. A stale option cannot silently describe old
    /// resource custody; inedible gathering options return `Ok(None)`.
    pub fn food_value(
        self,
        registries: &Registries,
        state: &AppState,
    ) -> Result<Option<PlayerSurfaceFoodValue>, SurfaceGatheringError> {
        let actual_surface_revision = state.surface().revision();
        if actual_surface_revision != self.expected_surface_revision {
            return Err(SurfaceGatheringError::StalePlayerOption {
                expected_surface_revision: self.expected_surface_revision,
                actual_surface_revision,
            });
        }
        let resource = state
            .available_surface_resources()
            .find(|resource| resource.id() == self.resource)
            .ok_or(SurfaceGatheringError::ResourceUnavailableAtPlayer {
                resource: self.resource,
            })?;
        let Some(food) = registries.survival().get_food(self.commodity()) else {
            return Ok(None);
        };
        let batch_mass = self.source_mass.min(self.definition.direct_batch_mass());
        let history = resource
            .ambient_storage_history_at(state.tick())
            .unwrap_or_else(|| {
                unreachable!("a currently visible source cannot predate its physical creation")
            });
        let current_freshness = freshness_from_history(
            history,
            NORMALIZED_PARTS_PER_MILLION,
            state.tick(),
            food.shelf_life(),
        )
        .map_err(|error| match error {
            FoodFreshnessError::ShelfLifeOverflow => SurfaceGatheringError::FoodAgeOverflow,
            FoodFreshnessError::UnknownLot { .. }
            | FoodFreshnessError::UnknownStockpile { .. }
            | FoodFreshnessError::NotEdible { .. } => {
                unreachable!("known local food cannot lose its food identity")
            }
        })?;
        Ok(Some(PlayerSurfaceFoodValue {
            batch_mass,
            dietary_energy: food.dietary_energy_for_mass(batch_mass),
            category: food.category(),
            hydration_multiplier_ppm: food.hydration_multiplier_ppm(),
            fresh_ambient_shelf_life: food.shelf_life(),
            current_freshness,
        }))
    }

    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.definition.commodity()
    }

    #[must_use]
    pub const fn source_mass(self) -> Mass {
        self.source_mass
    }

    #[must_use]
    pub const fn maximum_batch_mass(self) -> Mass {
        self.definition.maximum_batch_mass()
    }

    #[must_use]
    pub const fn direct_batch_mass(self) -> Mass {
        self.definition.direct_batch_mass()
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.definition.exertion()
    }

    #[must_use]
    pub fn duration_for_mass(self, mass: Mass) -> Option<TickSpan> {
        self.definition.duration_for_mass(mass)
    }
}

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
    StalePlayerOption {
        expected_surface_revision: u64,
        actual_surface_revision: u64,
    },
    UnknownMethod {
        method: SurfaceGatheringMethodId,
    },
    ResourceUnavailableAtPlayer {
        resource: SurfaceResourceId,
    },
    CommodityMismatch {
        method: SurfaceGatheringMethodId,
        expected: CommodityKey,
        actual: CommodityKey,
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
    ResourceTooHot {
        resource: SurfaceResourceId,
        temperature: Temperature,
        maximum: Temperature,
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
    FoodAgeOverflow,
    PlayerWork(PlayerWorkStartError),
}

fn hand_gathering_temperature_blocker(
    resource: SurfaceResourceId,
    temperature: Temperature,
) -> Option<SurfaceGatheringError> {
    (temperature > MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE).then_some(
        SurfaceGatheringError::ResourceTooHot {
            resource,
            temperature,
            maximum: MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        },
    )
}

impl Display for SurfaceGatheringError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotLocated => {
                formatter.write_str("surface gathering requires an admitted player location")
            }
            Self::StalePlayerOption {
                expected_surface_revision,
                actual_surface_revision,
            } => write!(
                formatter,
                "player surface-gathering option was projected at surface revision {expected_surface_revision} but current revision is {actual_surface_revision}"
            ),
            Self::UnknownMethod { method } => write!(
                formatter,
                "unknown surface gathering method {}",
                method.value()
            ),
            Self::ResourceUnavailableAtPlayer { resource } => write!(
                formatter,
                "surface resource {} is not available at the player's location",
                resource.value()
            ),
            Self::CommodityMismatch {
                method,
                expected,
                actual,
            } => write!(
                formatter,
                "surface gathering method {} expects commodity {} but resource contains {}",
                method.value(),
                expected.value(),
                actual.value()
            ),
            Self::ZeroMass => formatter.write_str("surface gathering mass must be nonzero"),
            Self::BatchTooLarge { requested, maximum } => write!(
                formatter,
                "surface gathering requested {} mg but the method permits at most {} mg per batch",
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::InsufficientResource {
                requested,
                remaining,
            } => write!(
                formatter,
                "surface gathering requested {} mg but only {} mg remains",
                requested.milligrams(),
                remaining.milligrams()
            ),
            Self::ResourceTooHot {
                resource,
                temperature,
                maximum,
            } => write!(
                formatter,
                "surface resource {} is {} mK and exceeds the {} mK unprotected hand-gathering limit",
                resource.value(),
                temperature.millikelvin(),
                maximum.millikelvin(),
            ),
            Self::DestinationAccess(error) => write!(
                formatter,
                "surface gathering destination is inaccessible: {error}"
            ),
            Self::DestinationMissing { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} is missing",
                stockpile.value()
            ),
            Self::DestinationMounted { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} is mounted and cannot receive direct gathering",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "surface gathering destination cannot store the resource: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "surface gathering destination stockpile {} mass overflowed",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "surface gathering destination stockpile {} has {} mg capacity with {} mg committed and cannot reserve another {} mg",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::MaterialLotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted for surface gathering"),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted for surface gathering")
            }
            Self::SurfaceRevisionExhausted => {
                formatter.write_str("surface revision space is exhausted for gathering")
            }
            Self::CompletionTickOverflow => {
                formatter.write_str("surface gathering completion tick overflowed")
            }
            Self::FoodAgeOverflow => {
                formatter.write_str("surface food opportunity aging cannot be projected")
            }
            Self::PlayerWork(error) => write!(
                formatter,
                "surface gathering cannot reserve player work: {error}"
            ),
        }
    }
}

impl Error for SurfaceGatheringError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationAccess(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::PlayerNotLocated
            | Self::StalePlayerOption { .. }
            | Self::UnknownMethod { .. }
            | Self::ResourceUnavailableAtPlayer { .. }
            | Self::CommodityMismatch { .. }
            | Self::ZeroMass
            | Self::BatchTooLarge { .. }
            | Self::InsufficientResource { .. }
            | Self::ResourceTooHot { .. }
            | Self::DestinationMissing { .. }
            | Self::DestinationMounted { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::SurfaceRevisionExhausted
            | Self::CompletionTickOverflow
            | Self::FoodAgeOverflow => None,
        }
    }
}

/// Projects current exact-local natural gathering choices for the admitted player.
///
/// Player-created drops are excluded because their ordinary path is automatic pickup. A resource
/// with multiple authored gathering methods yields one option per method so callers can surface a
/// real technique choice or reject ambiguity rather than silently selecting an implementation ID.
pub fn player_surface_gathering_options(
    registries: &Registries,
    state: &AppState,
) -> Result<Vec<PlayerSurfaceGatheringOption>, SurfaceGatheringError> {
    if state.logistics().player().is_none() {
        return Err(SurfaceGatheringError::PlayerNotLocated);
    }
    let expected_surface_revision = state.surface().revision();
    let mut options = Vec::new();
    for resource in state
        .available_surface_resources()
        .filter(|resource| resource.kind() == SurfaceResourceKind::Opportunity)
    {
        for definition in registries
            .labor()
            .surface_gathering_for_commodity(resource.commodity())
            .copied()
        {
            options.push(PlayerSurfaceGatheringOption {
                expected_surface_revision,
                resource: resource.id(),
                source_mass: resource.remaining_mass(),
                temperature: resource.temperature(),
                definition,
            });
        }
    }
    Ok(options)
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
    let actual_surface_revision = state.surface().revision();
    if actual_surface_revision != option.expected_surface_revision {
        return Err(SurfaceGatheringError::StalePlayerOption {
            expected_surface_revision: option.expected_surface_revision,
            actual_surface_revision,
        });
    }
    validate_start_player_surface_gathering(
        registries,
        state,
        option.definition.id(),
        option.resource,
        mass,
    )
}

/// Read-only capacity preflight for a proposed direct gathering amount. This invokes
/// inventory's exact reservation policy without reserving anything; full gathering
/// admission still checks storage, player attention, revisions, and source custody.
pub(crate) fn assess_player_surface_gathering_reservation(
    state: &AppState,
    mass: Mass,
) -> Result<(), SurfaceGatheringError> {
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
        .checked_add_span(duration)
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
    let destination = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(SurfaceGatheringError::PlayerNotLocated)?;
    validate_start_surface_gathering(
        registries,
        state,
        SurfaceGatheringRequest::new(method, resource, destination, mass),
    )
}
