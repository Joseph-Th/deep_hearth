//! Input-matter reservation and work-in-process storage-history admission.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ConsumptionReservation, MaterialStorageHistory,
    ReservationError, StockpileId, validate_consumption_reservation_from_selection,
};
use crate::production::ProcessResolution;

use super::super::StartProcessError;

#[must_use]
pub(in super::super) struct ValidatedMaterialReservation {
    pub(in super::super) reservation: ConsumptionReservation,
    pub(in super::super) storage_history: MaterialStorageHistory,
}

pub(in super::super) fn validate_material_reservation(
    state: &AppState,
    resolution: &ProcessResolution,
    inbound_by_destination: BTreeMap<StockpileId, Mass>,
    completes_at: SimulationTick,
) -> Result<ValidatedMaterialReservation, StartProcessError> {
    let reservation = validate_consumption_reservation_from_selection(
        state.inventory(),
        resolution.selection().clone(),
        inbound_by_destination,
    )
    .map_err(map_reservation_error)?;
    // Production consumes/reserves inventory at admission and deterministically lands the reserved
    // output at completion. The generic reservation owns only the admission mutation, so budget the
    // second production-specific inventory mutation here.
    reservation
        .expected_revision()
        .checked_add(2)
        .ok_or(StartProcessError::InventoryRevisionExhausted)?;
    let storage_history = reservation
        .oldest_storage_history_at(state.inventory(), state.tick())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: validated production inputs have unprojectable storage history")
        });
    assert!(
        storage_history
            .project(completes_at, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
            .is_some(),
        "runtime invariant broken: physically reachable production input history must project through scheduled completion"
    );
    Ok(ValidatedMaterialReservation {
        reservation,
        storage_history,
    })
}

fn map_reservation_error(error: ReservationError) -> StartProcessError {
    match error {
        ReservationError::UnknownStockpile { stockpile } => {
            StartProcessError::UnknownStockpile { stockpile }
        }
        ReservationError::MassOverflow { stockpile } => {
            StartProcessError::MassOverflow { stockpile }
        }
        ReservationError::CapacityExceeded {
            stockpile,
            capacity,
            committed_after_consumption,
            requested_inbound,
        } => StartProcessError::CapacityExceeded {
            stockpile,
            capacity,
            committed_after_consumption,
            requested_inbound,
        },
        ReservationError::RevisionExhausted => StartProcessError::InventoryRevisionExhausted,
        ReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedInputs {
                expected_inventory_revision: expected,
                actual_inventory_revision: actual,
            }
        }
    }
}
