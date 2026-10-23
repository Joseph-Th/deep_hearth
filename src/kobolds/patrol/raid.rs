//! Read-only planning of the scout's local interest and bounded side raids.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::super::feeding::scout_wants_food_opportunity;
use super::super::junkworks::{den_or_relay_needs_striker, is_junk_striker};
use super::super::scavenging::{
    best_local_forage, has_local_scout_interest, is_portable_drop, local_portable_source,
    matches_salvage_goal, reachable_detour_drop, reachable_detour_food,
    reachable_inherited_salvage, scout_wants_loose_drop,
};
use super::super::scrap_tools::{is_needed_outer_tool_part, junction_awaits_better_head};
use super::super::visibility::scout_visible_from;
use super::super::{KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_MASS};
use super::{scout_can_enter, scout_sees_guarded_cell};

/// Determine whether a den's expelled inert parcel should be ignored in
/// favor of a reachable piece of inherited machinery. The indexed target is
/// then reused by movement planning, avoiding repeated two-edge searches.
pub(super) fn scout_raid_interest(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    from: VoxelCoord,
    capacity: Mass,
    forage_ready: bool,
    at: SimulationTick,
    player_position: Option<VoxelCoord>,
    pressured: bool,
) -> (bool, Option<(VoxelCoord, Option<VoxelCoord>)>) {
    let inherited_target = (colony.mobilized_at.is_some()
        && !pressured
        && colony.scout_step == 1
        && !colony.returning)
        .then(|| {
            let [den, junction, tip] = colony.route;
            reachable_inherited_salvage(
                registries,
                state,
                junction,
                from,
                den,
                tip,
                player_position,
                capacity,
                at,
            )
        })
        .flatten();
    let local_food = colony.hungry
        && has_local_scout_interest(registries, state, from, capacity, forage_ready, at, true);
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
    let missing_tool_at_tip = !pressured
        && colony.scout_step == 1
        && !colony.returning
        && reachable_needed_tool_at_tip(
            registries,
            state,
            colony,
            from,
            capacity,
            at,
            player_position,
        );
    let exposed = (has_local_scout_interest(
        registries,
        state,
        from,
        capacity,
        forage_ready,
        at,
        colony.seeks_food_at_junction(),
    ) || (colony.scout_step == 2 && local_food))
        && ((inherited_target.is_none() && !missing_striker_at_tip && !missing_tool_at_tip)
            || local_food)
        && !continue_to_planned_far_raid(
            registries,
            state,
            colony,
            from,
            capacity,
            forage_ready,
            at,
            player_position,
        );
    (exposed, inherited_target)
}

/// Repair priority requires a visible, reachable physical part on the patrol.
fn reachable_needed_tool_at_tip(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    from: VoxelCoord,
    capacity: Mass,
    at: SimulationTick,
    player_position: Option<VoxelCoord>,
) -> bool {
    if !junction_awaits_better_head(state, colony) {
        return false;
    }
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
    from: VoxelCoord,
    capacity: Mass,
    forage_ready: bool,
    at: SimulationTick,
    player_position: Option<VoxelCoord>,
) -> bool {
    if colony.scout_step != 2 || colony.returning {
        return false;
    }
    let Some(goal) = colony.detour_goal else {
        return false;
    };
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
    step: u8,
    returning: bool,
    player_position: Option<VoxelCoord>,
    den_space: Mass,
    forage_ready: bool,
    at: SimulationTick,
    inherited_target: Option<(VoxelCoord, Option<VoxelCoord>)>,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    if step != 2 || returning || colony.scout_step != 1 {
        return None;
    }
    let [den, junction, tip] = colony.route;
    let loose_cache = |food_only| {
        reachable_detour_drop(
            registries,
            state,
            junction,
            den,
            tip,
            player_position,
            den_space,
            at,
            food_only,
        )
    };
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
                reachable_detour_food(
                    registries,
                    state,
                    junction,
                    den,
                    tip,
                    player_position,
                    at,
                    maximum_mass,
                    shortage,
                )
            })
            .flatten()
    };
    let inherited_salvage = || inherited_target;
    if colony.hungry {
        // Food first, then the remembered machine-service salvage priority.
        // Ordinary nearby scraps remain available when neither is reachable.
        loose_cache(true)
            .or_else(wild_food)
            .or_else(inherited_salvage)
            .or_else(|| loose_cache(false))
    } else {
        inherited_salvage()
            .or_else(|| loose_cache(false))
            .or_else(wild_food)
    }
}
