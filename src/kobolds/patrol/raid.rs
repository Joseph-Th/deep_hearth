//! Read-only planning of the scout's local interest and bounded side raids.

mod continuation;

use continuation::{continue_to_planned_far_raid, recoverable_local_food_serving};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::super::junkworks::{
    den_or_relay_needs_striker, den_or_relay_wants_iron_chime, is_iron_chime_striker,
    is_junk_striker,
};
use super::super::scavenging::{
    ScavengeRoute, best_local_forage, has_local_scout_interest, is_portable_drop,
    local_portable_source, reachable_detour_drop, reachable_detour_food, reachable_far_stockpile,
    reachable_inherited_salvage, reachable_side_stockpile, scout_wants_loose_drop,
};
use super::super::scrap_tools::{is_needed_outer_tool_part, junction_awaits_better_head};
use super::super::visibility::scout_visible_from;
use super::super::{KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_MASS};
use super::{ScoutPatrolContext, scout_can_enter, scout_sees_guarded_cell};

/// One scout observation shared between turn-home and detour decisions.
/// Preserve the chosen visible forage route rather than independently searching
/// and potentially contradicting the earlier local-food decision.
#[derive(Clone, Copy)]
pub(super) struct ScoutRaidInterest {
    pub(super) exposed_at_current: bool,
    pub(super) inherited_target: Option<(VoxelCoord, Option<VoxelCoord>)>,
    pub(super) preferred_forage: Option<(VoxelCoord, Option<VoxelCoord>)>,
    pub(super) preferred_water: Option<(VoxelCoord, Option<VoxelCoord>)>,
    // A real, visible part needed at the patrol tip outranks optional side scavenging.
    pub(super) needed_tip_salvage: bool,
}

/// Choose a larger observed ration only when nearby food is insufficient.
/// Comparing actual transferable portions prevents one tiny patch from consuming
/// the full forage cooldown while a reachable full serving goes untouched.
fn preferred_hungry_forage(
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
    } = context;
    if pressured
        || !colony.hungry
        || colony.scout_step != 1
        || colony.returning
        || !forage_ready
        || den_space.is_zero()
    {
        return None;
    }
    let maximum_mass = colony
        .forage_batch_in_world(registries, state, from, at)
        .min(den_space);
    let local_portable = local_portable_source(
        registries,
        state,
        from,
        den_space,
        at,
        true,
        colony.mobilized_at.is_some(),
    );
    // Do not compare the largest visible parcel with distant forage: the
    // return edge may choose a smaller intact parcel before a bulk split.
    // Project only the serving that the canonical return-edge order can move.
    let nearby =
        recoverable_local_food_serving(registries, state, colony, context, local_portable, true)
            .unwrap_or(Mass::ZERO);
    let shortage = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident's partial meal is below one serving");
    if nearby >= shortage {
        return None;
    }
    let chosen = reachable_detour_food(
        registries,
        state,
        ScavengeRoute::new(colony.route, player_position),
        at,
        maximum_mass,
        shortage,
    )?;
    let goal = chosen.1.unwrap_or(chosen.0);
    let far_maximum = colony
        .forage_batch_in_world(registries, state, goal, at)
        .min(den_space);
    let far = best_local_forage(registries, state, goal, at, far_maximum)?.0;
    (far > nearby).then_some(chosen)
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
    } = context;
    // Water is the first outbound choice. Once a real water route is visible,
    // speculative food and salvage searches cannot change this patrol beat.
    // Do not traverse those unrelated resource indexes just to discard them.
    let preferred_water = (at >= colony.next_drink_at
        && colony.scout_step == 1
        && !colony.returning
        && !pressured
        && colony.scavenged_drop.is_none())
    .then(|| {
        super::super::hydration::reachable_water_from_junction(
            registries,
            state,
            colony,
            player_position,
        )
    })
    .flatten();
    let pursuing_water = preferred_water.is_some();
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
    let preferred_forage = (!pursuing_water)
        .then(|| preferred_hungry_forage(registries, state, colony, context))
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
        && scout_visible_from(registries, state, from, tip)
        && scout_can_enter(registries, state, tip, player_position)
        && !scout_sees_guarded_cell(registries, state, from, tip)
        && assess_local_illumination(registries, state, tip).level() < SCOUT_DETERRENT_LIGHT_LEVEL;
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
        && preferred_forage.is_none();
    ScoutRaidInterest {
        exposed_at_current: exposed,
        inherited_target,
        preferred_forage,
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
        preferred_forage,
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
                reachable_detour_food(registries, state, route, at, maximum_mass, meal_shortage)
            })
            .flatten()
    };
    let inherited_salvage = || inherited_target;
    if super::super::hydration::prolonged_water_deprivation(colony, at) {
        // The scout may still carry a meal, or follow its real water lead,
        // but stops making optional ore and equipment salvage expeditions.
        return colony
            .hungry
            .then(|| {
                loose_cache(Some(meal_shortage))
                    .or(preferred_forage)
                    .or_else(wild_food)
            })
            .flatten();
    }
    if colony.hungry {
        // A visibly larger wild serving can justify one extra edge, but an
        // adequate portable ration is simpler to recover and avoids the
        // foraging cooldown. Compare actual transfer yields, not mere presence.
        let food_choice = || {
            let loose = loose_cache(Some(meal_shortage));
            let (Some(drop_route), Some(forage_route)) = (loose, preferred_forage) else {
                return loose.or(preferred_forage);
            };
            let drop_goal = drop_route.1.unwrap_or(drop_route.0);
            let portable = local_portable_source(
                registries,
                state,
                drop_goal,
                den_space,
                at,
                true,
                colony.mobilized_at.is_some(),
            );
            // Reuse the return-edge's actual one-parcel choice. A small intact
            // morsel must not hide a complete meal from the same site's bulk
            // cache when comparing this visit with a separate forage route.
            let drop_yield = recoverable_local_food_serving(
                registries,
                state,
                colony,
                ScoutPatrolContext {
                    from: drop_goal,
                    forage_ready: false,
                    ..context
                },
                portable,
                true,
            );
            let forage_goal = forage_route.1.unwrap_or(forage_route.0);
            let forage_yield = best_local_forage(
                registries,
                state,
                forage_goal,
                at,
                colony.forage_batch().min(den_space),
            )
            .map(|(mass, _)| mass);
            match (drop_yield, forage_yield) {
                (Some(drop), Some(forage)) if drop >= meal_shortage || drop >= forage => {
                    Some(drop_route)
                }
                (Some(_), Some(_)) | (None, Some(_)) => Some(forage_route),
                (Some(_), None) => Some(drop_route),
                (None, None) => None,
            }
        };
        // Food first, then the remembered machine-service salvage priority.
        // Ordinary nearby scraps remain available when neither is reachable.
        let priority_route = food_choice().or_else(wild_food).or_else(inherited_salvage);
        if priority_route.is_some() || needed_tip_salvage {
            priority_route
        } else {
            loose_cache(None)
        }
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
    }
}
