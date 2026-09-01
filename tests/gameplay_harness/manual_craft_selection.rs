//! Deterministic actor-side lot selection for manual-crafting requests.

use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::ManualCraftRequest;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;

fn first_sufficient_pure_temperature(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    required_mass: Mass,
    context: &'static str,
) -> Temperature {
    let expected_composition = MaterialComposition::pure(commodity.material());
    let mut temperatures = Vec::<(Temperature, Mass)>::new();
    for lot in state.inventory().lot_ids(stockpile) {
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("gameplay harness {context} input lot disappeared"));
        if record.commodity() != commodity || record.composition() != &expected_composition {
            continue;
        }
        if let Some((_, mass)) = temperatures
            .iter_mut()
            .find(|(temperature, _)| *temperature == record.temperature())
        {
            *mass = mass
                .checked_add(record.mass())
                .unwrap_or_else(|| panic!("gameplay harness {context} eligible mass overflowed"));
        } else {
            temperatures.push((record.temperature(), record.mass()));
        }
    }
    temperatures
        .into_iter()
        .find(|(_, mass)| *mass >= required_mass)
        .map(|(temperature, _)| temperature)
        .unwrap_or_else(|| {
            panic!(
                "gameplay harness {context} has no homogeneous pure {} batch of {}mg",
                commodity.value(),
                required_mass.milligrams()
            )
        })
}

fn select_pure_mass_at_temperature(
    state: &AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    temperature: Temperature,
    mass: Mass,
    context: &'static str,
) -> Vec<MaterialLotSelection> {
    let expected_composition = MaterialComposition::pure(commodity.material());
    let mut remaining = mass;
    let mut selections = Vec::new();
    for lot in state.inventory().lot_ids(stockpile) {
        if remaining.is_zero() {
            break;
        }
        let record = state
            .inventory()
            .get_lot(lot)
            .unwrap_or_else(|| panic!("gameplay harness {context} selected lot disappeared"));
        if record.commodity() != commodity
            || record.composition() != &expected_composition
            || record.temperature() != temperature
        {
            continue;
        }
        let selected =
            Mass::from_milligrams(record.mass().milligrams().min(remaining.milligrams()));
        selections.push(MaterialLotSelection::new(lot, selected));
        remaining = remaining
            .checked_sub(selected)
            .unwrap_or_else(|| unreachable!("manual craft selection cannot exceed remaining mass"));
    }
    assert!(remaining.is_zero());
    selections
}

/// Builds one explicit manual-crafting request from player-observable lot state.
///
/// Manual crafting preserves the selected batch temperature and requires pure authored input
/// material. The actor therefore chooses a homogeneous eligible temperature group instead of asking
/// inventory to allocate arbitrary matching commodity mass. Canonical crafting validation still
/// proves lot ownership, exact mass, composition, temperature consistency, and process legality.
pub(super) fn select_manual_craft_request(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    batches: u64,
    context: &'static str,
) -> ManualCraftRequest {
    assert!(
        batches > 0,
        "gameplay harness {context} requires at least one manual-craft batch"
    );
    let definition = registries
        .crafting()
        .get_manual(process)
        .unwrap_or_else(|| panic!("gameplay harness {context} references unknown manual process"));
    let required_mass = Mass::from_milligrams(
        definition
            .input_mass()
            .milligrams()
            .checked_mul(batches)
            .unwrap_or_else(|| panic!("gameplay harness {context} craft input mass overflowed")),
    );
    let selected_temperature = first_sufficient_pure_temperature(
        state,
        source,
        definition.input(),
        required_mass,
        context,
    );
    let selections = select_pure_mass_at_temperature(
        state,
        source,
        definition.input(),
        selected_temperature,
        required_mass,
        context,
    );
    ManualCraftRequest::new(process, source, selections)
}
