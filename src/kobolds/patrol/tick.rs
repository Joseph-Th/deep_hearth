//! Scheduled scout work, custody transitions, and observable event settlement.

use super::*;

mod care;
mod ground_raid;
mod movement;

use care::settle_scout_visit_care;
use ground_raid::execute_ground_raid;
use movement::{ScoutMovementFrame, settle_scout_movement};

/// Deterministic slow-tick patrol with finite, interruptible return-trip scavenging.
///
/// The scout waits one visit before collecting an exposed drop. On retreat it takes one
/// identity-preserving adjacent step with the parcel, then leaves it at the den where normal
/// player pickup can recover it. Blocking the route stops the scout and its cargo together.
pub(crate) fn apply_kobold_scout_tick(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
) -> (Option<KoboldScoutEvent>, Vec<KoboldAlarmPulse>) {
    super::super::temple_guard::apply_temple_warden_tick(registries, state, next_tick);
    super::super::temple_archivist::apply_temple_archivist_tick(registries, state, next_tick);
    if state
        .kobold_state()
        .colony
        .as_ref()
        .is_none_or(|colony| colony.next_step_at > next_tick)
    {
        return (None, Vec::new());
    }
    // Balance recovers only on the existing five-simulation-tick resident cadence.
    // This is not healing of injured tissue and transfers no matter.
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .recover_balance();
    let colony = state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap_or_else(|| unreachable!("the guarded scout tick requires a colony"));
    let from = colony.scout_position();
    observe_progress(registries, state, next_tick, from);
    let forage_ready = state
        .kobold_state()
        .colony
        .as_ref()
        .unwrap_or_else(|| unreachable!("the kobold colony survived den progress observation"))
        .next_forage_at
        .is_none_or(|due| next_tick >= due);
    let player_position = state.logistics().player().map(|player| player.position());
    let observer = super::super::living_player_observer(state);
    let pressured_by_noise =
        super::super::acoustics::scout_hears_running_player(registries, state, from, next_tick);
    let pressured_by_player = living_player_deters_scout(registries, state, from);
    // World-owned opaque blocks and live finite energy stores control this field.
    // Neither player-held torches nor hidden ambient/skylight are invented here.
    let pressured_by_light =
        assess_local_illumination(registries, state, from).level() >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let recently_hit = state.kobold_state().colony.as_ref().is_some_and(|colony| {
        colony.recently_shoved(super::super::KoboldRole::Scout, state.control_tick())
    });
    let pressured = pressured_by_player || pressured_by_noise || pressured_by_light || recently_hit;
    let care = settle_scout_visit_care(registries, state, from, next_tick, observer, pressured);
    let pressured = pressured || care.heard_junk_alarm;
    // Metal fitting and inventory raids occupy real grounded work slots at the
    // scout's current cell. A mined-out source floor cannot authorize those
    // actions just because ordinary patrol movement already stays put.
    let optional_work_blocked =
        pressured || care.water_deprived || !opening_resident_can_depart(registries, state, from);
    if let Some(contact) =
        super::super::defense::attempt_worksite_counterstroke(registries, state, next_tick)
    {
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .unwrap()
            .next_step_at = next_tick
            .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
            .expect("worksite defense has the same bounded scout cadence");
        return (
            Some(KoboldScoutEvent {
                heard_junk_alarm: care.heard_junk_alarm,
                defensive_contact: Some(contact),
                ..care.observed_care
            }),
            care.alarm_pulses,
        );
    }
    // Copper fitting occupies two real slow-clock work slots at the junction.
    // Player pressure, alarm warnings and absent parts interrupt unfinished work.
    if apply_pry_fitting_tick(state, next_tick, optional_work_blocked) {
        return (
            care.observed_care.has_care().then_some(care.observed_care),
            care.alarm_pulses,
        );
    }
    // A reported industrial encounter gives the scout a reason to inspect
    // exposed *ground storage*, not only loose drops. Extraction costs this
    // complete patrol work slot and leaves the parcel openly recoverable here.
    // The next step must move it by the same physical cargo route as any loot.
    // Existing loose cargo and food needs retain priority; fear, light, walls,
    // player presence and den capacity can prevent the raid altogether.
    if let Some((stockpile, stolen)) = execute_ground_raid(
        registries,
        state,
        from,
        next_tick,
        forage_ready,
        optional_work_blocked,
    ) {
        let visible = observer.is_some_and(|player| {
            crate::world::player_can_see_from_eye_at(registries, state, player, from, next_tick)
        });
        return (
            visible.then_some(KoboldScoutEvent {
                raided_stockpile: Some((stockpile, stolen)),
                heard_junk_alarm: care.heard_junk_alarm,
                ..care.observed_care
            }),
            care.alarm_pulses,
        );
    }
    settle_scout_movement(
        registries,
        state,
        ScoutMovementFrame {
            from,
            at: next_tick,
            forage_ready,
            player_position,
            observer,
            pressured,
            pressured_by_player,
            pressured_by_noise,
            pressured_by_light,
        },
        care,
    )
}
