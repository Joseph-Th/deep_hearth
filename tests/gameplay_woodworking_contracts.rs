//! Woodworking investment contracts isolated from the play-like probe binary for fast exact reruns.

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

macro_rules! include_woodworking_policy_contract_tests {
    () => {
        #[path = "woodworking_policy_tests.rs"]
        mod tests;
    };
}

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[allow(
    dead_code,
    reason = "contract target exercises investment policy but not report labels or the outer bare-hands reason"
)]
#[path = "gameplay_harness/woodworking_policy.rs"]
mod woodworking_policy;

#[path = "gameplay_harness/saw_bench_contract_tests.rs"]
mod saw_bench_contract_tests;
#[path = "gameplay_harness/woodworking_contract_tests.rs"]
mod woodworking_contract_tests;
