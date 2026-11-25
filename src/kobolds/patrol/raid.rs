//! Read-only planning of the scout's local interest and bounded side raids.

mod continuation;

use continuation::{continue_to_planned_far_raid, recoverable_local_food_serving};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::super::junkworks::{
    den_or_relay_needs_striker, den_or_relay_wants_iron_chime, is_iron_chime_striker,
    is_junk_striker,
};
use super::super::scavenging::{
    ScavengeRoute, best_local_forage, has_local_scout_interest, is_portable_drop,
    local_portable_source, reachable_detour_drop, reachable_far_stockpile, reachable_forage_route,
    reachable_inherited_salvage, reachable_side_stockpile, scout_wants_loose_drop,
};
use super::super::scrap_tools::{is_needed_outer_tool_part, junction_awaits_better_head};
use super::super::{KoboldColony, SCOUT_MEAL_MASS};
use super::{ScoutPatrolContext, scout_can_approach_visible_unlit_cell};

/// One scout observation shared between turn-home and detour decisions.
/// Preserve the chosen visible food route rather than independently searching
/// and potentially contradicting the earlier local-food decision.
#[derive(Clone, Copy)]
pub(super) struct ScoutRaidInterest {
    pub(super) exposed_at_current: bool,
    pub(super) inherited_target: Option<(VoxelCoord, Option<VoxelCoord>)>,
    pub(super) preferred_food: Option<(VoxelCoord, Option<VoxelCoord>)>,
    pub(super) preferred_water: Option<(VoxelCoord, Option<VoxelCoord>)>,
    // A real, visible part needed at the patrol tip outranks optional side scavenging.
    pub(super) needed_tip_salvage: bool,
}

/// Choose a larger observed ration only when the local return edge cannot
/// satisfy the remaining meal. Loose food remains an option on forage cooldown.
fn preferred_hungry_food_route(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let ScoutPatrolContext {
        from,
        den_space,
        forage_ready,
        at,
        player_position,
        pressured,
        ..
    } = context;
    if pressured
        || !colony.hungry
        || colony.scout_step != 1
        || colony.returning
        || den_space.is_zero()
    {
        return None;
    }
    let serving_at = |position, can_forage| {
        let portable = local_portable_source(
            registries,
            state,
            position,
            den_space,
            at,
            true,
            colony.mobilized_at.is_some(),
        );
        recoverable_local_food_serving(
            registries,
            state,
            colony,
            ScoutPatrolContext {
                from: position,
                forage_ready: can_forage,
                ..context
            },
            portable,
            true,
        )
    };
    let nearby = serving_at(from, forage_ready).unwrap_or(Mass::ZERO);
    let shortage = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident's partial meal is below one serving");
    if nearby >= shortage {
        return None;
    }
    let route = ScavengeRoute::new(colony.route, player_position);
    let loose = reachable_detour_drop(registries, state, route, den_space, at, Some(shortage))
        .and_then(|chosen| {
            let mass = serving_at(chosen.1.unwrap_or(chosen.0), false)?;
            (mass > nearby).then_some((chosen, mass))
        });
    if let Some((chosen, mass)) = loose
        && mass >= shortage
    {
        return Some(chosen);
    }
    let forage = forage_ready
        .then(|| {
            let maximum_mass = colony
                .forage_batch_in_world(registries, state, from, at)
                .min(den_space);
            let chosen =
                reachable_forage_route(registries, state, route, at, maximum_mass, shortage)?;
            let goal = chosen.1.unwrap_or(chosen.0);
            let mass = serving_at(goal, true)?;
            (mass > nearby).then_some((chosen, mass))
        })
        .flatten();
    match (loose, forage) {
        (Some((_, drop_mass)), Some((chosen, forage_mass))) if forage_mass > drop_mass => {
            Some(chosen)
        }
        (Some((chosen, _)), _) | (None, Some((chosen, _))) => Some(chosen),
        (None, None) => None,
    }
}

