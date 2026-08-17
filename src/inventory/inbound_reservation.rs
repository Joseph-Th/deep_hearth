//! Inventory-owned capacity reservation for conserved matter held by another runtime owner.

use crate::core::quantity::Mass;

use super::state::{InventoryState, StockpileId, get_stockpile_mut_or_panic};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InboundReservationError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedInboundReservation {
    expected_revision: u64,
    next_revision: u64,
    stockpile: StockpileId,
    mass: Mass,
}

impl ValidatedInboundReservation {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn apply(self, state: &mut InventoryState) {
        assert_eq!(state.revision(), self.expected_revision);
        let record = get_stockpile_mut_or_panic(state, self.stockpile);
        record.reserved_inbound = match record.reserved_inbound.checked_add(self.mass) {
            Some(value) => value,
            None => panic!("validated inbound reservation overflowed at commit"),
        };
        state.apply_revision(self.next_revision);
    }
}

pub(crate) fn validate_inbound_reservation(
    state: &InventoryState,
    stockpile: StockpileId,
    mass: Mass,
) -> Result<ValidatedInboundReservation, InboundReservationError> {
    let record = state
        .get_stockpile(stockpile)
        .ok_or(InboundReservationError::UnknownStockpile { stockpile })?;
    let committed = record
        .stored_mass()
        .checked_add(record.reserved_inbound())
        .ok_or(InboundReservationError::MassOverflow { stockpile })?;
    let after = committed
        .checked_add(mass)
        .ok_or(InboundReservationError::MassOverflow { stockpile })?;
    if after > record.capacity() {
        return Err(InboundReservationError::CapacityExceeded {
            stockpile,
            capacity: record.capacity(),
            committed,
            requested: mass,
        });
    }
    let expected_revision = state.revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InboundReservationError::RevisionExhausted)?;
    Ok(ValidatedInboundReservation {
        expected_revision,
        next_revision,
        stockpile,
        mass,
    })
}
