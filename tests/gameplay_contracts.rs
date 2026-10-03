//! Lightweight gameplay-harness contracts kept out of the heavy scenario/probe binaries.

#[path = "gameplay_harness/capital_investment_policy.rs"]
mod capital_investment_policy;
#[path = "gameplay_harness/catalog.rs"]
mod catalog;
#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/exact_local_runtime.rs"]
mod exact_local_runtime;
#[path = "gameplay_harness/focused_case.rs"]
mod focused_case;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/manual_craft_selection.rs"]
mod manual_craft_selection;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;
#[path = "gameplay_harness/world_admission.rs"]
mod world_admission;

#[path = "gameplay_harness/capital_investment_policy_tests.rs"]
mod capital_investment_policy_tests;
#[path = "gameplay_harness/configuration_tests.rs"]
mod configuration_tests;
#[path = "gameplay_harness/fixture_boundary_tests.rs"]
mod fixture_boundary_tests;
#[path = "gameplay_harness/manual_craft_selection_tests.rs"]
mod manual_craft_selection_tests;
#[path = "gameplay_harness/primitive_liberation_contract_tests.rs"]
mod primitive_liberation_contract_tests;
#[path = "gameplay_harness/process_catalog_contract_tests.rs"]
mod process_catalog_contract_tests;
#[path = "gameplay_harness/seed_contract_tests.rs"]
mod seed_contract_tests;
