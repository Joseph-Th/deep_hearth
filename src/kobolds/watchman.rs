//! A fixed second den resident undertakes short, food-funded physical watch patrols.
//! No population growth or offscreen troop spawning is inferred from player progress.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::feeding::consume_den_meal;
use super::patrol::{living_player_deters_scout, scout_can_enter};
use super::visibility::scout_visible_from;
use super::{
    FIRST_SCOUT_TICK, KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_STEP_TICKS,
    SCOUT_WARY_INTERVAL_TICKS, WATCH_DURATION_TICKS, WATCH_RECOVERY_TICKS,
};

impl KoboldColony {
    /// The watchman exists at the den from the beginning, independent of scout cargo.
    pub(super) fn watch_position(&self) -> VoxelCoord {
        if self.watch_deployed_at.is_some() {
            self.route[1]
        } else {
            self.route[0]
        }
    }

    /// Danger can be passed to a scout only while both residents physically
    /// occupy the same sheltered den, not while one remains afield.
    pub(super) fn deliver_watch_alarm_at_den(&mut self, at: SimulationTick) {
        if !self.watch_alarm_pending || self.watch_deployed_at.is_some() || self.scout_step != 0 {
            return;
        }
        let until = at
            .checked_add_span(TickSpan::new(SCOUT_WARY_INTERVAL_TICKS))
            .expect("bounded scout caution cannot overflow");
        self.wary_until = Some(self.wary_until.map_or(until, |old| old.max(until)));
        self.watch_alarm_pending = false;
    }

    pub(super) fn validate_watchman(&self, now: SimulationTick) -> Result<(), &'static str> {
        let ready = self.watch_ready_at.value();
        if ready < FIRST_SCOUT_TICK
            || (ready - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
            || (ready != FIRST_SCOUT_TICK
                && ready > now.value().saturating_add(WATCH_RECOVERY_TICKS))
            || (self.mobilized_at.is_none()
                && (ready != FIRST_SCOUT_TICK || self.watch_deployed_at.is_some()))
            || (self.watch_alarm_pending
                && (self.watch_ready_at.value() == FIRST_SCOUT_TICK
                    || (self.watch_deployed_at.is_none() && self.scout_step == 0)))
        {
            return Err("kobold watchman's food-backed patrol cooldown is invalid");
        }
        if let Some(deployed) = self.watch_deployed_at {
            if deployed > now
                || deployed.value() < FIRST_SCOUT_TICK
                || (deployed.value() - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
                || self.mobilized_at.is_none_or(|report| report >= deployed)
                || deployed.checked_add_span(TickSpan::new(WATCH_RECOVERY_TICKS))
                    != Some(self.watch_ready_at)
            {
                return Err("kobold watchman lacks a possible provisioned deployment");
            }
        }
        Ok(())
    }
}

/// Called only on the existing 5-slow-tick resident cadence, after the scout's
/// meal has had first claim on local food. A complete 20 g fresh den ration
/// provides one patrol; no ration means no watchman deployment.
pub(super) fn apply_watchman_tick(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return;
    };
    let den = colony.route[0];
    let junction = colony.route[1];
    let guard = colony.watch_position();
    let player = state.logistics().player().map(|record| record.position());
    let player_seen = colony.watch_deployed_at.is_some()
        && living_player_deters_scout(state, guard)
        && player.is_some_and(|position| scout_visible_from(registries, state, guard, position));
    let pressured = living_player_deters_scout(state, guard)
        || assess_local_illumination(registries, state, guard).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let destination = if colony.watch_deployed_at.is_some() {
        (pressured
            || colony.watch_deployed_at.is_some_and(|start| {
                at.value() >= start.value().saturating_add(WATCH_DURATION_TICKS)
            }))
        .then_some(den)
    } else {
        (colony.mobilized_at.is_some() && at >= colony.watch_ready_at && !pressured)
            .then_some(junction)
    };
    let scout_at_post = colony.scout_position() == junction;
    // A witness retains the alarm even if a blocked return prevents reporting it.
    if player_seen {
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .unwrap()
            .watch_alarm_pending = true;
    }
    let Some(destination) = destination else {
        return;
    };
    if !scout_can_enter(registries, state, destination, player)
        || (destination == junction
            && (scout_at_post || living_player_deters_scout(state, destination)))
        || assess_local_illumination(registries, state, destination).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL
    {
        return;
    }
    if destination == den {
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .unwrap()
            .watch_deployed_at = None;
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .unwrap()
            .deliver_watch_alarm_at_den(at);
    } else if consume_den_meal(registries, state, den, at, super::SCOUT_MEAL_MASS).is_some() {
        let watcher = state.kobold_state_mut().colony.as_mut().unwrap();
        watcher.watch_deployed_at = Some(at);
        watcher.watch_ready_at = at
            .checked_add_span(TickSpan::new(WATCH_RECOVERY_TICKS))
            .expect("bounded watch cooldown cannot overflow");
    }
}
