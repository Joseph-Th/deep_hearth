//! Material-backed prospecting-instrument contracts.
//!
//! These contracts are independent of the full fieldwork episode and keep instrument repair on a
//! small integration-test crate instead of rebuilding the larger fieldwork planning surface.

#[path = "gameplay_harness/environment.rs"]
mod environment;
#[path = "gameplay_harness/inventory_support.rs"]
mod inventory_support;
#[path = "gameplay_harness/ore_fixture.rs"]
mod ore_fixture;
#[path = "gameplay_harness/persistence_timing.rs"]
mod persistence_timing;
#[path = "gameplay_harness/prospecting_instrument_contract_tests.rs"]
mod prospecting_instrument_contract_tests;
#[path = "gameplay_harness/prospecting_timing.rs"]
mod prospecting_timing;
#[path = "gameplay_harness/tick_observation.rs"]
mod tick_observation;
