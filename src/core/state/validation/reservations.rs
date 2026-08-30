//! Reconciles cross-owner material-output reservations against inventory custody.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::StockpileId;

use super::StateValidationError;

#[derive(Default)]
pub(super) struct ExpectedReservations(BTreeMap<StockpileId, Mass>);

impl ExpectedReservations {
    pub(super) fn add(
        &mut self,
        stockpile: StockpileId,
        mass: Mass,
    ) -> Result<(), StateValidationError> {
        let current = self.0.get(&stockpile).copied().unwrap_or(Mass::ZERO);
        let expected = current
            .checked_add(mass)
            .ok_or(StateValidationError::ReservedMassOverflow { stockpile })?;
        self.0.insert(stockpile, expected);
        Ok(())
    }

    fn get(&self, stockpile: StockpileId) -> Mass {
        self.0.get(&stockpile).copied().unwrap_or(Mass::ZERO)
    }
}

pub(super) fn validate_reserved_inbound(
    state: &AppState,
    mut expected: ExpectedReservations,
) -> Result<(), StateValidationError> {
    for job in state.systems.mining.jobs() {
        expected.add(job.destination(), job.output().mass())?;
    }
    for stockpile in state.systems.inventory.stockpiles() {
        let expected_mass = expected.get(stockpile.id());
        if stockpile.reserved_inbound() != expected_mass {
            return Err(StateValidationError::ReservedInboundMismatch {
                stockpile: stockpile.id(),
                reserved: stockpile.reserved_inbound(),
                expected: expected_mass,
            });
        }
    }
    Ok(())
}
