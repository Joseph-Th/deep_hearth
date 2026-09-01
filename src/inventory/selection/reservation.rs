//! Revision-bound custody reservation and commit for already-selected inventory matter.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::MaterialInputSpec;

use super::ConsumptionSelection;
use crate::inventory::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialStorageHistory, StockpileId,
    apply_aggregate_withdraw, apply_consume_lot_slice, checked_consumed_material_mass,
    get_stockpile_mut_or_panic,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReservationError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed_after_consumption: Mass,
        requested_inbound: Mass,
    },
    RevisionExhausted,
    StaleSelection {
        expected: u64,
        actual: u64,
    },
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConsumptionReservation {
    pub(super) expected_revision: u64,
    pub(super) next_revision: u64,
    pub(super) source: StockpileId,
    pub(super) inputs: Vec<MaterialInputSpec>,
    pub(super) lot_slices: Vec<LotSlice>,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) inbound_by_destination: BTreeMap<StockpileId, Mass>,
}

impl ConsumptionReservation {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    /// Captures the oldest ambient-equivalent storage exposure represented by this exact
    /// reservation at `at`. Production uses the conservative oldest cohort because generic
    /// process streams do not retain a one-to-one lineage from every selected input lot to every
    /// output lot.
    pub(crate) fn oldest_storage_history_at(
        &self,
        state: &InventoryState,
        at: SimulationTick,
    ) -> Option<MaterialStorageHistory> {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "storage exposure must be captured from the reservation's validated inventory revision"
        );
        let source = state.get_stockpile(self.source).unwrap_or_else(|| {
            panic!(
                "validated reservation source stockpile {} disappeared",
                self.source.value()
            )
        });
        let preservation_multiplier_ppm = source.storage_profile().preservation_multiplier_ppm();
        let mut oldest_ambient_age_parts = 0_u128;
        for slice in &self.lot_slices {
            let lot = state.get_lot(slice.lot).unwrap_or_else(|| {
                panic!(
                    "validated reservation references missing material lot {}",
                    slice.lot.value()
                )
            });
            let age = lot
                .storage_history()
                .project(at, preservation_multiplier_ppm)?;
            oldest_ambient_age_parts = oldest_ambient_age_parts.max(age);
        }
        Some(MaterialStorageHistory::with_ambient_age_parts(
            oldest_ambient_age_parts,
            at,
        ))
    }
}

pub(crate) fn validate_consumption_reservation_from_selection(
    state: &InventoryState,
    selection: ConsumptionSelection,
    inbound_by_destination: BTreeMap<StockpileId, Mass>,
) -> Result<ConsumptionReservation, ReservationError> {
    let ConsumptionSelection {
        expected_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    } = selection;
    if state.revision() != expected_revision {
        return Err(ReservationError::StaleSelection {
            expected: expected_revision,
            actual: state.revision(),
        });
    }
    let Some(next_revision) = state.revision().checked_add(1) else {
        return Err(ReservationError::RevisionExhausted);
    };
    let total_consumed = checked_consumed_material_mass(&consumed_inputs)
        .ok_or(ReservationError::MassOverflow { stockpile: source })?;

    for (destination, inbound_mass) in &inbound_by_destination {
        let Some(destination_record) = state.get_stockpile(*destination) else {
            return Err(ReservationError::UnknownStockpile {
                stockpile: *destination,
            });
        };
        let outgoing = if source == *destination {
            total_consumed
        } else {
            Mass::ZERO
        };
        let projection = destination_record
            .project_mass_exchange(outgoing, *inbound_mass)
            .ok_or(ReservationError::MassOverflow {
                stockpile: *destination,
            })?;
        if projection.after_incoming > destination_record.capacity() {
            return Err(ReservationError::CapacityExceeded {
                stockpile: *destination,
                capacity: destination_record.capacity(),
                committed_after_consumption: projection.committed_before_incoming,
                requested_inbound: *inbound_mass,
            });
        }
    }

    Ok(ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs,
        inbound_by_destination,
    })
}

#[cfg(test)]
pub(crate) fn apply_consumption_reservation(
    state: &mut InventoryState,
    reservation: ConsumptionReservation,
) -> Result<(), ReservationCommitError> {
    if state.revision() != reservation.expected_revision {
        return Err(ReservationCommitError::StaleInventoryRevision {
            expected: reservation.expected_revision,
            actual: state.revision(),
        });
    }
    apply_prechecked_consumption_reservation(state, reservation);
    Ok(())
}

/// Applies a reservation after a surrounding multi-owner transaction has checked its revision.
///
/// This form is intentionally infallible: callers must perform every recoverable stale-state check
/// before mutating any other owner. The assertion protects that ordering contract from internal
/// misuse without introducing a post-mutation error path.
pub(crate) fn apply_prechecked_consumption_reservation(
    state: &mut InventoryState,
    reservation: ConsumptionReservation,
) {
    reservation.assert_matches_state(state);
    let ConsumptionReservation {
        expected_revision,
        next_revision,
        source,
        inputs,
        lot_slices,
        consumed_inputs: _consumed_inputs,
        inbound_by_destination,
    } = reservation;
    assert_eq!(
        state.revision(),
        expected_revision,
        "prechecked consumption reservation requires its validated inventory revision"
    );

    for input in &inputs {
        apply_aggregate_withdraw(state, source, input.commodity(), input.mass());
    }
    for slice in lot_slices {
        apply_consume_lot_slice(state, slice);
    }

    for (destination, inbound_mass) in inbound_by_destination {
        let destination_record = get_stockpile_mut_or_panic(state, destination);
        destination_record.reserved_inbound = match destination_record
            .reserved_inbound
            .checked_add(inbound_mass)
        {
            Some(value) => value,
            None => panic!(
                "validated reservation overflowed stockpile {} inbound mass",
                destination.value()
            ),
        };
    }
    state.apply_revision(next_revision);
}
