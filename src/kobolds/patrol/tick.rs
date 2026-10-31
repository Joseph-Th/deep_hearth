//! Scheduled scout work, custody transitions, and observable event settlement.

use super::*;

fn prioritize_food_cargo(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    from: VoxelCoord,
    den_space: Mass,
    forage_ready: bool,
    at: SimulationTick,
) -> bool {
    (super::super::hydration::prolonged_water_deprivation(colony, at)
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

/// An industrial raid needs both real capacity and a resident fit to work.
fn planned_ground_raid(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
    at: SimulationTick,
    forage_ready: bool,
    optional_work_blocked: bool,
) -> Option<(
    crate::inventory::StockpileId,
    ScavengedGroundStockpileRequest,
)> {
    let colony = state.kobold_state().colony.as_ref()?;
    let space = den_cache_space(state, colony.route[0]);
    if colony.mobilized_at.is_none()
        || colony.scout_step == 0
        || colony.returning
        || colony.scavenged_drop.is_some()
        || colony.hungry
        || optional_work_blocked
        || space.is_zero()
        || has_local_scout_interest(registries, state, from, space, forage_ready, at, false)
    {
        return None;
    }
    let (stockpile, lot) = local_unenclosed_stockpile_target(registries, state, from, at)?;
    Some((
        stockpile,
        ScavengedGroundStockpileRequest::new(
            stockpile,
            lot,
            from,
            at,
            colony.salvage_batch().min(space),
            SCOUT_SAFE_SCAVENGE_TEMPERATURE,
        ),
    ))
}

/// One canonical extraction occupies the scout's work slot and settles
/// finite stockpile custody, never material created from a target sighting.
fn execute_ground_raid(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    forage_ready: bool,
    blocked: bool,
) -> Option<(crate::inventory::StockpileId, Mass)> {
    let (stockpile, request) =
        planned_ground_raid(registries, state, from, at, forage_ready, blocked)?;
    let (_, stolen) = scavenge_unenclosed_ground_stockpile_material(registries, state, request)?;
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("ground raid retains its resident scout")
        .next_step_at = at
        .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
        .expect("bounded ground raid cannot overflow its scout clock");
    Some((stockpile, stolen))
}

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
    let drank_water = (!pressured)
        .then(|| super::super::hydration::drink_on_scout_visit(registries, state, from, next_tick))
        .flatten();
    // Meals use the existing sparse scout schedule, never a per-control-tick AI update.
    // Den resources remain physically recoverable until they are actually eaten.
    let (consumed_food, discarded_food, cleared_cache) =
        apply_scheduled_den_care(registries, state, from, next_tick, pressured);
    let water_deprived = super::super::hydration::prolonged_water_deprivation(
        state
            .kobold_state()
            .colony
            .as_ref()
            .expect("active scout exists"),
        next_tick,
    );
    let (heard_junk_alarm, alarm_pulses) = apply_watchman_tick(registries, state, next_tick);
    let pressured = pressured || heard_junk_alarm;
    let optional_work_blocked = pressured || water_deprived;
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
                position: from,
                discovered_player_drop: false,
                raided_stockpile: None,
                relocated_drop: None,
                fled_player: false,
                fled_light: false,
                turned_back_at_barrier: false,
                cancelled_empty_cache_raid: false,
                abandoned_drop: None,
                foraged_food: None,
                split_drop: None,
                consumed_food: None,
                drank_water: None,
                discarded_food: None,
                cleared_cache: None,
                heard_junk_alarm,
                defensive_contact: Some(contact),
            }),
            alarm_pulses,
        );
    }
    // Copper fitting occupies two real slow-clock work slots at the junction.
    // Player pressure, alarm warnings and absent parts interrupt unfinished work.
    if apply_pry_fitting_tick(state, next_tick, optional_work_blocked) {
        return (None, alarm_pulses);
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
        let visible = player_position
            .is_some_and(|player| scout_visible_from(registries, state, from, player));
        return (
            visible.then_some(KoboldScoutEvent {
                position: from,
                discovered_player_drop: false,
                raided_stockpile: Some((stockpile, stolen)),
                relocated_drop: None,
                fled_player: false,
                fled_light: false,
                turned_back_at_barrier: false,
                cancelled_empty_cache_raid: false,
                abandoned_drop: None,
                foraged_food: None,
                split_drop: None,
                consumed_food: None,
                drank_water: None,
                discarded_food: None,
                cleared_cache: None,
                heard_junk_alarm: false,
                defensive_contact: None,
            }),
            alarm_pulses,
        );
    }
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
    // A scout emerging from an occluded den must not disclose the identity
    // or mass of food eaten out of the observer's line of sight.
    let den_visible =
        player_position.is_some_and(|player| scout_visible_from(registries, state, from, player));
    let observed_meal = consumed_food.filter(|_| den_visible);
    let observed_water = drank_water.filter(|_| den_visible);
    let observed_discard = discarded_food.filter(|_| den_visible);
    let observed_clearing = cleared_cache.filter(|_| den_visible);
    let had_local_activity = [
        can_move,
        heard_junk_alarm,
        abandoned_drop.is_some(),
        observed_meal.is_some(),
        observed_water.is_some(),
        observed_discard.is_some(),
        observed_clearing.is_some(),
    ]
    .contains(&true);
    let visible = had_local_activity
        && player_position
            .is_some_and(|player| scout_visible_from(registries, state, scout_position, player));
    (
        visible.then_some(KoboldScoutEvent {
            position: scout_position,
            heard_junk_alarm,
            defensive_contact: None,
            discovered_player_drop: discovered,
            raided_stockpile: None,
            relocated_drop,
            fled_player: (pressured_by_player || pressured_by_noise || ahead_is_guarded)
                && can_move,
            fled_light: (pressured_by_light || (turned_back_at_barrier && ahead_is_lit))
                && can_move,
            turned_back_at_barrier: turned_back_at_barrier && can_move,
            cancelled_empty_cache_raid: cancelled_empty_cache_raid && can_move,
            abandoned_drop,
            foraged_food,
            split_drop,
            consumed_food: observed_meal,
            drank_water: observed_water,
            discarded_food: observed_discard,
            cleared_cache: observed_clearing,
        }),
        alarm_pulses,
    )
}
