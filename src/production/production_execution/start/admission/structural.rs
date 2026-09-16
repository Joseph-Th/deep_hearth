//! Source stored-matter structural-load admission for production start.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
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

pub(in super::super) fn validate_structural_revision_budget(
    state: &AppState,
    source_load: Option<&ValidatedStockpileStructuralLoad>,
    destination_structure_revision: Option<u64>,
    completes_at: SimulationTick,
) -> Result<(), StartProcessError> {
    let admission_steps = source_load.map_or(0, ValidatedStockpileStructuralLoad::revision_delta);
    let post_admission_revision = state
        .structures()
        .revision()
        .checked_add(admission_steps)
        .ok_or(StartProcessError::StructureRevisionExhausted)?;
    if !state
        .production()
        .has_revision_capacity_for_scheduled_ticks(
            post_admission_revision,
            state
                .production()
                .jobs()
                .filter(|job| {
                    !job.is_suspended()
                        && job.output_streams().iter().any(|stream| {
                            state
                                .inventory()
                                .get_stockpile(stream.destination())
                                .is_some_and(|stockpile| stockpile.supported_by().is_some())
                        })
                })
                .map(crate::production::ProductionJobRecord::completes_at)
                .chain(destination_structure_revision.map(|_| completes_at)),
        )
    {
        return Err(StartProcessError::StructureRevisionExhausted);
    }
    Ok(())
}
