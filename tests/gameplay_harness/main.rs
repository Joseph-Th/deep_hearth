// Headless workshop gameplay harness over the same canonical content registries used by the game.
//
// The harness deliberately varies physical initial conditions and player priorities, then lets a
// small operational policy react only to observed state and resolver projections. The required gate
// runs seven maintained anchor cases plus two deterministic bounded variation cases. The explicit
// report lane uses a larger fresh bounded sample. Every generated root is printed so any result can
// be reproduced. Physical scenario and
// automated-player behavior randomness are independent. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED`
// reproduces the world/scenario sample and `DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED` reproduces policy
// variation. Focused gameplay probes use one maintained anchor plus two independently salted bounded
// physical variations by default; `python ci.py report` supplies a fresh shared replay root while
// ordinary gates retain a stable default. An explicit variation seed reproduces a specific sample and
// `DEEP_HEARTH_GAMEPLAY_SEEDS` provides an exact focused-probe sweep. Each scenario schedules a real
// material transfer into supported storage, so ordinary inventory ownership can change structural
// margin while production is active. The event is not forced after the player's work-order episode has
// already completed or reached a terminal stop.
// The controlled delivery event is hidden from the acting policy until its effects are observable.
// `DEEP_HEARTH_GAMEPLAY_SEEDS` replaces the whole matrix with an exact comma-separated decimal or
// `0x` hexadecimal seed list; malformed entries are rejected instead of ignored. Concise report mode
// keeps aggregate experience evidence, representative workshop highlights, and compact focused-probe
// reviews visible; every workshop scenario plus detailed physical/decision traces are opt-in via
// `DEEP_HEARTH_GAMEPLAY_VERBOSE`.

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

#[cfg(test)]
#[path = "workshop_contract_tests.rs"]
mod workshop_contract_tests;
