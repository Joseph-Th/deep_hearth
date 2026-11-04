//! One scheduled scout movement, conserved return cargo, and local event reporting.

use super::*;
use crate::kobolds::visibility::player_sees_opening_resident_at;

/// Ephemeral facts settled by the scout's due-work and care coordinator.
/// Movement may use only this local frame and the current authoritative owners.
pub(super) struct ScoutMovementFrame {
    pub(super) from: VoxelCoord,
    pub(super) at: SimulationTick,
    pub(super) forage_ready: bool,
    pub(super) player_position: Option<VoxelCoord>,
    pub(super) observer: Option<VoxelCoord>,
    pub(super) pressured: bool,
    pub(super) pressured_by_player: bool,
    pub(super) pressured_by_noise: bool,
    pub(super) pressured_by_light: bool,
}

fn prioritize_food_cargo(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    from: VoxelCoord,
    den_space: Mass,
    forage_ready: bool,
    at: SimulationTick,
) -> bool {
    (crate::kobolds::hydration::prolonged_water_deprivation(colony, at)
        && colony.scavenged_drop.is_none())
        || should_seek_local_food(
            registries,
            state,
            from,
            den_space,
            forage_ready,
            at,
            colony.hungry,
        )
}

/// Complete one real patrol edge and its surface custody after due care and
/// stationary work have had priority. The emitted event respects actual sight.
pub(super) fn settle_scout_movement(
    registries: &Registries,
    state: &mut AppState,
    frame: ScoutMovementFrame,
    care: care::ScoutVisitCare,
) -> (Option<KoboldScoutEvent>, Vec<KoboldAlarmPulse>) {
    let ScoutMovementFrame {
        from,
        at: next_tick,
        forage_ready,
        player_position,
        observer,
        pressured,
        pressured_by_player,
        pressured_by_noise,
        pressured_by_light,
    } = frame;
    let care::ScoutVisitCare {
        drank_water,
        discarded_food,
        cleared_cache,
        observed_care,
        heard_junk_alarm,
        alarm_pulses,
        water_deprived: _,
    } = care;
    let colony = state
        .kobold_state()
        .colony
        .as_ref()
        .expect("colony still exists");
    let salvage_batch = colony.salvage_batch();
    let forage_batch = colony.forage_batch_in_world(registries, state, from, next_tick);
    let forage_interval = colony.forage_interval();
    let mobilized = colony.mobilized_at.is_some();
    let den_space = den_cache_space(state, colony.route[0]);
    let decision = ScoutPatrolContext {
        from,
        den_space,
        forage_ready,
        at: next_tick,
        player_position,
        pressured,
    };
    let raid_interest = scout_raid_interest(registries, state, colony, decision);
    let move_plan = plan_scout_move(registries, state, colony, decision, raid_interest);
    let ScoutMovePlan {
        step,
        returning,
        detour,
        position,
        ahead_is_lit,
        returning_into_light,
        ahead_is_guarded,
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear,
    } = move_plan;
    let frightened = pressured || ahead_is_guarded || returning_into_light;
    let may_forage_on_return = colony.can_forage_on_return(step);
    // Hunger prioritizes food when the scout can currently gather or move it.
    // Without a locally edible source, inert scavenging remains a real fallback.
    let food_only = prioritize_food_cargo(
        registries,
        state,
        colony,
        from,
        den_space,
        forage_ready,
        next_tick,
    );
    let cargo_choice = CargoDecision {
        from,
        to: position,
        returning,
        frightened,
        den_space,
        food_only,
        mobilized,
        at: next_tick,
    };
    let (carried_drop, cargo_blocked_at_den, dragged_candidate) =
        plan_returning_cargo(registries, state, colony, cargo_choice);
    let ScoutReturnCargoOutcome {
        can_move,
        discovered,
        relocated_drop,
        split_drop,
        foraged_food,
        staged_tool_part,
        abandoned_drop,
    } = execute_scout_return_cargo(
        registries,
        state,
        ScoutReturnCargoRequest {
            choice: cargo_choice,
            physically_clear,
            carried_drop,
            cargo_blocked_at_den,
            dragged_candidate,
            may_forage_on_return,
            forage_ready,
            forage_batch,
            salvage_batch,
            ignored_external: [cleared_cache, discarded_food],
        },
    );
    let next_step_at = next_tick
        .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
        .expect("bounded control-aligned simulation cannot overflow a scout schedule");

    let colony = state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap_or_else(|| unreachable!("scout decision requires a colony"));
    if can_move {
        colony.apply_step(
            step,
            returning,
            detour,
            relocated_drop
                .filter(|_| !staged_tool_part)
                .or(split_drop.map(|(id, _)| id))
                .or_else(|| (step > 0).then(|| foraged_food.map(|(id, _)| id)).flatten()),
        );
        let activity = if drank_water.is_some()
            && !frightened
            && relocated_drop.is_none()
            && split_drop.is_none()
            && foraged_food.is_none()
        {
            KoboldTrailKind::Drinking
        } else {
            KoboldTrailKind::from_step(
                frightened,
                turned_back_at_barrier,
                relocated_drop.is_some() || cleared_cache.is_some() || discarded_food.is_some(),
                split_drop.is_some(),
                foraged_food.is_some(),
            )
        };
        colony.record_track(from, position, next_tick, activity);
        colony.report_progress_at_home(next_tick);
        colony.deliver_watch_alarm_at_den(next_tick);
    }
    if frightened {
        // A trapped scout also relinquishes the parcel, enabling recovery without
        // inventing inaccessible enemy inventory or a damage/attack verb.
        colony.scavenged_drop = None;
    }
    colony.next_step_at = next_step_at;
    update_scout_caution(
        colony,
        next_tick,
        scout_has_cause_for_caution(frightened, turned_back_at_barrier, ahead_is_lit, can_move),
    );
    if foraged_food.is_some() {
        colony.next_forage_at = Some(
            next_tick
                .checked_add_span(TickSpan::new(forage_interval))
                .expect("bounded simulation cannot overflow the next forage opportunity"),
        );
    }
    colony.noticed_player_drop |= discovered || relocated_drop.is_some() || split_drop.is_some();
    let scout_position = colony.scout_position();
    // Only a visible destination authorizes reporting the scout's movement.
    // Source-visible care remains reportable even if the scout departs unseen.
    let had_local_activity = [
        can_move,
        heard_junk_alarm,
        abandoned_drop.is_some(),
        observed_care.has_care(),
    ]
    .contains(&true);
    let visible = had_local_activity
        && observer.is_some_and(|player| {
            player_sees_opening_resident_at(registries, state, player, scout_position, next_tick)
        });
    // An unobstructed head is enough to witness the walking resident,
    // but the identity and mass of parcels on the ground still require
    // their actual ground cell to be visible.
    let destination_ground_visible = observer.is_some_and(|player| {
        crate::world::player_can_see_local_cell_at(
            registries,
            state,
            player,
            scout_position,
            next_tick,
        )
    });
    // A parcel released at the previous patrol cell remains there. Seeing
    // the scout at its destination cannot reveal matter outside the player's
    // actual sight envelope.
    let observed_abandoned_drop = abandoned_drop.filter(|_| {
        observer.is_some_and(|player| {
            crate::world::player_can_see_local_cell_at(registries, state, player, from, next_tick)
        })
    });
    let reported = visible
        .then_some(KoboldScoutEvent {
            position: scout_position,
            heard_junk_alarm,
            defensive_contact: None,
            discovered_player_drop: discovered && destination_ground_visible,
            raided_stockpile: None,
            relocated_drop: relocated_drop.filter(|_| destination_ground_visible),
            fled_player: (pressured_by_player || pressured_by_noise || ahead_is_guarded)
                && can_move,
            fled_light: (pressured_by_light || (turned_back_at_barrier && ahead_is_lit))
                && can_move,
            turned_back_at_barrier: turned_back_at_barrier && can_move,
            cancelled_empty_cache_raid: cancelled_empty_cache_raid && can_move,
            abandoned_drop: observed_abandoned_drop,
            foraged_food: foraged_food.filter(|_| destination_ground_visible),
            split_drop: split_drop.filter(|_| destination_ground_visible),
            ..observed_care
        })
        // Source-visible care and abandoned cargo remain observable even if
        // the scout departs unseen. Report only facts from that source cell,
        // never the new position, retreat decision, or hidden cargo route.
        .or_else(|| {
            (observed_care.has_care() || observed_abandoned_drop.is_some()).then_some(
                KoboldScoutEvent {
                    abandoned_drop: observed_abandoned_drop,
                    ..observed_care
                },
            )
        });
    (reported, alarm_pulses)
}
