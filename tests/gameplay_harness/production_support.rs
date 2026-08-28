//! Shared industrial helpers for focused gameplay capability probes.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::maintenance::{CONDITION_PARTS_PER_MILLION, Condition};
use deep_hearth::production::ProductionJobId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

pub(super) fn varied_healthy_condition(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    roll: u64,
) -> Condition {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    let warning = definition
        .maintenance_thresholds()
        .warning_below()
        .parts_per_million();
    let healthy_span = CONDITION_PARTS_PER_MILLION.saturating_sub(warning);
    let lower = warning
        .saturating_add(healthy_span.div_ceil(2))
        .min(CONDITION_PARTS_PER_MILLION);
    let span = CONDITION_PARTS_PER_MILLION - lower;
    let value = lower
        + u32::try_from(roll % (u64::from(span) + 1)).unwrap_or_else(|_| {
            unreachable!("normalized gameplay condition variation always fits u32")
        });
    Condition::new(value)
        .unwrap_or_else(|error| panic!("gameplay harness varied condition failed: {error}"))
}

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

/// Advances an already-admitted capability-probe job whose providers are intentionally stable.
///
/// This helper is not a general actor scheduler. It asserts that no suspension or other world event
/// changes the runtime duration, so callers cannot accidentally hide a support or availability branch
/// behind a generic "finish" utility.
pub(super) fn finish_uninterrupted_production_job(
    registries: &Registries,
    state: &mut AppState,
    job: ProductionJobId,
    resolved_duration: TickSpan,
    context: &'static str,
) {
    let expected_ticks = resolved_duration.value();
    assert!(
        expected_ticks > 0,
        "gameplay harness {context} resolved a zero-tick production job"
    );
    for elapsed in 1..=expected_ticks {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        if state.production().get_job(job).is_none() {
            assert_eq!(
                elapsed, expected_ticks,
                "gameplay harness {context} completed before its resolved duration"
            );
            return;
        }
    }
    panic!(
        "gameplay harness {context} remained active after its resolved {expected_ticks}-tick duration"
    );
}
