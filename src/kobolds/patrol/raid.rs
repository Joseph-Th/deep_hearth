//! Read-only planning of the scout's local interest and bounded side raids.

use crate::core::state::AppState;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::super::feeding::scout_wants_food_opportunity;
use super::super::junkworks::{
    den_or_relay_needs_striker, den_or_relay_wants_iron_chime, is_iron_chime_striker,
    is_junk_striker,
};
use super::super::scavenging::{
    ScavengeRoute, best_local_forage, has_local_scout_interest, is_portable_drop,
    local_portable_source, local_split_source, matches_salvage_goal, reachable_detour_drop,
    reachable_detour_food, reachable_inherited_salvage, scout_wants_loose_drop,
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
    let maximum_mass = colony.forage_batch().min(den_space);
    let local_forage =
        best_local_forage(registries, state, from, at, maximum_mass).map(|(mass, _)| mass);
    let local_drop = local_portable_source(
        registries,
        state,
        from,
        den_space,
        at,
        true,
        colony.mobilized_at.is_some(),
    )
    .and_then(|id| state.surface().get(id))
    .map(|resource| resource.remaining_mass());
    let nearby = local_forage.into_iter().chain(local_drop).max()?;
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
    let far = best_local_forage(registries, state, goal, at, maximum_mass)?.0;
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
    let inherited_target = (colony.mobilized_at.is_some()
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
    let local_food = colony.hungry
        && has_local_scout_interest(registries, state, from, capacity, forage_ready, at, true);
    let preferred_forage = preferred_hungry_forage(registries, state, colony, context);
    // A shaped striker on the authored outer patrol is a real salvage goal too.
    // A fed kobold bypasses nearer inert rock only while the part is still
    // visible, den-fitting, physically reachable, unguarded, and unlit.
    let missing_striker_at_tip = colony.mobilized_at.is_some()
        && colony.scout_step == 1
        && !colony.returning
        && !pressured
        && den_or_relay_needs_striker(state, colony.route[0])
        && state
            .surface()
            .available_resources_at(Some(colony.route[2]))
            .any(|resource| {
                is_portable_drop(resource)
                    && resource.remaining_mass() <= capacity
                    && is_junk_striker(resource)
                    && scout_wants_loose_drop(registries, state, resource, at)
            })
        && scout_visible_from(registries, state, from, colony.route[2])
        && scout_can_enter(registries, state, colony.route[2], player_position)
        && !scout_sees_guarded_cell(registries, state, from, colony.route[2])
        && assess_local_illumination(registries, state, colony.route[2]).level()
            < SCOUT_DETERRENT_LIGHT_LEVEL;
    // Having actually seen a usable metal replacement, the scout will not
    // turn home for inert junk at the junction. Visibility, carriage, den
    // capacity and deterrents are checked before treating this as a goal.
    let useful_iron_at_tip = colony.mobilized_at.is_some()
        && colony.scout_step == 1
        && !colony.returning
        && !pressured
        && den_or_relay_wants_iron_chime(state, colony)
        && state
            .surface()
            .available_resources_at(Some(colony.route[2]))
            .any(|resource| {
                is_portable_drop(resource)
                    && resource.remaining_mass() <= capacity
                    && is_iron_chime_striker(resource)
            })
        && scout_visible_from(registries, state, from, colony.route[2])
        && scout_can_enter(registries, state, colony.route[2], player_position)
        && !scout_sees_guarded_cell(registries, state, from, colony.route[2])
        && assess_local_illumination(registries, state, colony.route[2]).level()
            < SCOUT_DETERRENT_LIGHT_LEVEL;
    let missing_tool_at_tip = !pressured
        && colony.scout_step == 1
        && !colony.returning
        && reachable_needed_tool_at_tip(registries, state, colony, context);
    let exposed = (has_local_scout_interest(
        registries,
        state,
        from,
        capacity,
        forage_ready,
        at,
        colony.seeks_food_at_junction(),
    ) || (colony.scout_step == 2 && local_food))
        && ((inherited_target.is_none()
            && !missing_striker_at_tip
            && !useful_iron_at_tip
            && !missing_tool_at_tip)
            || local_food)
        && !continue_to_planned_far_raid(registries, state, colony, context)
        && preferred_forage.is_none();
    ScoutRaidInterest {
        exposed_at_current: exposed,
        inherited_target,
        preferred_forage,
    }
}

/// Repair priority requires a visible, reachable physical part on the patrol.
fn reachable_needed_tool_at_tip(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
) -> bool {
    if !junction_awaits_better_head(state, colony) {
        return false;
    }
    let ScoutPatrolContext {
        from,
        den_space: capacity,
        at,
        player_position,
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
        && scout_visible_from(registries, state, from, tip)
        && scout_can_enter(registries, state, tip, player_position)
        && !scout_sees_guarded_cell(registries, state, from, tip)
        && assess_local_illumination(registries, state, tip).level() < SCOUT_DETERRENT_LIGHT_LEVEL
}

/// Do not abandon a viable distant ration for an insufficient local crumb, or
/// an already-chosen whole-cargo raid for a bulky intermediate pile. Re-evaluate
/// physical supplies, sight and safety at the actual intermediate step.
fn continue_to_planned_far_raid(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
) -> bool {
    if colony.scout_step != 2 || colony.returning {
        return false;
    }
    let Some(goal) = colony.detour_goal else {
        return false;
    };
    let ScoutPatrolContext {
        from,
        den_space: capacity,
        forage_ready,
        at,
        player_position,
        ..
    } = context;
    let needs_striker =
        colony.mobilized_at.is_some() && den_or_relay_needs_striker(state, colony.route[0]);
    let target_drop = local_portable_source(
        registries,
        state,
        goal,
        capacity,
        at,
        false,
        colony.mobilized_at.is_some(),
    )
    .and_then(|id| state.surface().get(id));
    let target_forage = (colony.hungry && forage_ready)
        .then(|| {
            best_local_forage(
                registries,
                state,
                goal,
                at,
                colony.forage_batch().min(capacity),
            )
        })
        .flatten();
    let meal_shortage = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident's partial meal is below a full ration");
    // The goal can gain unrelated loose scrap during the scout's first edge.
    // Never let that newer inert parcel conceal the edible patch the scout
    // actually planned to reach. A complete drop remains cheaper than foraging.
    let (target_food, target_precision, target_mass) = match (target_drop, target_forage) {
        (Some(drop), Some((forage_mass, _)))
            if drop.commodity().form() != crate::content::FORM_FOOD
                || (drop.remaining_mass() < meal_shortage
                    && forage_mass > drop.remaining_mass()) =>
        {
            (true, false, forage_mass)
        }
        (Some(drop), _) => (
            drop.commodity().form() == crate::content::FORM_FOOD,
            colony.mobilized_at.is_some() && matches_salvage_goal(drop, needs_striker),
            drop.remaining_mass(),
        ),
        (None, Some((mass, _))) => (true, false, mass),
        (None, None) => return false,
    };
    let local_whole = local_portable_source(
        registries,
        state,
        from,
        capacity,
        at,
        target_food && colony.hungry,
        colony.mobilized_at.is_some(),
    );
    let local_food = colony.hungry
        && has_local_scout_interest(registries, state, from, capacity, forage_ready, at, true);
    let local_serving = local_whole
        .and_then(|id| state.surface().get(id))
        .filter(|resource| resource.commodity().form() == crate::content::FORM_FOOD)
        .map(|resource| resource.remaining_mass())
        .into_iter()
        .chain(
            (forage_ready && colony.hungry)
                .then(|| {
                    best_local_forage(
                        registries,
                        state,
                        from,
                        at,
                        colony.forage_batch().min(capacity),
                    )
                    .map(|(mass, _)| mass)
                })
                .flatten(),
        )
        .max();
    let better_here = if local_food && !target_food {
        true
    } else if colony.hungry && target_food {
        local_serving.is_some_and(|serving| serving >= meal_shortage || serving >= target_mass)
    } else if target_precision {
        local_whole.is_some_and(|id| {
            state
                .surface()
                .get(id)
                .is_some_and(|drop| matches_salvage_goal(drop, needs_striker))
        })
    } else {
        local_whole.is_some()
    };
    !better_here
        && scout_visible_from(registries, state, from, goal)
        && scout_can_enter(registries, state, goal, player_position)
        && !scout_sees_guarded_cell(registries, state, from, goal)
        && assess_local_illumination(registries, state, goal).level() < SCOUT_DETERRENT_LIGHT_LEVEL
}

pub(super) fn choose_outbound_detour(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
    step: u8,
    returning: bool,
    inherited_target: Option<(VoxelCoord, Option<VoxelCoord>)>,
    preferred_forage: Option<(VoxelCoord, Option<VoxelCoord>)>,
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
    let junction = colony.route[1];
    let route = ScavengeRoute::new(colony.route, player_position);
    let loose_cache =
        |food_only| reachable_detour_drop(registries, state, route, den_space, at, food_only);
    let wild_food = || {
        let local_food_remains = state
            .surface()
            .available_resources_at(Some(junction))
            .any(|resource| scout_wants_food_opportunity(registries, resource, at));
        (forage_ready && !den_space.is_zero() && !local_food_remains)
            .then(|| {
                let maximum_mass = colony.forage_batch().min(den_space);
                let shortage = SCOUT_MEAL_MASS
                    .checked_sub(colony.partial_meal)
                    .expect("trusted resident's partial meal is below a full ration");
                reachable_detour_food(registries, state, route, at, maximum_mass, shortage)
            })
            .flatten()
    };
    let inherited_salvage = || inherited_target;
    if colony.hungry {
        // A visibly larger wild serving can justify one extra edge, but an
        // adequate portable ration is simpler to recover and avoids the
        // foraging cooldown. Compare actual transfer yields, not mere presence.
        let food_choice = || {
            let loose = loose_cache(true);
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
            )
            .and_then(|id| state.surface().get(id))
            .map(|drop| drop.remaining_mass());
            let drop_yield = portable.or_else(|| {
                local_split_source(
                    registries,
                    state,
                    drop_goal,
                    den_space,
                    at,
                    true,
                    colony.mobilized_at.is_some(),
                )
                .and_then(|id| state.surface().get(id))
                .map(|drop| {
                    drop.remaining_mass()
                        .min(colony.salvage_batch())
                        .min(den_space)
                })
            });
            let forage_goal = forage_route.1.unwrap_or(forage_route.0);
            let forage_yield = best_local_forage(
                registries,
                state,
                forage_goal,
                at,
                colony.forage_batch().min(den_space),
            )
            .map(|(mass, _)| mass);
            let shortage = SCOUT_MEAL_MASS
                .checked_sub(colony.partial_meal)
                .expect("trusted resident's partial meal is below a full ration");
            match (drop_yield, forage_yield) {
                (Some(drop), Some(forage)) if drop >= shortage || drop >= forage => {
                    Some(drop_route)
                }
                (Some(_), Some(_)) | (None, Some(_)) => Some(forage_route),
                (Some(_), None) => Some(drop_route),
                (None, None) => None,
            }
        };
        // Food first, then the remembered machine-service salvage priority.
        // Ordinary nearby scraps remain available when neither is reachable.
        food_choice()
            .or_else(wild_food)
            .or_else(inherited_salvage)
            .or_else(|| loose_cache(false))
    } else {
        inherited_salvage()
            .or_else(|| loose_cache(false))
            .or_else(wild_food)
    }
}
