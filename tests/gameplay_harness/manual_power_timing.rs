//! Canonical actor-facing stepping for direct manual-power work.

use deep_hearth::core::state::AppState;
use deep_hearth::labor::{ManualPowerWork, PlayerWork};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;

/// Advances one manual-power action to a tick no later than its validated completion.
/// Returns whether the action completed at that target.
pub(super) fn advance_manual_power_to(
    registries: &Registries,
    state: &mut AppState,
    work: ManualPowerWork,
    target_tick: u64,
    context: &'static str,
) -> bool {
    assert!(
        target_tick > state.tick().value() && target_tick <= work.completes_at().value(),
        "gameplay harness {context} manual-power target must be after now and no later than completion"
    );
    while state.tick().value() < target_tick {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("gameplay harness {context} tick failed: {error}"));
        assert!(
            outcome.production_availability_changes().is_empty()
                && outcome.production_completions().is_empty()
                && outcome.ready_mining_jobs().is_empty()
                && outcome.field_prospecting().is_none(),
            "gameplay harness {context} crossed unrelated observable work during manual power"
        );
        if outcome.tick() < work.completes_at() {
            assert_eq!(
                outcome.manual_power(),
                None,
                "gameplay harness {context} reported manual power before its validated completion"
            );
            assert_eq!(
                state.player_work().active(),
                Some(PlayerWork::ManualPower { work }),
                "gameplay harness {context} lost manual-power attention before completion"
            );
            continue;
        }
        let completed = outcome.manual_power().unwrap_or_else(|| {
            panic!("gameplay harness {context} produced no manual-power receipt")
        });
        assert_eq!(completed.method(), work.method());
        assert_eq!(completed.equipment(), work.equipment());
        assert_eq!(completed.destination(), work.destination());
        assert_eq!(completed.energy(), work.output().energy());
        assert_eq!(state.player_work().active(), None);
    }
    state.tick() == work.completes_at()
}

pub(super) fn finish_manual_power_work(
    registries: &Registries,
    state: &mut AppState,
    work: ManualPowerWork,
    context: &'static str,
) -> u64 {
    let ticks = work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("gameplay harness {context} completion precedes current time"));
    assert!(ticks > 0, "gameplay harness {context} must occupy time");
    assert!(advance_manual_power_to(
        registries,
        state,
        work,
        work.completes_at().value(),
        context,
    ));
    ticks
}
