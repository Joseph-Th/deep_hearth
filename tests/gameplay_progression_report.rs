//! Focused primitive-progression exploratory report.
#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_progression.rs"]
mod gameplay_progression;

fn main() {
    gameplay_progression::run_report();
}
