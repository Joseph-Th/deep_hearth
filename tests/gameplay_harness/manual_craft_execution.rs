//! Canonical manual-craft execution helpers for gameplay episodes.

use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::crafting::{
    ManualCraftRequest, ManualCraftStartRequest, resolve_manual_craft, validate_start_manual_craft,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::manual_craft_selection::select_manual_craft_request;
use super::production_timing::finish_uninterrupted_production_job;

pub(super) fn execute_manual_craft(
    registries: &Registries,
    state: &mut AppState,
    request: ManualCraftRequest,
    destination: StockpileId,
    context: &'static str,
) -> TickSpan {
    let resolution = resolve_manual_craft(registries, state, &request)
        .unwrap_or_else(|error| panic!("gameplay harness {context} resolution failed: {error}"));
    let duration = resolution.duration();
    let expected_condition = resolution.equipment_condition_after();
    let equipment = request.equipment();
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, destination),
    )
    .unwrap_or_else(|error| panic!("gameplay harness {context} start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("gameplay harness {context} commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, duration, context);
    if let (Some(equipment), Some(expected)) = (equipment, expected_condition) {
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(expected),
            "gameplay harness {context} equipment condition diverged from canonical resolution"
        );
    }
    duration
}

pub(super) fn execute_manual_craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: ProcessId,
    source: StockpileId,
    destination: StockpileId,
    batches: u64,
    context: &'static str,
) -> TickSpan {
    let request = select_manual_craft_request(registries, state, process, source, batches, context);
    execute_manual_craft(registries, state, request, destination, context)
}
