//! Replayable focused-probe runner shared by the small iteration targets and full gameplay report.

use std::env;

#[cfg(test)]
use deep_hearth::content::build_registries;
use deep_hearth::registry::Registries;

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeCase, FocusedProbeRole, FocusedProbeSeedPlan,
    GATE_VARIATION_COUNT, focused_probe_cases_from, probe_uses_behavior_seed,
};

pub(super) const PROGRESSION_REFINEMENT_COVERAGE_SEED: u64 = 3;
pub(super) const PROGRESSION_SURFACE_RESOLVED_COVERAGE_SEED: u64 = 4;
pub(super) const ORE_FINITE_ENERGY_COVERAGE_SEED: u64 = 2;
pub(super) const FOUNDRY_THERMAL_RECOVERY_COVERAGE_SEED: u64 = 2;
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

fn maintained_behavior_override(name: &str, case: FocusedProbeCase) -> Option<u64> {
    match (name, case.role(), case.seed()) {
        // This world exposes the full timber preservation frontier. The maintained actor is
        // deliberately patient but not all-in on material, so the double-wall chest wins between
        // the fast field box and stronger pantry. Keep this deterministic witness while organic
        // behavior remains independently varied from the fresh behavior root.
        ("survival-provisioning", FocusedProbeRole::MaintainedCoverage, 0x0000_0000_0000_0002) => {
            Some(0xAB2C_977A_0B20_C7A3)
        }
        // This second choice-rich preservation world protects the opposite endpoint. The actor
        // has enough disclosed timber to build the strongest pantry and values its much longer
        // fresh-food window enough to accept the additional construction attention and matter.
        ("survival-provisioning", FocusedProbeRole::MaintainedCoverage, 0x0000_0000_0000_0006) => {
            Some(0x0274_20B1_9FB8_38F7)
        }
        _ => None,
    }
}

fn probe_seed_spec(name: &str) -> (u64, &'static [u64], u64) {
    match name {
        // Stable survival coverage protects pressure response plus preservation choice shape:
        // cheapest/strongest endpoints, one true intermediate value-frontier choice, explicit
        // decline, and the distinct stone-only crock opportunity. Organic variation still owns
        // broad exploration.
        "survival-provisioning" => (
            0xD33F_C01D_5A70,
            &[1, 2, 5, 6, 0x043C_561D_398D_32BA, 0xF495_6470_1464_3BC2],
            0x5355_5256_5052_4F42,
        ),
        "primitive-progression" => (
            0xD33F_C01D_5052,
            &[
                PROGRESSION_REFINEMENT_COVERAGE_SEED,
                PROGRESSION_SURFACE_RESOLVED_COVERAGE_SEED,
            ],
            0x5052_4F47_5052_4F42,
        ),
        // Coverage spans break-even net-timber investment, setup-budget rejection,
        // outright copper blocking, protected-reserve refusal despite a profitable saw route,
        // a short queued job just below the saw crossover, a long saw-to-adze fallback, and a
        // copper-rich pipeline that actually replaces a worn blade.
        "woodworking" => (
            1,
            &[3, 4, 6, 12, 14, 250, 0x36F7_E3A2_7870_3A8A],
            0x574F_4F44_5052_4F42,
        ),
        // Maintained fieldwork spans all four extraction tools, soft/reinforcement/hard-specialist
        // geology, and one-, two-, and three-site survey horizons. Paired reinforcement worlds
        // expose both sides of heavy-tool investment: a large visible reserve selects the
        // reinforced quarry, while a small localized reserve cuts the same nominal project back to
        // the lighter hard pick before construction. Seed 5 keeps the short soft-rock stone-pick
        // baseline visible without relying on organic sampling luck.
        "fieldwork" => (1, &[0, 2, 3, 5, 6], 0x4649_454C_4450_5242),
        // Keep one long-project coverage world because it crosses repeated crusher service and
        // survival provisioning; this caught meal-time reserve planning that short cycles cannot.
        // Organic variation still owns broad provider/workload exploration.
        "power-provider" => (
            0xD33F_C01D_907E,
            &[7, 11, 0x10FA_D311_A1B9_7550],
            0x504F_5752_5052_4F42,
        ),
        "ore-preparation" => (
            0xD33F_C01D_0A11,
            &[ORE_FINITE_ENERGY_COVERAGE_SEED],
            0x0AE5_1A5E_5052_4F42,
        ),
        "foundry" => (
            0xD33F_C01D_F001,
            &[FOUNDRY_THERMAL_RECOVERY_COVERAGE_SEED],
            0xF0A1_DA7A_5052_4F42,
        ),
        unknown => panic!("unknown focused gameplay probe {unknown:?}"),
    }
}

#[cfg(test)]
pub(super) fn run_focused_probe(name: &str, probe: fn(&Registries, FocusedProbeCase)) {
    let registries = build_registries();
    let (_maintained_seed, _coverage, salt) = probe_seed_spec(name);
    let variation_root = MAINTAINED_VARIATION_ROOT ^ salt ^ 0x4741_5445_5F57_4F52;
    let behavior_root = MAINTAINED_VARIATION_ROOT ^ salt.rotate_left(23) ^ 0x4741_5445_5F42_4856;
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
    let uses_behavior_seed = probe_uses_behavior_seed(name);
    let behavior_root = if uses_behavior_seed {
        Some(default_behavior_root)
    } else {
        None
    };
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let behavior_raw = uses_behavior_seed
        .then(|| env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok())
        .flatten();
    let variation_count = if explore {
        EXPLORATORY_VARIATION_COUNT
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
    let cases = cases
        .into_iter()
        .map(|case| {
            maintained_behavior_override(name, case).map_or(case, |behavior_seed| {
                FocusedProbeCase::new(case.seed(), Some(behavior_seed), case.role())
            })
        })
        .collect::<Vec<_>>();
    let replay = cases
        .iter()
        .map(|case| {
            if uses_behavior_seed {
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
        if uses_behavior_seed {
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
