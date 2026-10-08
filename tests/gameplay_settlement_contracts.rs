//! Settlement-machine investment contracts kept off the frequent lived probe.

#![allow(
    dead_code,
    reason = "contract-only crate reuses settlement modules whose lived-probe entrypoint belongs to the focused target"
)]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
mod output;

#[path = "gameplay_harness/capital_investment_crossover.rs"]
mod capital_investment_crossover;
#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/inherited_condition.rs"]
mod inherited_condition;
#[path = "gameplay_harness/manual_craft_execution.rs"]
mod manual_craft_execution;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/manual_craft_topology_planning.rs"]
mod manual_craft_topology_planning;
#[path = "gameplay_harness/manual_power_timing.rs"]
mod manual_power_timing;
#[path = "gameplay_harness/material_selection.rs"]
mod material_selection;
#[path = "gameplay_harness/powered_craft_planning.rs"]
mod powered_craft_planning;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/settlement_demand.rs"]
mod settlement_demand;
#[path = "gameplay_harness/settlement_drill_contract_tests.rs"]
mod settlement_drill_contract_tests;
#[path = "gameplay_harness/settlement_fixture.rs"]
mod settlement_fixture;
#[path = "gameplay_harness/settlement_generation.rs"]
mod settlement_generation;
#[path = "gameplay_harness/settlement_helve_contract_tests.rs"]
mod settlement_helve_contract_tests;
#[path = "gameplay_harness/settlement_machine_contract_tests.rs"]
mod settlement_machine_contract_tests;
#[path = "gameplay_harness/settlement_power_planning.rs"]
mod settlement_power_planning;
#[path = "gameplay_harness/settlement_wire_contract_tests.rs"]
mod settlement_wire_contract_tests;
#[path = "gameplay_harness/settlement_workshop_investment.rs"]
mod settlement_workshop_investment;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;
