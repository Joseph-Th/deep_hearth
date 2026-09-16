//! Source stored-matter structural-load admission for production start.

use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionReservation, StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;

use super::super::StartProcessError;

pub(in super::super) fn validate_source_structural_load(
    registries: &Registries,
    state: &AppState,
    reservation: &ConsumptionReservation,
) -> Result<Option<ValidatedStockpileStructuralLoad>, StartProcessError> {
    let source = reservation.source();
    let source_after = reservation.source_stored_mass_after(state.inventory());
    validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StartProcessError::StructuralLoad)
}
