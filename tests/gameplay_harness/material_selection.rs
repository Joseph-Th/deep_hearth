//! Deterministic actor-side material-lot selection policy shared by gameplay probes.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};

/// Selects an exact positive mass from one stockpile in stable lot-ID order.
///
/// This is an actor/harness policy for constructing an explicit production request, not inventory
/// authority. Production validators remain responsible for proving that the returned selections are
/// legal when a consequential operation is admitted.
pub(super) fn select_stockpile_mass(
    state: &AppState,
    stockpile: StockpileId,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    assert!(
        !mass.is_zero(),
        "gameplay harness {context} requires a positive material selection"
    );
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let available = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("gameplay harness {context} output lot disappeared"))
            .mass();
        let selected = Mass::from_milligrams(available.milligrams().min(remaining.milligrams()));
        if selected.is_zero() {
            continue;
        }
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("selected output mass is bounded by remaining demand"));
    }
    assert!(
        remaining.is_zero(),
        "gameplay harness {context} is missing {}mg of the requested runtime output",
        remaining.milligrams()
    );
    selections
}
