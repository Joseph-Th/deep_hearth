//! Replayable focused-probe runner shared by the small iteration targets and full gameplay report.

use std::env;

#[cfg(test)]
use deep_hearth::content::build_registries;
use deep_hearth::registry::Registries;

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeCase, FocusedProbeRole, FocusedProbeSeedPlan,
    GATE_VARIATION_COUNT, focused_probe_cases_from, probe_uses_actor_behavior,
};
#[cfg(test)]
use super::fresh_seed::fresh_root;
#[cfg(test)]
use super::seed::MAINTAINED_VARIATION_ROOT;

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
        // Stable coverage cases protect known behavior. One supplemental organic case keeps the
        // routine gameplay client from proving only the same authored story every run.
        "survival-provisioning" => (0xD33F_C01D_5A70, &[1, 2, 5], 0x5355_5256_5052_4F42),
        "primitive-progression" => (0xD33F_C01D_5052, &[3, 4], 0x5052_4F47_5052_4F42),
        // Coverage spans: no batch-boundary timber gain, rational copper conservation, a one-unit
        // copper shortfall, and a fundable timber-saving order where the timber-priority actor
        // actually builds and uses the saw. Organic cases remain unconstrained.
        "woodworking" => (1, &[3, 4, 12], 0x574F_4F44_5052_4F42),
        // Anchor exercises quarry reinforcement; coverage adds ordinary soft rock and the distinct
        // 750 MPa hard-pick specialist path. Organic worlds remain free to land in any tier.
        "fieldwork" => (1, &[2, 3], 0x4649_454C_4450_5242),
        "ore-preparation" => (0xD33F_C01D_0A11, &[2], 0x0AE5_1A5E_5052_4F42),
        "foundry" => (0xD33F_C01D_F001, &[2], 0xF0A1_DA7A_5052_4F42),
        unknown => panic!("unknown focused gameplay probe {unknown:?}"),
    }
}

#[cfg(test)]
pub(super) fn run_focused_probe(name: &str, probe: fn(&Registries, FocusedProbeCase)) {
    let registries = build_registries();
    let (_maintained_seed, _coverage, salt) = probe_seed_spec(name);
    let variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ salt ^ 0x4741_5445_5F57_4F52);
    let behavior_root =
        fresh_root(MAINTAINED_VARIATION_ROOT ^ salt.rotate_left(23) ^ 0x4741_5445_5F42_4856);
    run_focused_probe_with_registries(
        &registries,
        name,
        probe,
        false,
        variation_root,
        behavior_root,
    );
}

pub(super) fn run_focused_probe_with_registries(
    registries: &Registries,
    name: &str,
    probe: fn(&Registries, FocusedProbeCase),
    explore: bool,
    default_variation_root: u64,
    default_behavior_root: u64,
) {
    let (maintained_seed, maintained_coverage_seeds, salt) = probe_seed_spec(name);
    let uses_actor_behavior = probe_uses_actor_behavior(name);
    let behavior_root = if uses_actor_behavior {
        Some(default_behavior_root)
    } else {
        None
    };
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let behavior_raw = uses_actor_behavior
        .then(|| env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok())
        .flatten();
    let variation_count = if explore {
        EXPLORATORY_VARIATION_COUNT
    } else if variation_raw.is_some() {
        1
    } else {
        GATE_VARIATION_COUNT
    };
    let cases = focused_probe_cases_from(FocusedProbeSeedPlan {
        variation_count,
        scenario_raw: scenario_raw.as_deref(),
        variation_raw: variation_raw.as_deref(),
        behavior_raw: behavior_raw.as_deref(),
        maintained_seed,
        maintained_coverage_seeds,
        probe_salt: salt,
        default_variation_root,
        default_behavior_root: behavior_root,
    })
    .unwrap_or_else(|error| panic!("gameplay focused probe seed configuration failed: {error:?}"));
    let replay = cases
        .iter()
        .map(|case| {
            if uses_actor_behavior {
                let behavior_seed = case.behavior_seed().unwrap_or_else(|| {
                    panic!("focused actor probe {name:?} lost its behavior seed")
                });
                format!(
                    "{}:0x{:016X}@0x{:016X}",
                    focused_probe_role_label(case.role()),
                    case.seed(),
                    behavior_seed,
                )
            } else {
                format!(
                    "{}:0x{:016X}",
                    focused_probe_role_label(case.role()),
                    case.seed(),
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    std::println!(
        "PROBE INPUT name={name} mode={} samples={} organic={} world_root={} behavior_root={} replay={replay}",
        if explore { "explore" } else { "gate" },
        cases.len(),
        cases
            .iter()
            .filter(|case| case.role() == FocusedProbeRole::OrganicVariation)
            .count(),
        scenario_raw.as_deref().map_or_else(
            || {
                variation_raw
                    .as_deref()
                    .map_or_else(|| format!("0x{default_variation_root:016X}"), str::to_owned)
            },
            |_| "explicit".to_owned(),
        ),
        if uses_actor_behavior {
            behavior_raw
                .as_deref()
                .map_or_else(|| format!("0x{default_behavior_root:016X}"), str::to_owned)
        } else {
            "unused".to_owned()
        },
    );

    for case in cases {
        probe(registries, case);
    }
}
