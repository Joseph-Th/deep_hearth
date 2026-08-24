//! Workshop gameplay contracts shared by the focused workshop target and consolidated audit target.

use std::collections::BTreeSet;

use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CONCENTRATE_COPPER, PROCESS_CRUSH_ORE,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER,
    PROCESS_SCREEN_CRUSHED_ORE, PROCESS_SEPARATE_NATIVE_COPPER, build_registries,
};
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
    assert_eq!(report.inputs.delivery_at_tick, 1);
    assert_eq!(report.resources.metabolic_energy_spent.nanojoules(), 0);
    assert_eq!(report.resources.hydration_spent.microliters(), 0);
}

#[test]
fn gameplay_machine_process_catalog_has_cold_agent_evidence() {
    let registries = build_registries();
    let manual_processes = registries
        .crafting()
        .definitions()
        .map(|definition| definition.process())
        .collect::<BTreeSet<_>>();
    let actual_machine_processes = registries
        .production()
        .definitions()
        .map(|definition| definition.id())
        .filter(|process| !manual_processes.contains(process))
        .collect::<BTreeSet<_>>();
    let exercised_machine_processes = BTreeSet::from([
        PROCESS_CRUSH_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_CONCENTRATE_COPPER,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
        PROCESS_SEPARATE_NATIVE_COPPER,
    ]);
    assert_eq!(
        actual_machine_processes, exercised_machine_processes,
        "cold-agent capability coverage is stale: update progression/workshop/ore/foundry probes so every authored non-manual production process has gameplay evidence"
    );
}
