//! Replayable focused-probe runner shared by the small iteration targets and full gameplay report.

use std::env;

use deep_hearth::content::build_registries;
use deep_hearth::registry::Registries;

use super::focused_seeds::{
    FocusedProbeCase, FocusedProbeRole, MAINTAINED_VARIATION_ROOT, focused_probe_cases_from,
};
use super::fresh_seed::fresh_root;

pub(super) const fn focused_probe_role_label(role: FocusedProbeRole) -> &'static str {
    match role {
        FocusedProbeRole::MaintainedAnchor => "anchor",
        FocusedProbeRole::MaintainedCoverage => "coverage",
        FocusedProbeRole::OrganicVariation => "organic",
        FocusedProbeRole::ExplicitReplay => "replay",
    }
}

fn probe_seed_spec(name: &str) -> (u64, &'static [u64], u64) {
    match name {
        // Coverage worlds are stable, ordinary probe cases chosen because they exercise a structural
        // alternative to the primary anchor. Fresh organic worlds remain in every non-replay run.
        "survival-provisioning" => (0xD33F_C01D_5A70, &[1, 2, 3], 0x5355_5256_5052_4F42),
        "primitive-progression" => (0xD33F_C01D_5052, &[3], 0x5052_4F47_5052_4F42),
        "ore-preparation" => (0xD33F_C01D_0A11, &[2], 0x0AE5_1A5E_5052_4F42),
        "foundry" => (0xD33F_C01D_F001, &[2], 0xF0A1_DA7A_5052_4F42),
        unknown => panic!("unknown focused gameplay probe {unknown:?}"),
    }
}

pub(super) fn run_focused_probe(name: &str, probe: fn(&Registries, FocusedProbeCase)) {
    let registries = build_registries();
    let (_, _, salt) = probe_seed_spec(name);
    let default_variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ salt.rotate_left(13));
    run_focused_probe_with_registries(&registries, name, probe, default_variation_root);
}

pub(super) fn run_focused_probe_with_registries(
    registries: &Registries,
    name: &str,
    probe: fn(&Registries, FocusedProbeCase),
    default_variation_root: u64,
) {
    let (maintained_seed, maintained_coverage_seeds, salt) = probe_seed_spec(name);
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let cases = focused_probe_cases_from(
        scenario_raw.as_deref(),
        variation_raw.as_deref(),
        maintained_seed,
        maintained_coverage_seeds,
        salt,
        default_variation_root,
    )
    .unwrap_or_else(|error| panic!("gameplay focused probe seed configuration failed: {error:?}"));
    let replay = cases
        .iter()
        .map(|case| {
            format!(
                "{}:0x{:016X}",
                focused_probe_role_label(case.role()),
                case.seed()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "PROBE INPUT name={name} samples={} replay={replay}",
        cases.len()
    );

    for case in cases {
        probe(registries, case);
    }
}
