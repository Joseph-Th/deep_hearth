//! Scout patrol planning and conserved physical scavenging on the slow tick.

mod raid;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceLifecycle, forage_food_to_adjacent_den,
    relocate_scavenged_drop_step, split_bulk_scavenged_drop_step,
    split_bulk_scavenged_drop_with_head_wear,
};
use crate::survival::Vitality;

use super::feeding::apply_scheduled_den_care;
use super::junkworks::KoboldAlarmPulse;
use super::mobilization::observe_progress;
use super::scavenging::{
    best_local_forage, den_cache_space, finds_external_drop, is_portable_drop,
    local_portable_source, local_split_source, scout_goal_has_supplies, scout_wants_loose_drop,
    should_seek_local_food,
};
use super::scrap_tools::{
    LEVER_SPLIT_BATCH, apply_pry_fitting_tick, current_junction_scrap_lever,
    is_staged_scrap_tool_part,
};
use super::visibility::scout_visible_from;
use super::watchman::apply_watchman_tick;
use super::{
    KoboldColony, KoboldScoutEvent, KoboldTrailKind, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_STEP_TICKS,
    SCOUT_WARY_INTERVAL_TICKS,
};

use raid::{choose_outbound_detour, scout_raid_interest};

pub(super) fn scout_can_enter(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    if player_position == Some(position) {
        return false;
    }
    if state.world().generation_profile().is_none() {
        return true;
    }
    let Some(head_y) = position.y().checked_add(1) else {
        return false;
    };
    state
        .world()
        .has_standing_clearance(registries.world(), position)
        && [
            position,
            VoxelCoord::new(position.x(), head_y, position.z()),
        ]
        .into_iter()
        .all(|cell| {
            !super::temple_caste_occupies_voxel(registries, state, cell)
                && !crate::logistics::player_body_occupies_voxel(registries, state, cell)
                // A physical enclosure obstructs passage. Co-located loose
                // equipment and energy/fluid stores are locality markers,
                // not full-cube terrain; illumination owns fire deterrence.
                && !state.logistics().stockpile_locations().any(|(id, at)| {
                    at == cell
                        && state
                            .inventory()
                            .get_stockpile(id)
                            .is_some_and(|stockpile| stockpile.enclosure().is_some())
                })
                && !state.structures().blocks_full_cell(cell)
        })
}

pub(super) fn living_player_deters_scout(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
) -> bool {
    let Some(position) = state.logistics().player().map(|player| player.position()) else {
        return false;
    };
    // A diagonal neighbor is physically close enough to guard the same small clearing.
    // A wall across the corner is not: fear requires a local line of sight, not merely a
    // nearby player coordinate. Two-cell axial sightings remain outside deterrence range.
    from.y() == position.y()
        && from.x().abs_diff(position.x()) <= 1
        && from.z().abs_diff(position.z()) <= 1
        && state
            .survival()
            .player()
            .is_some_and(|player| player.vitality() != Vitality::ZERO)
        && super::visibility::scout_visible_from(registries, state, from, position)
}

/// The current scout position, not the future destination, supplies the
/// sightline for preemptive guard avoidance. Returns false behind opaque
/// terrain or when the observer is out of local sight range.
pub(super) fn scout_sees_guarded_cell(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
    destination: VoxelCoord,
) -> bool {
    living_player_deters_scout(registries, state, destination)
        && state
            .logistics()
            .player()
            .is_some_and(|player| scout_visible_from(registries, state, from, player.position()))
}

#[derive(Clone, Copy)]
struct ScoutMovePlan {
    step: u8,
    returning: bool,
    detour: Option<(VoxelCoord, Option<VoxelCoord>)>,
    position: VoxelCoord,
    ahead_is_lit: bool,
    returning_into_light: bool,
    ahead_is_guarded: bool,
    turned_back_at_barrier: bool,
    cancelled_empty_cache_raid: bool,
    physically_clear: bool,
}

