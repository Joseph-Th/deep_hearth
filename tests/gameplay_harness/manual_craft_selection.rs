//! Ordinary manual-crafting input selection through the production-owned convenience boundary.

use std::num::NonZeroU64;

use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftInputPlanError, ManualCraftRequest, plan_manual_craft_from_stockpile,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

/// Attempts the same recipe-and-batch input planning used by an ordinary inventory caller.
///
/// In particular, this does not choose between independently sufficient temperature cohorts.
/// Temperature is physical state, so an ambiguous source remains an explicit player choice rather
/// than being resolved by persistent lot order inside the gameplay harness.
pub(super) fn plan_manual_craft_request(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    batches: u64,
) -> Result<ManualCraftRequest, ManualCraftInputPlanError> {
    let batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("gameplay harness requires at least one manual-craft batch"));
    plan_manual_craft_from_stockpile(registries, state, process, source, batches)
}

pub(super) fn select_manual_craft_request(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    batches: u64,
    context: &'static str,
) -> ManualCraftRequest {
    plan_manual_craft_request(registries, state, process, source, batches).unwrap_or_else(|error| {
        panic!("gameplay harness {context} cannot plan the ordinary manual-craft action: {error}")
    })
}
