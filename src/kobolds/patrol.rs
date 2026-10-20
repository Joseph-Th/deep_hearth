//! Scout patrol planning and conserved physical scavenging on the slow tick.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, forage_food_to_adjacent_den,
    relocate_scavenged_drop_step, split_bulk_scavenged_drop_step,
};
use crate::survival::Vitality;

use super::feeding::{apply_scheduled_den_care, scout_wants_drop};
use super::scavenging::{
    den_cache_space, is_contestable_drop, is_portable_drop, reachable_far_drop,
    reachable_side_drop, reachable_side_food, scout_loot_priority,
};
use super::visibility::scout_visible_from;
use super::{
    KoboldColony, KoboldScoutEvent, SCOUT_BULK_SPLIT_BATCH, SCOUT_DETERRENT_LIGHT_LEVEL,
    SCOUT_FORAGE_BATCH, SCOUT_FORAGE_INTERVAL_TICKS, SCOUT_STEP_TICKS, SCOUT_WARY_INTERVAL_TICKS,
};

fn choose_outbound_detour(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    step: u8,
    returning: bool,
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
    at: SimulationTick,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    if step != 2 || returning || colony.scout_step != 1 {
        return None;
    }
    let [den, junction, tip] = colony.route;
    reachable_side_drop(
        registries,
        state,
        junction,
        den,
        tip,
        player_position,
        den_space,
        at,
    )
    .map(|first| (first, None))
    .or_else(|| {
        reachable_far_drop(
            registries,
            state,
            junction,
            den,
            tip,
            player_position,
            den_space,
            at,
        )
        .map(|(first, goal)| (first, Some(goal)))
    })
    .or_else(|| {
        let local_food_remains =
            state
                .surface()
                .available_resources_at(Some(junction))
                .any(|resource| {
                    resource.kind() == SurfaceResourceKind::Opportunity
                        && resource.commodity().form() == crate::content::FORM_FOOD
                        && !resource.remaining_mass().is_zero()
                });
        (forage_ready && !den_space.is_zero() && !local_food_remains)
            .then(|| reachable_side_food(registries, state, junction, den, tip, player_position))
            .flatten()
            .map(|destination| (destination, None))
    })
}

pub(super) fn scout_can_enter(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    player_position != Some(position)
        && (state.world().generation_profile().is_none()
            || state
                .world()
                .has_standing_clearance(registries.world(), position))
}

fn living_player_deters_scout(state: &AppState, from: VoxelCoord) -> bool {
    let near = state.logistics().player().is_some_and(|player| {
        let position = player.position();
        from.y() == position.y()
            && from
                .x()
                .abs_diff(position.x())
                .saturating_add(from.z().abs_diff(position.z()))
                <= 1
    });
    near && state
        .survival()
        .player()
        .is_some_and(|player| player.vitality() != Vitality::ZERO)
}

