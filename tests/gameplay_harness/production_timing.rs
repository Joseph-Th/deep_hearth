//! Canonical completion stepping for production work expected to remain uninterrupted.

use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::production::ProductionJobId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

/// Advances one already-admitted production job whose providers are intentionally stable.
///
/// This is not a general actor scheduler. It verifies the runtime completion receipt and refuses to
/// hide a suspension/resume branch behind a precomputed duration.
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
                .any(|change| change.job() == job),
            "gameplay harness {context} job changed availability inside an uninterrupted completion helper"
        );
        assert!(
            outcome
                .production_completions()
                .iter()
                .all(|completion| completion.job() == job),
            "gameplay harness {context} crossed an unrelated production completion"
        );
        assert!(
            outcome.ready_mining_jobs().is_empty()
                && outcome.manual_power().is_none()
                && outcome.field_prospecting().is_none(),
            "gameplay harness {context} crossed unrelated observable player work"
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
