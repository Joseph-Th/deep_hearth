//! Finite-energy reservation and cross-owner occupancy admission.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{
    ConsumedEnergyTrace, EnergyConsumptionReservation, EnergyIngressReservation,
    EnergyIngressReservationError, EnergyReservationError, EnergyStoreId, EnergyStoreOccupancy,
    ReleasedEnergyTrace, energy_store_occupancy, validate_energy_consumption_reservation,
    validate_energy_ingress_reservation,
};
use crate::production::ProcessResolution;
use crate::registry::Registries;

use super::super::StartProcessError;

#[must_use]
pub(in super::super) struct ValidatedEnergyReservations {
    pub(in super::super) consumption: Option<EnergyConsumptionReservation>,
    pub(in super::super) ingress: Option<EnergyIngressReservation>,
    pub(in super::super) consumed: Option<ConsumedEnergyTrace>,
    pub(in super::super) released: Option<ReleasedEnergyTrace>,
}

pub(in super::super) fn validate_energy_reservations(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    completes_at: SimulationTick,
) -> Result<ValidatedEnergyReservations, StartProcessError> {
    let consumption = match resolution.energy_supply() {
        Some(selection) => Some(
            validate_energy_consumption_reservation(state.energy(), selection)
                .map_err(map_energy_reservation_error)?,
        ),
        None => None,
    };
    let ingress = match resolution.energy_sink() {
        Some(selection) => Some(
            validate_energy_ingress_reservation(
                registries,
                state.energy(),
                selection,
                resolution.duration(),
            )
            .map_err(map_energy_ingress_reservation_error)?,
        ),
        None => None,
    };
    let consumed = consumption
        .as_ref()
        .map(EnergyConsumptionReservation::trace);
    let released = ingress.map(EnergyIngressReservation::trace);
    let post_admission_revision = state
        .energy()
        .revision()
        .checked_add(u64::from(consumption.is_some()))
        .ok_or(StartProcessError::EnergyRevisionExhausted)?;
    let post_nonproduction_revision = post_admission_revision
        .checked_add(state.future_nonproduction_energy_revision_demand())
        .ok_or(StartProcessError::EnergyRevisionExhausted)?;
    if released.is_some()
        && !state
            .production()
            .has_scheduled_released_energy_revision_capacity_with_tick_from(
                post_nonproduction_revision,
                completes_at,
            )
    {
        return Err(StartProcessError::EnergyRevisionExhausted);
    }
    if released.is_none()
        && !state
            .production()
            .has_scheduled_released_energy_revision_capacity_from(post_nonproduction_revision)
    {
        return Err(StartProcessError::EnergyRevisionExhausted);
    }
    for store in consumed
        .map(|trace| trace.source())
        .into_iter()
        .chain(released.map(|trace| trace.destination()))
    {
        validate_energy_store_available(state, store)?;
    }
    Ok(ValidatedEnergyReservations {
        consumption,
        ingress,
        consumed,
        released,
    })
}

fn validate_energy_store_available(
    state: &AppState,
    store: EnergyStoreId,
) -> Result<(), StartProcessError> {
    match energy_store_occupancy(state, store) {
        Some(EnergyStoreOccupancy::Production { job, release }) => {
            return Err(StartProcessError::EnergyStoreBusy {
                store,
                job,
                release,
            });
        }
        Some(EnergyStoreOccupancy::ManualPower) => {
            return Err(StartProcessError::EnergyStoreBusyManualPower { store });
        }
        None => {}
    }
    Ok(())
}

fn map_energy_ingress_reservation_error(error: EnergyIngressReservationError) -> StartProcessError {
    match error {
        EnergyIngressReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedEnergy {
                expected_energy_revision: expected,
                actual_energy_revision: actual,
            }
        }
        EnergyIngressReservationError::UnknownStore { store: _ } => {
            StartProcessError::ResolvedEnergySinkMissing
        }
        EnergyIngressReservationError::CapacityOverflow { store: _ }
        | EnergyIngressReservationError::InsufficientCapacity { .. } => {
            StartProcessError::ResolvedEnergySinkCapacity
        }
    }
}

fn map_energy_reservation_error(error: EnergyReservationError) -> StartProcessError {
    match error {
        EnergyReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedEnergy {
                expected_energy_revision: expected,
                actual_energy_revision: actual,
            }
        }
        EnergyReservationError::UnknownStore { store: _ } => {
            StartProcessError::ResolvedEnergyStoreMissing
        }
        EnergyReservationError::InsufficientEnergy { .. } => {
            StartProcessError::ResolvedEnergyInsufficient
        }
        EnergyReservationError::RevisionExhausted => StartProcessError::EnergyRevisionExhausted,
    }
}
