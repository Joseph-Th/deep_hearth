//! Canonical actor-facing stepping for already-admitted eating and drinking work.

use deep_hearth::core::state::AppState;
use deep_hearth::core::time::SimulationTick;
use deep_hearth::labor::PlayerWork;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

pub(super) fn finish_direct_consumption_work(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
    context: &'static str,
) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("gameplay harness {context} has no active direct consumption"));
    assert!(
        matches!(
            active,
            PlayerWork::Eating { .. } | PlayerWork::Drinking { .. }
        ),
        "gameplay harness {context} expected eating or drinking work, found {active:?}"
    );
    let ticks = completes_at
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("gameplay harness {context} completion precedes current tick"));
    assert!(
        ticks > 0,
        "gameplay harness {context} direct consumption must occupy time"
    );
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert_tick_events_within(&outcome, TickEventAllowance::default(), context);
        if elapsed < ticks {
            assert_eq!(
                state.player_work().active(),
                Some(active),
                "gameplay harness {context} lost direct-consumption attention before completion"
            );
        } else {
            assert_eq!(
                state.player_work().active(),
                None,
                "gameplay harness {context} retained direct-consumption work after completion"
            );
        }
    }
    ticks
}
