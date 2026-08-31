//! Provides registry-derived condition variation and bounded production advancement for gameplay probes.

use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::maintenance::{CONDITION_PARTS_PER_MILLION, Condition};
use deep_hearth::production::ProductionJobId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

pub(super) fn varied_healthy_condition(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    roll: u64,
) -> Condition {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    let warning = definition
        .maintenance_thresholds()
        .warning_below()
        .parts_per_million();
    let healthy_span = CONDITION_PARTS_PER_MILLION.saturating_sub(warning);
    let lower = warning
        .saturating_add(healthy_span.div_ceil(2))
        .min(CONDITION_PARTS_PER_MILLION);
    let span = CONDITION_PARTS_PER_MILLION - lower;
    let value = lower
        + u32::try_from(roll % (u64::from(span) + 1)).unwrap_or_else(|_| {
            unreachable!("normalized gameplay condition variation always fits u32")
        });
    Condition::new(value)
        .unwrap_or_else(|error| panic!("gameplay harness varied condition failed: {error}"))
}

/// Advances an already-admitted capability-probe job whose providers are intentionally stable.
///
/// This function is not a general actor scheduler. It asserts that no suspension or other world event
/// changes the runtime duration, so callers cannot hide a support or availability branch behind a
/// generic completion path.
pub(super) fn finish_uninterrupted_production_job(
    registries: &Registries,
    state: &mut AppState,
    job: ProductionJobId,
    resolved_duration: TickSpan,
    context: &'static str,
) {
    let expected_ticks = resolved_duration.value();
    assert!(
        expected_ticks > 0,
        "gameplay harness {context} resolved a zero-tick production job"
    );
    for elapsed in 1..=expected_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert!(
            !outcome
                .production_availability_changes()
                .iter()
                .any(|change| {
                    matches!(
                        change,
                        deep_hearth::production::ProductionAvailabilityChange::Suspended {
                            job: changed_job,
                            ..
                        } | deep_hearth::production::ProductionAvailabilityChange::Resumed {
                            job: changed_job,
                            ..
                        } if *changed_job == job
                    )
                }),
            "gameplay harness {context} job changed availability inside an uninterrupted completion helper"
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == job)
        {
            assert_eq!(
                elapsed, expected_ticks,
                "gameplay harness {context} completed before its resolved duration"
            );
            return;
        }
    }
    panic!(
        "gameplay harness {context} remained active after its resolved {expected_ticks}-tick duration"
    );
}
