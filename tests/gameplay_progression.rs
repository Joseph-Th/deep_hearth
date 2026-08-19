//! Focused primitive-progression target for fast gameplay iteration.

#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/support.rs"]
mod support;

use focused_runner::run_focused_probe;
use progression_probe::run_primitive_progression_probe;

#[test]
fn gameplay_primitive_progression_probe() {
    run_focused_probe("primitive-progression", run_primitive_progression_probe);
}
