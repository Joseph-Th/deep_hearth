// Headless workshop gameplay harness over the same canonical content registries used by the game.
//
// The harness deliberately varies physical initial conditions and player priorities, then lets a
// small operational policy react only to observed state and resolver projections. The required gate
// runs seven maintained anchor cases plus one deterministic bounded variation case. The explicit
// report lane uses a larger fresh bounded sample. Every generated root is printed so any result can
// be reproduced. Physical scenario and
// automated-player behavior randomness are independent. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`
// reproduces the world/scenario sample and `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` reproduces policy
// variation. Focused gameplay probes use one maintained anchor plus one deterministic bounded physical
// variation by default; an explicit variation seed reproduces a specific sample and
// `DEEP_HEARTH_GAMEPLAY_SEEDS` provides an exact focused-probe sweep. Each scenario schedules a real
// material transfer into supported storage, so ordinary inventory ownership can change structural
// margin while production is active.
// The controlled delivery event is hidden from the acting policy until its effects are observable.
// `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the whole matrix with an exact comma-separated decimal or
// `0x` hexadecimal seed list; malformed entries are rejected instead of ignored. Detailed trace
// output is opt-in via `DEEP_HEARTH_GAMEPLAY_VERBOSE`.

use std::collections::BTreeSet;
use std::env;

mod agency;
mod capability_boundary;
mod configuration;
mod contracts;
mod focused_seeds;
mod fresh_seed;
mod industrial_support;
mod report;
mod scenario;
mod seed;
#[cfg(test)]
mod seed_contract_tests;
mod seed_input;
mod support;

use configuration::ScenarioPlanMode;

fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
}

macro_rules! println {
    ($($argument:tt)*) => {{
        if has_verbose_output() {
            std::println!($($argument)*);
        }
    }};
}

mod workshop;
use workshop::run_gameplay_harness;

use deep_hearth::content::{
    PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_MELT_PURE_COPPER, PROCESS_SCREEN_CRUSHED_ORE,
    build_registries,
};
use deep_hearth::maintenance::Condition;

fn condition(parts_per_million: u32) -> Condition {
    Condition::new(parts_per_million)
        .unwrap_or_else(|error| panic!("gameplay harness condition is invalid: {error}"))
}

#[test]
fn gameplay_harness_gate() {
    run_gameplay_harness(ScenarioPlanMode::Gate);
}

#[test]
fn gameplay_machine_process_catalog_has_cold_agent_evidence() {
    let registries = build_registries();
    let manual_processes = registries
        .crafting()
        .definitions()
        .map(|definition| definition.process())
        .collect::<BTreeSet<_>>();
    let actual_machine_processes = registries
        .production()
        .definitions()
        .map(|definition| definition.id())
        .filter(|process| !manual_processes.contains(process))
        .collect::<BTreeSet<_>>();
    let exercised_machine_processes = BTreeSet::from([
        PROCESS_CRUSH_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
    ]);
    assert_eq!(
        actual_machine_processes, exercised_machine_processes,
        "cold-agent capability coverage is stale: update workshop/ore/foundry probes so every authored non-manual production process has gameplay evidence"
    );
}

#[test]
#[ignore = "exploratory gameplay report"]
fn gameplay_harness_exploratory_report() {
    run_gameplay_harness(ScenarioPlanMode::Explore);
}
