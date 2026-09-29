//! Replayable focused-probe runner shared by the small iteration targets and full gameplay report.

use std::env;

use deep_hearth::content::build_registries;
use deep_hearth::registry::Registries;

use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeSeedPlan, focused_probe_cases_from,
    probe_uses_behavior_seed,
};
use super::focused_witnesses::{
    FIELDWORK_ADAPTIVE_RETOOL_COVERAGE_SEED, FIELDWORK_PROJECT_HORIZON_COVERAGE_SEED,
    FOUNDRY_THERMAL_RECOVERY_COVERAGE_SEED, ORE_FINITE_ENERGY_COVERAGE_SEED,
    PROGRESSION_REFINEMENT_COVERAGE_SEED, PROGRESSION_SURFACE_RESOLVED_COVERAGE_SEED,
};
#[cfg(not(test))]
use super::fresh_seed::fresh_root;
use super::seed::MAINTAINED_VARIATION_ROOT;

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

fn print_simulation_time(registries: &Registries) {
    std::println!(
        "SIMULATION TIME physical-tick-us={}",
        registries.core().physical_tick_duration().microseconds()
    );
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
        "primitive-liberation" => (
            0xD33F_C01D_5052,
            &[
                PROGRESSION_REFINEMENT_COVERAGE_SEED,
                PROGRESSION_SURFACE_RESOLVED_COVERAGE_SEED,
            ],
            0x4C49_4245_5052_4F42,
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
        // baseline visible without relying on organic sampling luck. Seed 7 keeps the ordinary
        // middle project horizon present between those short and bulk anchors. Seed 11 begins on a
        // shallow soft site, then makes new hardness evidence justify salvaging the obsolete pick
        // into a different extraction tool during lived multi-site shortfall recovery.
        "fieldwork" => (
            1,
            &[
                0,
                2,
                3,
                5,
                6,
                FIELDWORK_PROJECT_HORIZON_COVERAGE_SEED,
                FIELDWORK_ADAPTIVE_RETOOL_COVERAGE_SEED,
            ],
            0x4649_454C_4450_5242,
        ),
        // Keep one long-project coverage world because it crosses repeated crusher service and
        // survival provisioning, which short cycles do not exercise together. Organic variation
        // owns broader provider/workload exploration.
        "power-provider" => (
            0xD33F_C01D_907E,
            &[7, 11, 0x10FA_D311_A1B9_7550],
            0x504F_5752_5052_4F42,
        ),
        // Settlement investment keeps one short direct-work anchor and one long mechanization
        // coverage horizon; organic variation fills the crossover between them.
        "settlement" => (
            0xD33F_C01D_5E77,
            &[0x0000_0000_0000_0040],
            0x5345_5454_5052_4F42,
        ),
        "foundry-bootstrap" => (
            0xD33F_C01D_5E77,
            &[0x0000_0000_0000_0040],
            0x464F_554E_5052_4F42,
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
    let explore = env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some();
    if explore {
        print_simulation_time(&registries);
    }
    run_focused_probe_with_registries(
        &registries,
        name,
        probe,
        explore,
        variation_root,
        behavior_root,
    );
}

#[cfg(not(test))]
#[allow(
    dead_code,
    reason = "owner-specific report examples use this convenience entrypoint; the cross-system report reuses one registry through run_focused_probe_with_registries"
)]
pub(super) fn run_focused_report(name: &str, probe: fn(&Registries, FocusedProbeCase)) {
    let registries = build_registries();
    print_simulation_time(&registries);
    let (_maintained_seed, _coverage, salt) = probe_seed_spec(name);
    let variation_root = fresh_root(MAINTAINED_VARIATION_ROOT ^ salt ^ 0x4558_504C_5F57_4F52);
    let behavior_root =
        fresh_root(MAINTAINED_VARIATION_ROOT ^ salt.rotate_left(23) ^ 0x4558_504C_5F42_4856);
    run_focused_probe_with_registries(
        &registries,
        name,
        probe,
        true,
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
    let requested_variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    let variation_count = if explore {
        EXPLORATORY_VARIATION_COUNT
    } else if requested_variation_raw.is_some() {
        1
    } else {
        0
    };
    let variation_raw = (variation_count > 0)
        .then_some(requested_variation_raw)
        .flatten();
    let behavior_raw = (variation_count > 0 || scenario_raw.is_some())
        .then(|| {
            uses_behavior_seed
                .then(|| env::var("DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED").ok())
                .flatten()
        })
        .flatten();
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
                    case.role().label(),
                    case.seed(),
                    behavior_seed,
                )
            } else {
                format!("{}:0x{:016X}", case.role().label(), case.seed(),)
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
                if variation_count == 0 {
                    "n/a".to_owned()
                } else {
                    variation_raw
                        .as_deref()
                        .map_or_else(|| format!("0x{default_variation_root:016X}"), str::to_owned)
                }
            },
            |_| "explicit".to_owned(),
        ),
        if uses_behavior_seed {
            if variation_count == 0 && scenario_raw.is_none() {
                "maintained".to_owned()
            } else {
                behavior_raw
                    .as_deref()
                    .map_or_else(|| format!("0x{default_behavior_root:016X}"), str::to_owned)
            }
        } else {
            "unused".to_owned()
        },
    );

    for case in cases {
        probe(registries, case);
    }
}
