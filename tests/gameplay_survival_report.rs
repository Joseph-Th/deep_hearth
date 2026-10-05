//! Report-only survival gameplay rendering.

#![cfg(not(test))]

#[macro_use]
#[path = "gameplay_harness/output.rs"]
pub(crate) mod output;
#[path = "gameplay_survival.rs"]
mod gameplay_survival;

fn main() {
    gameplay_survival::run_report();
}
