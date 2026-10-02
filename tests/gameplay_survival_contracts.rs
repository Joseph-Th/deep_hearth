//! Survival generation and preservation contracts kept off the focused provisioning probe build.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_survival_probe_contract_tests {
    () => {
        #[path = "survival_probe/provisioning_support_tests.rs"]
        mod provisioning_support_tests;
    };
}

#[path = "gameplay_harness/direct_consumption_timing.rs"]
mod direct_consumption_timing;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[allow(
    dead_code,
    reason = "survival contracts use the shared case generator without focused-runner sampling helpers"
)]
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[allow(
    dead_code,
    reason = "the shared witness catalog contains maintained seeds for every gameplay owner"
)]
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/preservation_route.rs"]
mod preservation_route;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[allow(
    dead_code,
    reason = "survival contracts need seed mixing through focused cases but not the focused runner root"
)]
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/survival_contract_tests.rs"]
mod survival_contract_tests;
#[path = "gameplay_harness/survival_probe.rs"]
mod survival_probe;
#[path = "gameplay_harness/temporal.rs"]
mod temporal;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

// Keep the shared probe implementation live for lint purposes without adding a redundant runtime
// test. This target owns private contracts inside that graph; the focused target owns execution.
const _: fn(&deep_hearth::registry::Registries, focused_case::FocusedProbeCase) =
    survival_probe::run_survival_provisioning_probe;
