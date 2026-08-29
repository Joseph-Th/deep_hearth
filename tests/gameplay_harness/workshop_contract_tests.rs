//! Contract tests shared by the focused workshop target and consolidated gameplay audit.

use deep_hearth::content::build_registries;
use deep_hearth::core::quantity::Mass;
use deep_hearth::maintenance::Condition;

use super::configuration::{
    MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT, ScenarioPlanMode, scenario_seeds_from,
};
use super::{scenario, workshop};

fn condition(parts_per_million: u32) -> Condition {
    Condition::new(parts_per_million)
        .unwrap_or_else(|error| panic!("gameplay harness condition is invalid: {error}"))
}

#[test]
fn gameplay_harness_gate() {
    workshop::run_gameplay_harness(ScenarioPlanMode::Gate);
}

#[test]
fn gameplay_terminal_prework_stop_does_not_plan_unreachable_work_or_wait_for_hidden_event() {
    let registries = build_registries();
    let mut variation = scenario::ScenarioVariation::from_seeds(&registries, 4, 1, None);
    variation.crusher.initial_crusher_condition = condition(1);
    variation.crusher.maintenance_replacement_units = 0;
    variation.delivery.delivery_at_tick = 64;

    let report = workshop::run_scenario(&registries, variation, None);

    assert!(report.limits.maintenance_stop);
    assert!(!report.progress.delivery_applied);
    assert_eq!(report.progress.operations_completed, 0);
    assert_eq!(report.resources.elapsed_ticks, 0);
    assert!(report.resources.elapsed_ticks < report.inputs.delivery_at_tick);
    assert_eq!(report.resources.metabolic_energy_spent.nanojoules(), 0);
    assert_eq!(report.resources.hydration_spent.microliters(), 0);
}

#[test]
fn hidden_delivery_payload_does_not_change_pre_event_actor_choices() {
    let registries = build_registries();
    let plan = scenario_seeds_from(
        ScenarioPlanMode::Gate,
        None,
        None,
        None,
        MAINTAINED_VARIATION_ROOT,
        MAINTAINED_BEHAVIOR_ROOT,
    )
    .unwrap_or_else(|error| panic!("maintained hidden-delivery seed plan failed: {error:?}"));
    let (baseline, baseline_report) = plan
        .cases()
        .iter()
        .filter_map(|case| {
            let anchor = case.anchor?;
            let variation = scenario::ScenarioVariation::from_seeds(
                &registries,
                case.world_seed,
                case.behavior_seed,
                Some(anchor),
            );
            let report = workshop::run_scenario(&registries, variation, None);
            report
                .progress
                .delivery_applied
                .then_some((variation, report))
        })
        .next()
        .unwrap_or_else(|| {
            panic!("maintained workshop anchors contain no reached hidden delivery")
        });
    let mut alternate = baseline;
    alternate.delivery.destination_is_compact = !baseline.delivery.destination_is_compact;
    alternate.delivery.mass = Mass::from_milligrams(
        baseline
            .delivery
            .mass
            .milligrams()
            .checked_add(1)
            .unwrap_or_else(|| panic!("hidden-delivery counterfactual mass overflowed")),
    );

    let alternate_report = workshop::run_scenario(&registries, alternate, None);

    assert_ne!(
        baseline_report.inputs.delivery_mass,
        alternate_report.inputs.delivery_mass
    );
    assert_ne!(
        baseline_report.inputs.delivery_is_compact,
        alternate_report.inputs.delivery_is_compact
    );
    assert!(baseline_report.progress.delivery_applied);
    assert!(alternate_report.progress.delivery_applied);
    assert_eq!(
        baseline_report.inputs.delivery_at_tick, alternate_report.inputs.delivery_at_tick,
        "hidden delivery payload must not alter controller event timing"
    );
    assert_eq!(
        baseline_report.progress.operations_before_delivery,
        alternate_report.progress.operations_before_delivery,
        "actor must make the same number of pre-event workshop decisions when only hidden delivery payload changes"
    );
    assert_eq!(
        baseline_report.choices.chose_compact_support,
        alternate_report.choices.chose_compact_support,
        "initial support choice must depend only on observable structural state"
    );
}
