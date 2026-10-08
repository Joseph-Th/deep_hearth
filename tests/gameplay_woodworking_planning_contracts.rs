//! Woodworking generation and pre-action policy contracts kept off canonical execution work.

macro_rules! include_woodworking_policy_contract_tests {
    () => {
        #[path = "woodworking_policy_tests.rs"]
        mod tests;
    };
}

#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/primitive_workload.rs"]
mod primitive_workload;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/woodworking_generation.rs"]
mod woodworking_generation;
#[path = "gameplay_harness/woodworking_probe/evaluation_tests.rs"]
mod woodworking_generation_contract_tests;
#[path = "gameplay_harness/woodworking_policy.rs"]
mod woodworking_policy;