/// Movement and retreat planning does not mutate the authoritative colony.
/// An obstructed outbound scout returns by real edges; at the den it waits
/// for a cleared route rather than inventing a bypass.
fn plan_scout_move(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    next_tick: SimulationTick,
    pressured: bool,
    raid_interest: (bool, Option<(VoxelCoord, Option<VoxelCoord>)>),
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
) -> ScoutMovePlan {
    let (exposed_at_current, inherited_target) = raid_interest;
    let wary_at_den = colony.scout_step == 0
        && !pressured
        && colony.wary_until.is_some_and(|until| next_tick < until);
    let (proposed_step, proposed_returning) = if wary_at_den {
        (0, false)
    } else {
        colony.next_scout_step(pressured, exposed_at_current)
    };
    let proposed_detour = choose_outbound_detour(
        registries,
        state,
        colony,
        proposed_step,
        proposed_returning,
        player_position,
        den_space,
        forage_ready,
        next_tick,
        inherited_target,
    );
    let proposed_position = proposed_detour.map_or_else(
        || colony.position_at_step(proposed_step),
        |(first, _)| first,
    );
    let ahead_is_lit = assess_local_illumination(registries, state, proposed_position).level()
        >= SCOUT_DETERRENT_LIGHT_LEVEL
        && (!proposed_returning
            || scout_visible_from(
                registries,
                state,
                colony.scout_position(),
                proposed_position,
            ));
    // A scout can see a guarded destination from the previous route cell.
    // Approaching that clearing should prompt caution, not a free step into
    // the player's reach. This is checked only on the resident's slow clock.
    let ahead_is_guarded = proposed_step != colony.scout_step
        && scout_sees_guarded_cell(
            registries,
            state,
            colony.scout_position(),
            proposed_position,
        );
    let ahead_is_blocked = !scout_can_enter(registries, state, proposed_position, player_position)
        || (colony.watch_deployed_at.is_some() && proposed_position == colony.watch_position());
    // A detour chosen at the junction is not a reservation. Player pickup or
    // another canonical surface transaction can remove or alter the target
    // during the scout's intervening physical step. Recheck its neighboring
    // goal rather than walking into an empty camp by stale distant knowledge.
    let cancelled_empty_cache_raid = proposed_step == 3
        && !proposed_returning
        && !scout_goal_has_supplies(
            registries,
            state,
            proposed_position,
            den_space,
            forage_ready,
            next_tick,
        );
    let turned_back_at_barrier = !proposed_returning
        && proposed_step > colony.scout_step
        && colony.scout_step > 0
        && (ahead_is_lit || ahead_is_blocked || ahead_is_guarded);
    let (step, returning, detour, position) =
        if turned_back_at_barrier || cancelled_empty_cache_raid {
            let retreat_step = colony.scout_step - 1;
            (
                retreat_step,
                true,
                None,
                colony.position_at_step(retreat_step),
            )
        } else {
            (
                proposed_step,
                proposed_returning,
                proposed_detour,
                proposed_position,
            )
        };
    let returning_into_light =
        returning && !turned_back_at_barrier && !cancelled_empty_cache_raid && ahead_is_lit;
    ScoutMovePlan {
        step,
        returning,
        detour,
        position,
        ahead_is_lit,
        returning_into_light,
        ahead_is_guarded,
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear: step != colony.scout_step
            // A calm returning scavenger must not carry food or scrap into a
            // newly bright approach. A scout already in danger may still flee
            // along its real edge instead of freezing beside the light.
            && (pressured || !returning_into_light)
            && (returning || !ahead_is_lit)
            // Immediate flight already has priority over hypothetical next-cell
            // deterrence. Only an unpressured scout can plan to hold short of a
            // guarded homeward edge; a panicked scout still uses its real exit.
            && (pressured
                || !scout_sees_guarded_cell(registries, state, colony.scout_position(), position))
            && scout_can_enter(registries, state, position, player_position)
            && !(colony.watch_deployed_at.is_some() && position == colony.watch_position()),
    }
}

fn scout_has_cause_for_caution(
    frightened: bool,
    barrier: bool,
    lit_ahead: bool,
    moved: bool,
) -> bool {
    frightened || (barrier && lit_ahead && moved)
}

fn update_scout_caution(colony: &mut KoboldColony, at: SimulationTick, caution: bool) {
    if caution {
        colony.wary_until = Some(
            at.checked_add_span(TickSpan::new(SCOUT_WARY_INTERVAL_TICKS))
                .expect("bounded scout caution deadline cannot overflow"),
        );
    }
}

