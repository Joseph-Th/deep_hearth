//! Minimal stockpile fixtures shared by gameplay harnesses.

use deep_hearth::content::gameplay_fixture::seed_stockpile;
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};

pub(super) fn add_solid_stockpile(state: &mut AppState, capacity: Mass) -> StockpileId {
    seed_stockpile(
        state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    )
}
