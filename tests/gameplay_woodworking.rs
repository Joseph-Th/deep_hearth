//! Focused woodworking gameplay target for the fast edit/test loop.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/maintenance_timing.rs"]
mod maintenance_timing;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[allow(
    dead_code,
    reason = "focused target intentionally omits other consumers of shared planning helpers"
)]
#[path = "gameplay_harness/manual_craft_planning.rs"]
mod manual_craft_planning;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/woodworking_probe.rs"]
mod woodworking_probe;

#[cfg(test)]
#[test]
fn gameplay_woodworking_probe() {
    focused_runner::run_focused_probe("woodworking", woodworking_probe::run_woodworking_probe);
}

#[test]
#[ignore = "exploratory report; run via python ci.py report --scope woodworking"]
fn gameplay_woodworking_report() {
    focused_runner::run_focused_report("woodworking", woodworking_probe::run_woodworking_probe);
}
