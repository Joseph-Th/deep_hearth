//! Focused settlement-machine investment contracts for the fast edit/test loop.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/first_foundry_probe.rs"]
mod first_foundry_probe;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[allow(
    dead_code,
    reason = "settlement-to-foundry scope uses only the available-output half of shared craft planning"
)]
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/powered_craft_planning.rs"]
mod powered_craft_planning;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/settlement_probe.rs"]
mod settlement_probe;
#[path = "gameplay_harness/settlement_scope.rs"]
mod settlement_scope;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[test]
fn gameplay_settlement_probe() {
    focused_runner::run_focused_probe(
        "settlement",
        settlement_scope::run_settlement_progression_scope,
    );
}

#[test]
#[ignore = "exploratory report; run via python ci.py report --scope settlement"]
fn gameplay_settlement_report() {
    focused_runner::run_focused_report(
        "settlement",
        settlement_scope::run_settlement_progression_scope,
    );
}
