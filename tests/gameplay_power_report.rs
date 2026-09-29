//! Focused power-provider exploratory report.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_power.rs"]
mod gameplay_power;

fn main() {
    gameplay_power::run_report();
}
