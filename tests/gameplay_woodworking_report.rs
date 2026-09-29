//! Focused woodworking exploratory report.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_woodworking.rs"]
mod gameplay_woodworking;

fn main() {
    gameplay_woodworking::run_report();
}
