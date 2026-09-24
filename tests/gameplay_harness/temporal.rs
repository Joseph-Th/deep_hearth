//! Actor-facing canonical tick helpers that make expected observable outcomes explicit.

use deep_hearth::core::state::AppState;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

/// Advances deliberate idle observation while failing closed on newly observable non-survival work.
pub(super) fn advance_idle_ticks(
    registries: &Registries,
    state: &mut AppState,
    ticks: u64,
    context: &'static str,
) {
    assert_eq!(
        state.player_work().active(),
        None,
        "gameplay harness {context} cannot use idle stepping while player work is active"
    );
    for _ in 0..ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert_tick_events_within(&outcome, TickEventAllowance::default(), context);
        assert_eq!(
            state.player_work().active(),
            None,
            "gameplay harness {context} unexpectedly acquired player work while observing"
        );
    }
}
