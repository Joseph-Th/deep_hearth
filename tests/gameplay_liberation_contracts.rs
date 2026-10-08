//! Primitive-liberation generation and pre-action investment contracts.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    reason = "contract-only crate reuses the liberation evaluator without compiling the lived-probe runner"
)]

macro_rules! include_primitive_liberation_investment_contract_tests {
    () => {
        #[path = "primitive_liberation/investment_tests.rs"]
        mod investment_tests;
    };
}

#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/inherited_condition.rs"]
mod inherited_condition;
#[path = "gameplay_harness/manual_craft_equipment_ranking.rs"]
mod manual_craft_equipment_ranking;
#[path = "gameplay_harness/manual_craft_equipment_topology_planning.rs"]
mod manual_craft_equipment_topology_planning;
#[path = "gameplay_harness/manual_craft_topology_planning.rs"]
mod manual_craft_topology_planning;
#[path = "gameplay_harness/manual_ore_recovery_planning.rs"]
mod manual_ore_recovery_planning;
#[path = "gameplay_harness/primitive_liberation_generation.rs"]
mod primitive_liberation_generation;
#[path = "gameplay_harness/primitive_liberation_investment.rs"]
mod primitive_liberation_investment;
#[path = "gameplay_harness/primitive_liberation_kit_planning.rs"]
mod primitive_liberation_kit_planning;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/settlement_power_planning.rs"]
mod settlement_power_planning;
