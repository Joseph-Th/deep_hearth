//! Re-evaluate a previously selected far raid at its real intermediate waypoint.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::surface::SurfaceResourceId;

use super::super::super::hydration;
use super::super::super::junkworks::den_or_relay_needs_striker;
use super::super::super::scavenging::{
    best_local_forage, has_local_scout_interest, is_portable_drop, local_portable_source,
    local_split_source, matches_salvage_goal, preferred_salvage_split,
};
use super::super::super::{KoboldColony, SCOUT_MEAL_MASS};
use super::super::cargo::{forage_beats_loose_food, local_food_drop};
use super::super::{ScoutPatrolContext, scout_can_approach_visible_unlit_cell};

/// Do not abandon a viable distant ration for an insufficient local crumb, or
/// an already-chosen whole-cargo raid for a bulky intermediate pile. Re-evaluate
/// physical supplies, sight and safety at the actual intermediate step.
pub(super) fn continue_to_planned_far_raid(
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
    if context.at >= colony.next_drink_at
        && hydration::usable_water_at(registries, state, goal).is_some()
    {
        return scout_can_approach_visible_unlit_cell(
            registries,
            state,
            context.from,
            goal,
            context.player_position,
        );
    }
    let water_deprived = hydration::prolonged_water_deprivation(colony, context.at);
    if water_deprived && !colony.hungry {
        return false;
    }
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
    // Match the return-edge custody order: edible parcels first when hungry,
    // otherwise eligible salvage, with useful industrial splits before inert scrap.
    // A planned bulk destination stays meaningful at its recoverable yield.
    let target_drop = [true, false]
        .into_iter()
        .filter(|food_only| !*food_only || colony.hungry)
        .filter(|food_only| !water_deprived || *food_only)
        .find_map(|food_only| {
            let portable = local_portable_source(
                registries,
                state,
                goal,
                capacity,
                at,
                food_only,
                colony.mobilized_at.is_some(),
            );
            if food_only {
                local_food_drop(registries, state, colony, goal, capacity, at, portable)
                    .map(|drop| drop.source)
            } else {
                let mobilized = colony.mobilized_at.is_some();
                let preferred = mobilized
                    .then(|| {
                        preferred_salvage_split(registries, state, goal, capacity, at, portable)
                    })
                    .flatten();
                preferred.or(portable).or_else(|| {
                    local_split_source(registries, state, goal, capacity, at, false, mobilized)
                })
            }
        })
        .and_then(|id| state.surface().get(id))
        .map(|drop| {
            let transferable = if is_portable_drop(drop) && drop.remaining_mass() <= capacity {
                drop.remaining_mass()
            } else {
                drop.remaining_mass()
                    .min(colony.salvage_batch())
                    .min(capacity)
            };
            (drop, transferable)
        });
    let target_forage = (colony.hungry && forage_ready)
        .then(|| {
            best_local_forage(
                registries,
                state,
                goal,
                at,
                colony
                    .forage_batch_in_world(registries, state, goal, at)
                    .min(capacity),
            )
        })
        .flatten();
    let meal_shortage = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident's partial meal is below a full ration");
    // The goal can gain unrelated loose scrap during the scout's first edge.
    // Never let that newer inert parcel conceal the edible patch the scout
    // actually planned to reach. A complete drop remains cheaper than foraging.
    let (target_food, target_precision, target_mass, target_whole) =
        match (target_drop, target_forage) {
            (Some((drop, transferable)), Some((forage_mass, _)))
                if forage_beats_loose_food(
                    colony,
                    registries
                        .survival()
                        .get_food(drop.commodity())
                        .map(|_| transferable),
                    forage_mass,
                ) =>
            {
                (true, false, forage_mass, false)
            }
            (Some((drop, transferable)), _) => (
                registries.survival().get_food(drop.commodity()).is_some(),
                colony.mobilized_at.is_some() && matches_salvage_goal(drop, needs_striker),
                transferable,
                is_portable_drop(drop) && drop.remaining_mass() <= capacity,
            ),
            (None, Some((mass, _))) => (true, false, mass, false),
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
    let local_serving = (colony.hungry && target_food)
        .then(|| {
            recoverable_local_food_serving(
                registries,
                state,
                colony,
                context,
                local_whole,
                !target_whole || target_mass < meal_shortage,
            )
        })
        .flatten();
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
        && scout_can_approach_visible_unlit_cell(registries, state, from, goal, player_position)
}

/// Compare only the ration the next return edge would actually transfer.
/// A whole meal wins intact, but a complete bulk split outranks a tiny crumb;
/// a larger forage serving beats an insufficient loose parcel. This matches
/// cargo execution rather than projecting an independent maximum.
pub(super) fn recoverable_local_food_serving(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
    local_whole: Option<SurfaceResourceId>,
    allow_bulk_split: bool,
) -> Option<Mass> {
    let ScoutPatrolContext {
        from,
        den_space,
        forage_ready,
        at,
        ..
    } = context;
    let loose = local_food_drop(registries, state, colony, from, den_space, at, local_whole);
    let forage = forage_ready
        .then(|| {
            best_local_forage(
                registries,
                state,
                from,
                at,
                colony
                    .forage_batch_in_world(registries, state, from, at)
                    .min(den_space),
            )
        })
        .flatten();
    if let Some((mass, _)) = forage
        && forage_beats_loose_food(colony, loose.map(|drop| drop.mass), mass)
    {
        return Some(mass);
    }
    // Only a distant complete intact serving justifies suppressing a local split.
    loose
        .filter(|drop| allow_bulk_split || drop.whole)
        .map(|drop| drop.mass)
}
