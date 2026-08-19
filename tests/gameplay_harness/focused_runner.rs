//! Deterministic focused-probe runner shared by the small iteration targets and aggregate harness.

use std::env;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use deep_hearth::content::build_registries;
use deep_hearth::registry::Registries;

use super::focused_seeds::{MAINTAINED_VARIATION_ROOT, focused_probe_seeds_from};

fn fresh_probe_root(salt: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = (now as u64) ^ ((now >> 64) as u64) ^ u64::from(process::id()) ^ salt;
    super::seed::mix64(folded)
}

fn probe_seed_spec(name: &str) -> (u64, u64) {
    match name {
        "survival-provisioning" => (0xD33F_C01D_5A70, 0x5355_5256_5052_4F42),
        "primitive-progression" => (0xD33F_C01D_5052, 0x5052_4F47_5052_4F42),
        "ore-preparation" => (0xD33F_C01D_0A11, 0x0AE5_1A5E_5052_4F42),
        "foundry" => (0xD33F_C01D_F001, 0xF0A1_DA7A_5052_4F42),
        unknown => panic!("unknown focused gameplay probe {unknown:?}"),
    }
}

pub(super) fn run_focused_probe(name: &str, probe: fn(&Registries, u64)) {
    let (maintained_seed, salt) = probe_seed_spec(name);
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let default_variation_root = fresh_probe_root(MAINTAINED_VARIATION_ROOT ^ salt);
    let seeds = focused_probe_seeds_from(
        scenario_raw.as_deref(),
        variation_raw.as_deref(),
        maintained_seed,
        salt,
        default_variation_root,
    )
    .unwrap_or_else(|error| panic!("gameplay focused probe seed configuration failed: {error:?}"));
    let replay = seeds
        .iter()
        .map(|seed| format!("0x{seed:016X}"))
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "PROBE INPUT name={name} samples={} replay={replay}",
        seeds.len()
    );

    let registries = build_registries();
    for seed in seeds {
        probe(&registries, seed);
    }
}
