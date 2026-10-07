//! Focused settlement report including later-workshop specialization decisions.
#![cfg(not(test))]

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
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_runner.rs"]
mod focused_runner;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/focused_witnesses.rs"]
mod focused_witnesses;
#[path = "gameplay_harness/fresh_seed.rs"]
mod fresh_seed;
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
#[path = "gameplay_harness/physical_time.rs"]
mod physical_time;
#[path = "gameplay_harness/powered_craft_planning.rs"]
mod powered_craft_planning;
#[path = "gameplay_harness/production_timing.rs"]
mod production_timing;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
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
#[path = "gameplay_harness/settlement_power_planning.rs"]
mod settlement_power_planning;
#[path = "gameplay_harness/settlement_probe.rs"]
mod settlement_probe;
#[path = "gameplay_harness/settlement_storage_progression.rs"]
mod settlement_storage_progression;
#[path = "gameplay_harness/settlement_wire_contract_tests.rs"]
mod settlement_wire_contract_tests;
#[path = "gameplay_harness/settlement_workshop_investment.rs"]
mod settlement_workshop_investment;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

fn configured_variation_root() -> Option<u64> {
    std::env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED")
        .ok()
        .map(|raw| {
            seed_input::parse_seed(&raw)
                .unwrap_or_else(|| panic!("settlement report variation seed is invalid: {raw:?}"))
        })
}

fn main() {
    focused_runner::run_focused_report("settlement", settlement_probe::run_settlement_probe);

    let variation_root = configured_variation_root();
    settlement_drill_contract_tests::run_spindle_drill_investment_experience(variation_root);
    settlement_wire_contract_tests::run_wire_drawbench_investment_experience(variation_root);
    settlement_helve_contract_tests::run_treadle_hammer_investment_experience(variation_root);
    settlement_helve_contract_tests::run_helve_hammer_investment_experience(variation_root);
    settlement_workshop_investment::run_lathe_investment_experience(variation_root);
    settlement_workshop_investment::run_toolroom_investment_experience(variation_root);
    settlement_storage_progression::run_settlement_storage_progression_experience();
}