#[derive(Clone, Copy)]
struct CargoDecision {
    from: VoxelCoord,
    to: VoxelCoord,
    returning: bool,
    frightened: bool,
    den_space: Mass,
    food_only: bool,
    mobilized: bool,
    at: SimulationTick,
}

/// Existing carried matter must be checked for identity and location before
/// selecting new loot. Capacity limits the last den edge, not intermediate
/// carriage: a newly filled den cannot cause a parcel to drop miles away.
fn plan_returning_cargo(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    choice: CargoDecision,
) -> (Option<SurfaceResourceId>, bool, Option<SurfaceResourceId>) {
    let carried = colony.scavenged_drop.filter(|id| {
        state.surface().get(*id).is_some_and(|resource| {
            resource.position() == choice.from
                && resource.lifecycle() == SurfaceResourceLifecycle::Available
                && is_portable_drop(resource)
                && scout_wants_loose_drop(registries, state, resource, choice.at)
        })
    });
    let blocked_at_den = choice.to == colony.route[0]
        && carried.is_some_and(|id| {
            state
                .surface()
                .get(id)
                .is_some_and(|resource| resource.remaining_mass() > choice.den_space)
        });
    let switching_from_inert = choice.food_only
        && carried.is_some_and(|id| {
            state
                .surface()
                .get(id)
                .is_some_and(|resource| resource.commodity().form() != crate::content::FORM_FOOD)
        });
    let candidate = if !choice.returning || choice.frightened {
        None
    } else if switching_from_inert {
        // The scout may leave an inert parcel here to take fresh food at its
        // actual position. A loose portable serving goes first; otherwise
        // the return operation can split a local food pile or forage a patch.
        // If no food transfer succeeds, the old parcel is hauled as before.
        local_portable_source(
            registries,
            state,
            choice.from,
            choice.den_space,
            choice.at,
            true,
            choice.mobilized,
        )
    } else if let Some(id) = carried {
        (!blocked_at_den).then_some(id)
    } else {
        local_portable_source(
            registries,
            state,
            choice.from,
            choice.den_space,
            choice.at,
            choice.food_only,
            choice.mobilized,
        )
    };
    (carried, blocked_at_den, candidate)
}

fn scout_releases_cargo(frightened: bool, den_full: bool, can_move: bool) -> bool {
    frightened || (den_full && can_move)
}

/// The same canonical splitting operation must account for a tool's actual
/// abrasion, or the scout uses its original bare-hand batch instead.
fn split_scavenged_bulk_with_available_tool(
    registries: &Registries,
    state: &mut AppState,
    source: SurfaceResourceId,
    from: VoxelCoord,
    to: VoxelCoord,
    at: SimulationTick,
    bare_batch: Mass,
    den_space: Mass,
) -> Option<(SurfaceResourceId, Mass)> {
    let tool = state.kobold_state().colony.as_ref().and_then(|colony| {
        (from == colony.route[1])
            .then(|| current_junction_scrap_lever(state, colony))
            .flatten()
    });
    if let Some(tool) = tool {
        split_bulk_scavenged_drop_with_head_wear(
            registries,
            state,
            source,
            from,
            to,
            at,
            bare_batch.max(LEVER_SPLIT_BATCH).min(den_space),
            tool.head(),
            tool.kind().abrasion_per_split(),
            tool.kind().abrasion_form(),
        )
    } else {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source,
            from,
            to,
            at,
            bare_batch.min(den_space),
        )
    }
}

fn relocated_part_stays_at_junction(
    state: &AppState,
    relocated: Option<SurfaceResourceId>,
) -> bool {
    relocated
        .and_then(|id| state.surface().get(id))
        .is_some_and(|resource| {
            is_staged_scrap_tool_part(state, resource)
                || super::junkworks::is_staged_relay_component(state, resource)
        })
}

