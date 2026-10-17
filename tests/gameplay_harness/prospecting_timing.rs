//! Canonical completion stepping for already-admitted prospecting work.

use deep_hearth::core::state::AppState;
use deep_hearth::geology::FieldProspectingOutcome;
use deep_hearth::labor::ProspectingWork;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

/// Completes one admitted prospecting action by following its authoritative work schedule.
pub(super) fn complete_prospecting_work(
    registries: &Registries,
    state: &mut AppState,
    work: ProspectingWork,
    context: &'static str,
) -> FieldProspectingOutcome {
    assert!(
        state.tick() >= work.started_at() && state.tick() < work.completes_at(),
        "gameplay harness {context} requires active prospecting before its admitted completion"
    );
    let mut completion = None;
    while state.tick() < work.completes_at() {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        if state.tick() < work.completes_at() {
            assert_eq!(
                outcome.field_prospecting(),
                None,
                "gameplay harness {context} observed prospecting before its admitted completion"
            );
        } else {
            completion = outcome.field_prospecting();
        }
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                field_prospecting: true,
                ..TickEventAllowance::default()
            },
            context,
        );
    }
    completion.unwrap_or_else(|| {
        panic!("gameplay harness {context} prospecting produced no completion outcome")
    })
}
