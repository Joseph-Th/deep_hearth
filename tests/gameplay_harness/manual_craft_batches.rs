//! Batch-count adapter for gameplay episodes that choose a manual process before execution.

use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::inventory::StockpileId;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

use super::manual_craft_execution::execute_manual_craft;
use super::manual_craft_selection::select_manual_craft_request;

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