/// A returning scout may not leave a still-held parcel behind merely because
/// the surface owner has no revisions left for its actual physical transfer.
/// A food exchange also needs a reserved revision for the old-cargo fallback.
fn scout_cargo_movement_allowed(
    state: &AppState,
    physically_clear: bool,
    returning: bool,
    frightened: bool,
    blocked_at_den: bool,
    carried: Option<SurfaceResourceId>,
    candidate: Option<SurfaceResourceId>,
) -> (bool, bool) {
    let keep_carried = carried.is_some() && returning && !frightened && !blocked_at_den;
    let needs_revision = candidate.is_some() || keep_carried;
    (
        physically_clear && (!needs_revision || state.can_spend_surface_revisions(1)),
        keep_carried,
    )
}

/// Finish a locally attempted food exchange without abandoning the original
/// parcel if the new transfer failed. Both parcels remain surface-owned; the
/// boolean only records whether the scout truly switched its carried identity.
fn finish_return_cargo_exchange(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    to: VoxelCoord,
    carried: Option<SurfaceResourceId>,
    moved: Option<SurfaceResourceId>,
    can_keep_carried: bool,
    new_parcel: Option<SurfaceResourceId>,
) -> (Option<SurfaceResourceId>, bool) {
    let relocated = moved.or_else(|| {
        (can_keep_carried && new_parcel.is_none())
            .then(|| {
                carried.filter(|id| relocate_scavenged_drop_step(registries, state, *id, from, to))
            })
            .flatten()
    });
    let switched =
        carried.is_some_and(|old| relocated.is_some_and(|new| new != old) || new_parcel.is_some());
    (relocated, switched)
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
    super::temple_guard::apply_temple_warden_tick(registries, state, next_tick);
    super::temple_archivist::apply_temple_archivist_tick(registries, state, next_tick);
    if state
        .kobold_state()
        .colony
        .as_ref()
        .is_none_or(|colony| colony.next_step_at > next_tick)
    {
        return (None, Vec::new());
    }
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
    let pressured_by_player = living_player_deters_scout(registries, state, from);
    // World-owned opaque blocks and live finite energy stores control this field.
    // Neither player-held torches nor hidden ambient/skylight are invented here.
    let pressured_by_light =
        assess_local_illumination(registries, state, from).level() >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let recently_hit = state.kobold_state().colony.as_ref().is_some_and(|colony| {
        colony.recently_shoved(super::KoboldRole::Scout, state.control_tick())
    });
    let pressured = pressured_by_player || pressured_by_light || recently_hit;
    // Meals use the existing sparse scout schedule, never a per-control-tick AI update.
    // Den resources remain physically recoverable until they are actually eaten.
    let (consumed_food, discarded_food, cleared_cache) =
        apply_scheduled_den_care(registries, state, from, next_tick, pressured);
    let (heard_junk_alarm, alarm_pulses) = apply_watchman_tick(registries, state, next_tick);
    let pressured = pressured || heard_junk_alarm;
    // Copper fitting occupies two real slow-clock work slots at the junction.
    // Player pressure, alarm warnings and absent parts interrupt unfinished work.
    if apply_pry_fitting_tick(state, next_tick, pressured) {
        return (None, alarm_pulses);
    }
    let colony = state
        .kobold_state()
        .colony
        .as_ref()
        .expect("colony still exists");
    let salvage_batch = colony.salvage_batch();
    let forage_batch = colony.forage_batch();
    let forage_interval = colony.forage_interval();
    let mobilized = colony.mobilized_at.is_some();
    let den_space = den_cache_space(state, colony.route[0]);
    let raid_interest = scout_raid_interest(
        registries,
        state,
        colony,
        from,
        den_space,
        forage_ready,
        next_tick,
        player_position,
        pressured,
    );
    let move_plan = plan_scout_move(
        registries,
        state,
        colony,
        next_tick,
        pressured,
        raid_interest,
        player_position,
        den_space,
        forage_ready,
    );
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
    let food_only = should_seek_local_food(
        registries,
        state,
        from,
        den_space,
        forage_ready,
        next_tick,
        colony.hungry,
    );
    let (carried_drop, cargo_blocked_at_den, dragged_candidate) = plan_returning_cargo(
        registries,
        state,
        colony,
        CargoDecision {
            from,
            to: position,
            returning,
            frightened,
            den_space,
            food_only,
            mobilized,
            at: next_tick,
        },
    );
    let (can_move, continuing_carried_custody) = scout_cargo_movement_allowed(
        state,
        physically_clear,
        returning,
        frightened,
        cargo_blocked_at_den,
        carried_drop,
        dragged_candidate,
    );
    // Inspect the destination before any relocation. A scout dragging its own
    // parcel home has not discovered a second exposed item there.
    let discovered = can_move
        && finds_external_drop(
            registries,
            state,
            position,
            den_space,
            next_tick,
            [cleared_cache, discarded_food],
        );
    let first_relocated = if can_move {
        dragged_candidate
            .filter(|id| relocate_scavenged_drop_step(registries, state, *id, from, position))
    } else {
        None
    };
    // Whole portable parcels retain identity. Oversized exposed piles instead
    // yield a bounded daughter parcel, leaving the exact remainder at the source.
    let bulk_source = (can_move && returning && !frightened && first_relocated.is_none())
        .then(|| {
            local_split_source(
                registries, state, from, den_space, next_tick, food_only, mobilized,
            )
        })
        .flatten();
    let split_drop = bulk_source.and_then(|source| {
        split_scavenged_bulk_with_available_tool(
            registries,
            state,
            source,
            from,
            position,
            next_tick,
            salvage_batch,
            den_space,
        )
    });
    // An unattended food patch is genuine competition even if the player has
    // never dropped inventory. Loose parcels have priority; the scout forages at
    // the junction only while returning to its adjacent den, in one conserved
    // transaction that leaves the finite catch recoverable.
    let forage_source = (can_move
        && returning
        && may_forage_on_return
        && first_relocated.is_none()
        && split_drop.is_none()
        && !frightened
        && forage_ready)
        .then(|| {
            best_local_forage(
                registries,
                state,
                from,
                next_tick,
                forage_batch.min(den_space),
            )
            .map(|(_, id)| id)
        })
        .flatten();
    let foraged_food = forage_source.and_then(|source| {
        forage_food_to_adjacent_den(
            registries,
            state,
            source,
            from,
            position,
            next_tick,
            forage_batch.min(den_space),
        )
    });
    // Keep the original scrap only when no food transfer succeeded.
    let (relocated_drop, switched_cargo) = finish_return_cargo_exchange(
        registries,
        state,
        from,
        position,
        carried_drop,
        first_relocated,
        can_move && continuing_carried_custody,
        split_drop
            .map(|(id, _)| id)
            .or(foraged_food.map(|(id, _)| id)),
    );
    // A recovered handle remains at the junction for a later stone wedge.
    // The assembled kit likewise stays at its physical worksite, not in
    // abstract scout equipment or the den cache.
    let staged_tool_part = relocated_part_stays_at_junction(state, relocated_drop);
    let abandoned_drop = (scout_releases_cargo(frightened, cargo_blocked_at_den, can_move)
        || switched_cargo)
        .then_some(carried_drop)
        .flatten();
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
        let activity = KoboldTrailKind::from_step(
            frightened,
            turned_back_at_barrier,
            relocated_drop.is_some() || cleared_cache.is_some() || discarded_food.is_some(),
            split_drop.is_some(),
            foraged_food.is_some(),
        );
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
    let observed_discard = discarded_food.filter(|_| den_visible);
    let observed_clearing = cleared_cache.filter(|_| den_visible);
    let had_local_activity = [
        can_move,
        heard_junk_alarm,
        abandoned_drop.is_some(),
        observed_meal.is_some(),
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
            discovered_player_drop: discovered,
            relocated_drop,
            fled_player: (pressured_by_player || ahead_is_guarded) && can_move,
            fled_light: (pressured_by_light || (turned_back_at_barrier && ahead_is_lit))
                && can_move,
            turned_back_at_barrier: turned_back_at_barrier && can_move,
            cancelled_empty_cache_raid: cancelled_empty_cache_raid && can_move,
            abandoned_drop,
            foraged_food,
            split_drop,
            consumed_food: observed_meal,
            discarded_food: observed_discard,
            cleared_cache: observed_clearing,
        }),
        alarm_pulses,
    )
}
