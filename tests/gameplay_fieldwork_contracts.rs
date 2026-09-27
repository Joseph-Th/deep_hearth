//! Fieldwork instrument contracts isolated from the play-like probe binary for fast exact reruns.

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;

#[path = "gameplay_harness/prospecting_instrument_contract_tests.rs"]
mod prospecting_instrument_contract_tests;