#[derive(Clone, Copy)]
struct ScoutMovePlan {
    step: u8,
    returning: bool,
    detour: Option<(VoxelCoord, Option<VoxelCoord>)>,
    position: VoxelCoord,
    ahead_is_lit: bool,
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
    exposed_at_current: bool,
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
) -> ScoutMovePlan {
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
    );
    let proposed_position = proposed_detour.map_or_else(
        || colony.position_at_step(proposed_step),
        |(first, _)| first,
    );
    let ahead_is_lit = !proposed_returning
        && assess_local_illumination(registries, state, proposed_position).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let ahead_is_blocked = !scout_can_enter(registries, state, proposed_position, player_position);
    // A detour chosen at the junction is not a reservation. Player pickup or
    // another canonical surface transaction can remove or alter the target
    // during the scout's intervening physical step. Recheck its neighboring
    // goal rather than walking into an empty camp by stale distant knowledge.
    let cancelled_empty_cache_raid = proposed_step == 3
        && !proposed_returning
        && !state
            .surface()
            .available_resources_at(Some(proposed_position))
            .any(|resource| {
                is_contestable_drop(resource, den_space)
                    && scout_wants_drop(registries, resource, next_tick)
            });
    let turned_back_at_barrier = !proposed_returning
        && proposed_step > colony.scout_step
        && colony.scout_step > 0
        && (ahead_is_lit || ahead_is_blocked);
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
    ScoutMovePlan {
        step,
        returning,
        detour,
        position,
        ahead_is_lit,
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear: step != colony.scout_step
            && (returning || !ahead_is_lit)
            && scout_can_enter(registries, state, position, player_position),
    }
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
) -> Option<KoboldScoutEvent> {
    let colony = state.kobold_state().colony.as_ref()?;
    if colony.next_step_at > next_tick {
        return None;
    }
    let from = colony.scout_position();
    let forage_ready = colony.next_forage_at.is_none_or(|due| next_tick >= due);
    let player_position = state.logistics().player().map(|player| player.position());
    let pressured_by_player = living_player_deters_scout(state, from);
    // World-owned opaque blocks and live finite energy stores control this field.
    // Neither player-held torches nor hidden ambient/skylight are invented here.
    let pressured_by_light =
        assess_local_illumination(registries, state, from).level() >= SCOUT_DETERRENT_LIGHT_LEVEL;
    let pressured = pressured_by_player || pressured_by_light;
    // Meals use the existing sparse scout schedule, never a per-control-tick AI update.
    // Den resources remain physically recoverable until they are actually eaten.
    let (consumed_food, discarded_food) =
        apply_scheduled_den_care(registries, state, from, next_tick, pressured);
    let colony = state
        .kobold_state()
        .colony
        .as_ref()
        .expect("colony still exists");
    let den_space = den_cache_space(state, colony.route[0]);
    let exposed_at_current = state
        .surface()
        .available_resources_at(Some(from))
        .any(|resource| {
            is_contestable_drop(resource, den_space)
                && scout_wants_drop(registries, resource, next_tick)
        });
    let move_plan = plan_scout_move(
        registries,
        state,
        colony,
        next_tick,
        pressured,
        exposed_at_current,
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
        turned_back_at_barrier,
        cancelled_empty_cache_raid,
        physically_clear,
    } = move_plan;
    // A scout only scavenges during retreat. Player pickup may have exhausted its
    // previously dragged parcel between steps; never resurrect or replace that mass.
    let available_drop_at = |id: SurfaceResourceId| {
        state.surface().get(id).is_some_and(|resource| {
            resource.position() == from
                && resource.lifecycle() == SurfaceResourceLifecycle::Available
                && is_portable_drop(resource)
                && resource.remaining_mass() <= den_space
                && scout_wants_drop(registries, resource, next_tick)
        })
    };
    let abandoned_drop = pressured
        .then_some(colony.scavenged_drop)
        .flatten()
        .filter(|id| available_drop_at(*id));
    let dragged_candidate = (returning && !pressured)
        .then(|| {
            colony
                .scavenged_drop
                .filter(|id| available_drop_at(*id))
                .or_else(|| {
                    state
                        .surface()
                        .available_resources_at(Some(from))
                        .filter(|resource| {
                            is_portable_drop(resource)
                                && resource.remaining_mass() <= den_space
                                && scout_wants_drop(registries, resource, next_tick)
                        })
                        .min_by_key(|resource| scout_loot_priority(resource))
                        .map(|resource| resource.id())
                })
        })
        .flatten();
    // An exhausted surface revision must not let a creature move while silently
    // leaving still-held matter behind. No mutation is attempted when obstructed.
    let can_move =
        physically_clear && (dragged_candidate.is_none() || state.can_spend_surface_revisions(1));
    // Inspect the destination before any relocation. A scout dragging its own
    // parcel home has not discovered a second exposed item there.
    let discovered = can_move
        && state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| {
                is_contestable_drop(resource, den_space)
                    && scout_wants_drop(registries, resource, next_tick)
            });
    let relocated_drop = if can_move {
        dragged_candidate.filter(|id| relocate_scavenged_drop_step(state, *id, from, position))
    } else {
        None
    };
    // Whole portable parcels retain identity. Oversized exposed piles instead
    // yield a bounded daughter parcel, leaving the exact remainder at the source.
    let bulk_source = (can_move && returning && !pressured && relocated_drop.is_none())
        .then(|| {
            state
                .surface()
                .available_resources_at(Some(from))
                .filter(|resource| {
                    is_contestable_drop(resource, den_space)
                        && scout_wants_drop(registries, resource, next_tick)
                        && (!is_portable_drop(resource) || resource.remaining_mass() > den_space)
                })
                .min_by_key(|resource| scout_loot_priority(resource))
                .map(|resource| resource.id())
        })
        .flatten();
    let split_drop = bulk_source.and_then(|source| {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source,
            from,
            position,
            next_tick,
            SCOUT_BULK_SPLIT_BATCH.min(den_space),
        )
    });
    // An unattended food patch is genuine competition even if the player has
    // never dropped inventory. Loose parcels have priority; the scout forages at
    // the junction only while returning to its adjacent den, in one conserved
    // transaction that leaves the finite catch recoverable.
    let forage_source = (can_move
        && returning
        && step <= 1
        && relocated_drop.is_none()
        && split_drop.is_none()
        && !pressured
        && forage_ready)
        .then(|| {
            state
                .surface()
                .available_resources_at(Some(from))
                .find(|resource| {
                    resource.kind() == SurfaceResourceKind::Opportunity
                        && resource.commodity().form() == crate::content::FORM_FOOD
                })
                .map(|resource| resource.id())
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
            SCOUT_FORAGE_BATCH.min(den_space),
        )
    });
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
                .or(split_drop.map(|(id, _)| id))
                .or_else(|| (step > 0).then(|| foraged_food.map(|(id, _)| id)).flatten()),
        );
    }
    if pressured {
        // A trapped scout also relinquishes the parcel, enabling recovery without
        // inventing inaccessible enemy inventory or a damage/attack verb.
        colony.scavenged_drop = None;
    }
    colony.next_step_at = next_step_at;
    if pressured || (turned_back_at_barrier && ahead_is_lit && can_move) {
        colony.wary_until = Some(
            next_tick
                .checked_add_span(TickSpan::new(SCOUT_WARY_INTERVAL_TICKS))
                .expect("bounded simulation cannot overflow a scout caution deadline"),
        );
    }
    if foraged_food.is_some() {
        colony.next_forage_at = Some(
            next_tick
                .checked_add_span(TickSpan::new(SCOUT_FORAGE_INTERVAL_TICKS))
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
    let visible = (can_move
        || abandoned_drop.is_some()
        || observed_meal.is_some()
        || observed_discard.is_some())
        && player_position
            .is_some_and(|player| scout_visible_from(registries, state, scout_position, player));
    visible.then_some(KoboldScoutEvent {
        position: scout_position,
        discovered_player_drop: discovered,
        relocated_drop,
        fled_player: pressured_by_player && can_move,
        fled_light: (pressured_by_light || (turned_back_at_barrier && ahead_is_lit)) && can_move,
        turned_back_at_barrier: turned_back_at_barrier && can_move,
        cancelled_empty_cache_raid: cancelled_empty_cache_raid && can_move,
        abandoned_drop,
        foraged_food,
        split_drop,
        consumed_food: observed_meal,
        discarded_food: observed_discard,
    })
}
