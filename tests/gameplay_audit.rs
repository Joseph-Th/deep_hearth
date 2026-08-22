//! Consolidated broad-audit target for the four focused gameplay probes.
//!
//! Focused developer gates keep their independent binaries so a survival edit does not relink ore,
//! foundry, or progression code. The explicit broad audit compiles those shared probe modules once
//! here instead of linking four separate executables that provide the same aggregate checkpoint.

#[path = "gameplay_harness/capability_boundary.rs"]
mod capability_boundary;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/foundry_probe.rs"]
mod foundry_probe;
#[path = "gameplay_harness/foundry_setup.rs"]
mod foundry_setup;
#[path = "gameplay_harness/industrial_support.rs"]
mod industrial_support;
#[path = "gameplay_harness/ore_probe.rs"]
mod ore_probe;
#[path = "gameplay_harness/ore_setup.rs"]
mod ore_setup;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[path = "gameplay_harness/progression_probe.rs"]
mod progression_probe;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/support.rs"]
mod support;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;

#[cfg(test)]
mod focused {
    use super::focused_runner::run_focused_probe;
    use super::foundry_probe::run_foundry_capability_probe;
    use super::ore_probe::run_ore_preparation_capability_probe;
    use super::progression_probe::run_primitive_progression_probe;
    use super::survival_probe::run_survival_provisioning_probe;

    #[test]
    fn gameplay_survival_provisioning_probe() {
        run_focused_probe("survival-provisioning", run_survival_provisioning_probe);
    }

    #[test]
    fn gameplay_primitive_progression_probe() {
        run_focused_probe("primitive-progression", run_primitive_progression_probe);
    }

    #[test]
    fn gameplay_ore_preparation_probe() {
        run_focused_probe("ore-preparation", run_ore_preparation_capability_probe);
    }

    #[test]
    fn gameplay_foundry_probe() {
        run_focused_probe("foundry", run_foundry_capability_probe);
    }
}
