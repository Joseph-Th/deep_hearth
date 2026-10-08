//! Woodworking acquisition and lifecycle contracts kept off the frequent lived probe.

#[path = "gameplay_harness/environment.rs"]
mod environment;
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
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/saw_bench_contract_tests.rs"]
mod saw_bench_contract_tests;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/woodworking_contract_tests.rs"]
mod woodworking_contract_tests;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;
