//! Lightweight gameplay-harness contracts kept out of the heavy scenario/probe binaries.

#[path = "gameplay_harness/configuration.rs"]
mod configuration;
#[path = "gameplay_harness/focused_seeds.rs"]
mod focused_seeds;
#[path = "gameplay_harness/seed.rs"]
mod seed;
#[path = "gameplay_harness/seed_input.rs"]
mod seed_input;

#[path = "gameplay_harness/configuration_tests.rs"]
mod configuration_tests;
#[path = "gameplay_harness/fixture_boundary_tests.rs"]
mod fixture_boundary_tests;
#[path = "gameplay_harness/seed_contract_tests.rs"]
mod seed_contract_tests;
