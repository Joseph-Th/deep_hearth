//! Foundry generator contracts kept independent from canonical execution work.

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/equipment_support.rs"]
mod equipment_support;
#[path = "gameplay_harness/foundry_generation_contract_tests.rs"]
mod foundry_generation_contract_tests;
#[path = "gameplay_harness/foundry_probe_generation.rs"]
mod foundry_probe_generation;
#[path = "gameplay_harness/production_support.rs"]
mod production_support;
#[allow(
    dead_code,
    reason = "foundry generation contracts reuse mix64 without the broader seed-plan policy"
)]
#[path = "gameplay_harness/seed.rs"]
mod seed;
