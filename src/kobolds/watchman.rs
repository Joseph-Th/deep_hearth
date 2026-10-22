//! A fixed second den resident undertakes short, food-funded physical watch patrols.
//! No population growth or offscreen troop spawning is inferred from player progress.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

use super::feeding::consume_den_meal;
use super::junkworks::relay_watch_alarm;
use super::patrol::{living_player_deters_scout, scout_can_enter, scout_sees_guarded_cell};
use super::tracks::KoboldTrailKind;
use super::visibility::scout_visible_from;
use super::{
    FIRST_SCOUT_TICK, KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_STEP_TICKS,
    SCOUT_WARY_INTERVAL_TICKS, WATCH_DURATION_TICKS, WATCH_RECOVERY_TICKS,
};

impl KoboldColony {
    /// The watchman exists at the den from the beginning, independent of scout cargo.
    pub(super) fn watch_position(&self) -> VoxelCoord {
        if self.watch_deployed_at.is_none() {
            self.route[0]
        } else if self.watch_leg == 1 {
            self.route[2]
        } else {
            self.route[1]
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
        // The colony cannot order an outer picket before it has both an
        // industrial report and a real prior provisioned watch. An ordinary
        // den resident can still warn the scout without conjuring a sortie.
        if self.mobilized_at.is_some() && self.watch_ready_at.value() > FIRST_SCOUT_TICK {
            self.watch_counterpatrol_pending = true;
        }
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
                && self.watch_deployed_at.is_none()
                && self.scout_step == 0)
            || self.watch_leg > 2
            || (self.watch_deployed_at.is_none() && self.watch_leg != 0)
            || (self.watch_counterpatrol_pending
                && (self.mobilized_at.is_none()
                    || ready == FIRST_SCOUT_TICK
                    || self.watch_leg > 0
                    || self.watch_deployed_at.is_some_and(|start| {
                        self.mobilized_at.is_some_and(|reported| {
                            start.value() <= reported.value().saturating_add(SCOUT_STEP_TICKS)
                        })
                    })))
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
                || (self.watch_leg == 1
                    && now.value() < deployed.value().saturating_add(SCOUT_STEP_TICKS))
                || (self.watch_leg == 2
                    && now.value() < deployed.value().saturating_add(2 * SCOUT_STEP_TICKS))
            {
                return Err("kobold watchman lacks a possible provisioned deployment");
            }
        }
        Ok(())
    }
}

/// A reported player intrusion can fund one wider, two-edge picket on the
/// next rationed sortie. Every leg follows the same authored traversal graph.
/// The guard never abandons its post by teleporting through an obstructed leg.
fn plan_watch_step(
    colony: &KoboldColony,
    at: SimulationTick,
    pressured: bool,
) -> Option<(VoxelCoord, u8)> {
    let [den, junction, tip] = colony.route;
    match colony.watch_deployed_at {
        None if colony.mobilized_at.is_some()
            && at >= colony.watch_ready_at
            && !colony.hungry
            && !pressured =>
        {
            Some((junction, 0))
        }
        Some(start) => match colony.watch_leg {
            0 if pressured || at.value() >= start.value().saturating_add(WATCH_DURATION_TICKS) => {
                Some((den, 0))
            }
            0 if colony.watch_counterpatrol_pending && at > start => Some((tip, 1)),
            1 if pressured
                || at.value()
                    >= start
                        .value()
                        .saturating_add(WATCH_DURATION_TICKS - SCOUT_STEP_TICKS) =>
            {
                Some((junction, 2))
            }
            2 => Some((den, 0)),
            _ => None,
        },
        _ => None,
    }
}

/// Called only on the existing 5-slow-tick resident cadence, after the scout's
/// meal has had first claim on local food. A complete 20 g fresh den ration
/// provides one patrol; no ration means no watchman deployment.
pub(super) fn apply_watchman_tick(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    let den = colony.route[0];
    let guard = colony.watch_position();
    let player = state.logistics().player().map(|record| record.position());
    let player_nearby = living_player_deters_scout(registries, state, guard);
    // The permanent resident sees a living person at the edge of visual range,
    // at home or on patrol. Knowledge reaches the scout only when the pair
    // physically meets at the den, not across an offscreen telepathic link.
    let player_seen = state
        .survival()
        .player()
        .is_some_and(|record| record.vitality() != Vitality::ZERO)
        && player.is_some_and(|position| scout_visible_from(registries, state, guard, position));
    let pressured = player_nearby
        || assess_local_illumination(registries, state, guard).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL;
    // A resident with an intruder in direct sight protects the den instead
    // of starting a fresh food-funded excursion. An already deployed sentry
    // uses the ordinary close-range deterrence and physical return rules.
    let holding_den = colony.watch_deployed_at.is_none() && player_seen;
    let next = plan_watch_step(colony, at, pressured || holding_den);
    let scout_position = colony.scout_position();
    // A witness retains the alarm even if a blocked return prevents reporting it.
    if player_seen {
        let witness = state.kobold_state_mut().colony.as_mut().unwrap();
        witness.watch_alarm_pending = true;
        // At home the guard may tell a co-located scout immediately. When the
        // scout is out, the report remains pending until its physical return.
        if witness.watch_deployed_at.is_none() {
            witness.deliver_watch_alarm_at_den(at);
        }
    }
    let signaled = relay_watch_alarm(state, at);
    let Some((destination, next_leg)) = next else {
        return signaled;
    };
    if !scout_can_enter(registries, state, destination, player)
        || (destination != den && scout_position == destination)
        // Coming home is a real physical step too. A player watching the den
        // entrance can hold a calm sentry at its post, delaying both the
        // report and its next sortie. Immediate flight from danger already
        // at the sentry's position still takes precedence.
        || (!pressured && scout_sees_guarded_cell(registries, state, guard, destination))
        // Anticipating a bright post is useful while calm, but it must not
        // trap an already frightened sentry on a lit position. Its emergency
        // return still crosses one real edge; impassable and player-occupied
        // destinations remain blocked above.
        || (!pressured
            && assess_local_illumination(registries, state, destination).level()
                >= SCOUT_DETERRENT_LIGHT_LEVEL)
    {
        return signaled;
    }
    let moved = if destination == den {
        let watcher = state.kobold_state_mut().colony.as_mut().unwrap();
        watcher.watch_deployed_at = None;
        watcher.watch_leg = 0;
        watcher.deliver_watch_alarm_at_den(at);
        true
    } else if state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap()
        .watch_deployed_at
        .is_some()
    {
        let watcher = state.kobold_state_mut().colony.as_mut().unwrap();
        watcher.watch_leg = next_leg;
        if next_leg == 1 {
            watcher.watch_counterpatrol_pending = false;
        }
        true
    } else if consume_den_meal(
        registries,
        state,
        den,
        at,
        super::SCOUT_MEAL_MASS,
        super::SCOUT_MEAL_MASS,
    )
    .is_some()
    {
        let watcher = state.kobold_state_mut().colony.as_mut().unwrap();
        watcher.watch_deployed_at = Some(at);
        watcher.watch_leg = 0;
        watcher.watch_ready_at = at
            .checked_add_span(TickSpan::new(WATCH_RECOVERY_TICKS))
            .expect("bounded watch cooldown cannot overflow");
        true
    } else {
        false
    };
    if moved {
        let activity = if pressured {
            KoboldTrailKind::Fleeing
        } else {
            KoboldTrailKind::Walking
        };
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .unwrap()
            .record_watch_track(guard, destination, at, activity);
    }
    signaled
}
