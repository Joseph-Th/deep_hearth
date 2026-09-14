//! Source stored-matter structural-load admission for production start.

use crate::core::state::AppState;
use crate::inventory::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes,
};
use crate::production::ProcessResolution;
use crate::registry::Registries;

use super::super::StartProcessError;

pub(in super::super) fn validate_source_structural_load(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<Option<ValidatedStockpileStructuralLoad>, StartProcessError> {
    let source = resolution.source();
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(StartProcessError::UnknownStockpile { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(resolution.input_mass())
        .ok_or(StartProcessError::MassOverflow { stockpile: source })?;
    validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StartProcessError::StructuralLoad)
}
