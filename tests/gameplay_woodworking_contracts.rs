//! Woodworking acquisition, policy, and lifecycle contracts kept off the frequent lived probe.

macro_rules! include_woodworking_policy_contract_tests {
    () => {
        #[path = "woodworking_policy_tests.rs"]
        mod tests;
    };
}

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/manual_craft_batches.rs"]
mod manual_craft_batches;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/persistence_timing.rs"]
mod persistence_timing;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/saw_bench_contract_tests.rs"]
mod saw_bench_contract_tests;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/woodworking_contract_tests.rs"]
mod woodworking_contract_tests;
#[path = "gameplay_harness/woodworking_generation.rs"]
mod woodworking_generation;
#[path = "gameplay_harness/woodworking_probe/evaluation_tests.rs"]
mod woodworking_generation_contract_tests;
#[path = "gameplay_harness/woodworking_policy.rs"]
mod woodworking_policy;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;
