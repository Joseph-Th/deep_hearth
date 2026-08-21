//! Focused ore-preparation target for fast gameplay iteration.

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/support.rs"]
mod support;

use focused_runner::run_focused_probe;
use ore_probe::run_ore_preparation_capability_probe;

#[test]
fn gameplay_ore_preparation_probe() {
    run_focused_probe("ore-preparation", run_ore_preparation_capability_probe);
}
