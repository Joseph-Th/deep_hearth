//! Exploratory workshop/agency report entry point, separate from the routine workshop test root.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_workshop.rs"]
mod gameplay_workshop;

fn main() {
    gameplay_workshop::run_report();
}
