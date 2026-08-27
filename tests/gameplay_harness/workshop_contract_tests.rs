//! Workshop gameplay contracts shared by the focused workshop target and consolidated audit target.

use deep_hearth::content::build_registries;
use deep_hearth::maintenance::Condition;

use super::configuration::ScenarioPlanMode;
use super::{scenario, workshop};

fn condition(parts_per_million: u32) -> Condition {
    Condition::new(parts_per_million)
        .unwrap_or_else(|error| panic!("gameplay harness condition is invalid: {error}"))
}

#[test]
fn gameplay_harness_gate() {
    workshop::run_gameplay_harness(ScenarioPlanMode::Gate);
}

#[test]
fn gameplay_terminal_prework_stop_does_not_plan_unreachable_work_or_wait_for_hidden_event() {
    let registries = build_registries();
    let mut variation = scenario::ScenarioVariation::from_seeds(&registries, 4, 1, None);
    variation.crusher.initial_crusher_condition = condition(1);
    variation.crusher.maintenance_replacement_units = 0;
    variation.delivery.delivery_at_tick = 64;

    let report = workshop::run_scenario(&registries, variation);

    assert!(report.limits.maintenance_stop);
    assert!(!report.progress.delivery_applied);
    assert_eq!(report.progress.operations_completed, 0);
    assert_eq!(report.resources.elapsed_ticks, 0);
    assert!(report.resources.elapsed_ticks < report.inputs.delivery_at_tick);
    assert_eq!(report.resources.metabolic_energy_spent.nanojoules(), 0);
    assert_eq!(report.resources.hydration_spent.microliters(), 0);
}
