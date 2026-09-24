//! Canonical completion stepping for production work expected to remain uninterrupted.

use deep_hearth::core::state::AppState;
use deep_hearth::production::ProductionJobId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

/// Advances one already-admitted production job whose providers are intentionally stable.
///
/// This is not a general actor scheduler. It verifies the runtime completion receipt and refuses to
/// hide a suspension/resume branch behind a precomputed duration.
pub(super) fn finish_uninterrupted_production_job(
    registries: &Registries,
    state: &mut AppState,
    job: ProductionJobId,
    context: &'static str,
) {
    let admitted = state.production().get_job(job).unwrap_or_else(|| {
        panic!("gameplay harness {context} admitted production job disappeared")
    });
    assert!(
        !admitted.is_suspended(),
        "gameplay harness {context} cannot use uninterrupted stepping for a suspended job"
    );
    let expected_ticks = admitted
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("gameplay harness {context} completion precedes current time"));
    assert!(
        expected_ticks > 0,
        "gameplay harness {context} admitted a zero-remaining-tick production job"
    );
    for elapsed in 1..=expected_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[job],
                ..TickEventAllowance::default()
            },
            context,
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == job)
        {
            assert_eq!(
                elapsed, expected_ticks,
                "gameplay harness {context} completed before its admitted schedule"
            );
            return;
        }
    }
    panic!(
        "gameplay harness {context} remained active after its admitted {expected_ticks}-tick schedule"
    );
}
