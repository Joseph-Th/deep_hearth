//! Focused survival-provisioning target for fast gameplay iteration.

#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;

use focused_runner::run_focused_probe;
use survival_probe::run_survival_provisioning_probe;

#[test]
fn gameplay_survival_provisioning_probe() {
    run_focused_probe("survival-provisioning", run_survival_provisioning_probe);
}
