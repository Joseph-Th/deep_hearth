//! Exploratory workshop report kept separate so report-only formatting stays out of the hot test target.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_workshop.rs"]
mod gameplay_workshop;

fn main() {
    gameplay_workshop::run_report();
}
