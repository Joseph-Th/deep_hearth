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

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedInboundReservation {
    expected_revision: u64,
    next_revision: u64,
    stockpile: StockpileId,
    mass: Mass,
}

impl ValidatedInboundReservation {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn assert_matches_state(&self, state: &InventoryState) {
        assert_eq!(state.revision(), self.expected_revision);
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "inbound reservation must advance the inventory revision exactly once"
        );
        let record = state.get_stockpile(self.stockpile).unwrap_or_else(|| {
            panic!(
                "validated inbound reservation stockpile {} disappeared",
                self.stockpile.value()
            )
        });
        let projection = record
            .project_mass_exchange(Mass::ZERO, self.mass)
            .unwrap_or_else(|| panic!("validated inbound reservation mass projection overflowed"));
        assert!(
            projection.after_incoming <= record.capacity(),
            "validated inbound reservation exceeds stockpile capacity"
        );
    }

    pub(crate) fn apply(self, state: &mut InventoryState) {
        self.assert_matches_state(state);
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
    let projection = record
        .project_mass_exchange(Mass::ZERO, mass)
        .ok_or(InboundReservationError::MassOverflow { stockpile })?;
    if projection.after_incoming > record.capacity() {
        return Err(InboundReservationError::CapacityExceeded {
            stockpile,
            capacity: record.capacity(),
            committed: projection.committed_before_incoming,
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