/// Determine whether a den's expelled inert parcel should be ignored in
/// favor of a reachable piece of inherited machinery. The indexed target is
/// then reused by movement planning, avoiding repeated two-edge searches.
pub(super) fn scout_raid_interest(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
) -> ScoutRaidInterest {
    let ScoutPatrolContext {
        from,
        den_space: capacity,
        forage_ready,
        at,
        player_position,
        pressured,
        water_trip,
    } = context;
    // The tick coordinator already resolved the viable drinking route before
    // optional work. Do not override it or repeat the same resource search.
    let preferred_water = water_trip.and_then(|trip| trip.outbound_route());
    let pursuing_water = water_trip.is_some();
    let water_deprived = super::super::hydration::prolonged_water_deprivation(colony, at);
    let inherited_target = (!pursuing_water
        && colony.mobilized_at.is_some()
        && !water_deprived
        && !pressured
        && colony.scout_step == 1
        && !colony.returning)
        .then(|| {
            reachable_inherited_salvage(
                registries,
                state,
                ScavengeRoute::new(colony.route, player_position),
                from,
                capacity,
                at,
            )
        })
        .flatten();
    let local_food = !pursuing_water
        && colony.hungry
        && has_local_scout_interest(registries, state, from, capacity, forage_ready, at, true);
    let preferred_food = (!pursuing_water)
        .then(|| preferred_hungry_food_route(registries, state, colony, context))
        .flatten();
    // A real part at the main patrol tip is worth reaching before optional
    // side scrap. Check capacity, handling, sight and danger once for both
    // a missing striker and a higher-quality replacement; neither is a
    // valid goal simply because the item's shape matches the rig.
    let can_seek_tip_part = !pursuing_water
        && colony.mobilized_at.is_some()
        && !water_deprived
        && colony.scout_step == 1
        && !colony.returning
        && !pressured;
    let needs_striker = can_seek_tip_part && den_or_relay_needs_striker(state, colony.route[0]);
    let wants_iron = can_seek_tip_part && den_or_relay_wants_iron_chime(state, colony);
    let needed_junk_at_tip = (needs_striker || wants_iron)
        && state
            .surface()
            .available_resources_at(Some(colony.route[2]))
            .any(|resource| {
                is_portable_drop(resource)
                    && resource.remaining_mass() <= capacity
                    && scout_wants_loose_drop(registries, state, resource, at)
                    && ((needs_striker && is_junk_striker(resource))
                        || (wants_iron && is_iron_chime_striker(resource)))
            });
    let missing_tool_at_tip = !pursuing_water
        && !pressured
        && !water_deprived
        && colony.scout_step == 1
        && !colony.returning
        && needed_tool_present_at_tip(registries, state, colony, context);
    // Both tool and machinery salvage must pass the exact same sight,
    // body-clearance, guard, and light test. Resolve this once per decision.
    let tip = colony.route[2];
    let needed_tip_salvage = (needed_junk_at_tip || missing_tool_at_tip)
        && scout_can_approach_visible_unlit_cell(registries, state, from, tip, player_position);
    let exposed = !pursuing_water
        && (has_local_scout_interest(
            registries,
            state,
            from,
            capacity,
            forage_ready,
            at,
            colony.seeks_food_at_junction(),
        ) || (colony.scout_step == 2 && local_food))
        && ((inherited_target.is_none() && !needed_tip_salvage) || local_food)
        && !continue_to_planned_far_raid(registries, state, colony, context)
        && preferred_food.is_none();
    ScoutRaidInterest {
        exposed_at_current: exposed,
        inherited_target,
        preferred_food,
        preferred_water,
        needed_tip_salvage,
    }
}

/// Repair priority requires a portable part at the patrol tip.
/// Shared sight, safety, and clearance are checked alongside other tip salvage.
fn needed_tool_present_at_tip(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
) -> bool {
    if !junction_awaits_better_head(state, colony) {
        return false;
    }
    let ScoutPatrolContext {
        den_space: capacity,
        at,
        ..
    } = context;
    let tip = colony.route[2];
    state
        .surface()
        .available_resources_at(Some(tip))
        .any(|resource| {
            is_portable_drop(resource)
                && resource.remaining_mass() <= capacity
                && scout_wants_loose_drop(registries, state, resource, at)
                && is_needed_outer_tool_part(state, tip, resource)
        })
}

pub(super) fn choose_outbound_detour(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
    step: u8,
    returning: bool,
    interest: ScoutRaidInterest,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    if step != 2 || returning || colony.scout_step != 1 {
        return None;
    }
    let ScoutPatrolContext {
        den_space,
        forage_ready,
        at,
        player_position,
        ..
    } = context;
    let ScoutRaidInterest {
        inherited_target,
        preferred_food,
        preferred_water,
        needed_tip_salvage,
        ..
    } = interest;
    if let Some(route) = preferred_water {
        // The water planner also recognizes the normal route's next cell.
        // Preserve that urgent choice without marking it as a side detour.
        return (route != (colony.route[2], None)).then_some(route);
    }
    // Going to the nearby, already-seen part needs no side route. A selected
    // inherited salvage mission still wins, and urgent food or water comes first.
    if needed_tip_salvage && inherited_target.is_none() && !colony.hungry {
        return None;
    }
    let junction = colony.route[1];
    let route = ScavengeRoute::new(colony.route, player_position);
    let meal_shortage = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident's partial meal is below a full ration");
    let loose_cache =
        |shortage| reachable_detour_drop(registries, state, route, den_space, at, shortage);
    let wild_food = || {
        let maximum_mass = colony.forage_batch().min(den_space);
        let local_food_remains =
            best_local_forage(registries, state, junction, at, maximum_mass).is_some();
        (forage_ready && !den_space.is_zero() && !local_food_remains)
            .then(|| {
                reachable_forage_route(registries, state, route, at, maximum_mass, meal_shortage)
            })
            .flatten()
    };
    let inherited_salvage = || inherited_target;
    let water_deprived = super::super::hydration::prolonged_water_deprivation(colony, at);
    let chosen = if colony.hungry {
        let food = preferred_food;
        // Water deprivation suppresses optional salvage, not the normal
        // comparison between physically recoverable food servings.
        if water_deprived {
            food
        } else {
            let priority_route = food.or_else(inherited_salvage);
            if priority_route.is_some() || needed_tip_salvage {
                priority_route
            } else {
                loose_cache(None)
            }
        }
    } else if water_deprived {
        None
    } else {
        inherited_salvage()
            .or_else(|| loose_cache(None))
            .or_else(|| {
                (colony.mobilized_at.is_some() && !den_space.is_zero())
                    .then(|| {
                        reachable_side_stockpile(registries, state, route, at)
                            .map(|cell| (cell, None))
                            .or_else(|| {
                                reachable_far_stockpile(registries, state, route, at)
                                    .map(|(first, goal)| (first, Some(goal)))
                            })
                    })
                    .flatten()
            })
            .or_else(wild_food)
    };
    // A normal patrol destination is a route choice, not a persisted detour.
    chosen.filter(|route| *route != (colony.route[2], None))
}
