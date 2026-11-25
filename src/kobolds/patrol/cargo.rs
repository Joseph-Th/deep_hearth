//! One conserved, interruptible scout cargo exchange across a real patrol edge.

use super::*;
use crate::kobolds::scavenging::preferred_salvage_split;

#[derive(Clone, Copy)]
pub(super) struct CargoDecision {
    pub(super) from: VoxelCoord,
    pub(super) to: VoxelCoord,
    pub(super) returning: bool,
    pub(super) frightened: bool,
    pub(super) den_space: Mass,
    pub(super) food_only: bool,
    pub(super) mobilized: bool,
    pub(super) at: SimulationTick,
}

/// The actual loose-food transfer available at a cell, before considering forage.
#[derive(Clone, Copy)]
pub(super) struct LocalFoodDrop {
    pub(super) source: SurfaceResourceId,
    pub(super) mass: Mass,
    pub(super) whole: bool,
}

/// An intact complete ration avoids splitting. If no intact ration suffices,
/// prefer the larger recoverable serving; equal partial servings stay intact.
/// Forecast and actual cargo handling must use the same local decision.
pub(super) fn local_food_drop(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    position: VoxelCoord,
    den_space: Mass,
    at: SimulationTick,
    portable: Option<SurfaceResourceId>,
) -> Option<LocalFoodDrop> {
    let whole = portable
        .and_then(|id| state.surface().get(id))
        .filter(|drop| registries.survival().get_food(drop.commodity()).is_some())
        .map(|drop| LocalFoodDrop {
            source: drop.id(),
            mass: drop.remaining_mass(),
            whole: true,
        });
    let shortage = super::super::SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted scout's credited partial ration is below one full meal");
    if whole.is_some_and(|drop| drop.mass >= shortage) {
        return whole;
    }
    let bulk = local_split_source(
        registries,
        state,
        position,
        den_space,
        at,
        true,
        colony.mobilized_at.is_some(),
    )
    .and_then(|id| state.surface().get(id))
    .map(|drop| LocalFoodDrop {
        source: drop.id(),
        mass: drop
            .remaining_mass()
            .min(colony.salvage_batch())
            .min(den_space),
        whole: false,
    });
    if bulk.is_some_and(|drop| whole.is_none_or(|intact| drop.mass > intact.mass)) {
        bulk
    } else {
        whole
    }
}

/// Foraging costs a recovery interval. Spend it instead of handling a loose
/// parcel only when the loose parcel cannot finish the meal and forage yields more.
pub(super) fn forage_beats_loose_food(
    colony: &KoboldColony,
    loose: Option<Mass>,
    forage: Mass,
) -> bool {
    let shortage = super::super::SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted scout's credited partial ration is below one full meal");
    loose.is_none_or(|mass| mass < shortage && forage > mass)
}

struct ScoutCargoPlan {
    carried: Option<SurfaceResourceId>,
    blocked_at_den: bool,
    candidate: Option<SurfaceResourceId>,
    preferred_split: Option<SurfaceResourceId>,
    prefer_forage: bool,
}

/// Existing carried matter must be checked for identity and location before
/// selecting new loot. Capacity limits the last den edge, not intermediate
/// carriage: a newly filled den cannot cause a parcel to drop miles away.
fn plan_returning_cargo(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    choice: CargoDecision,
    forage: Option<Mass>,
) -> ScoutCargoPlan {
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
            state.surface().get(id).is_some_and(|resource| {
                registries
                    .survival()
                    .get_food(resource.commodity())
                    .is_none()
            })
        });
    let mut plan = ScoutCargoPlan {
        carried,
        blocked_at_den,
        candidate: None,
        preferred_split: None,
        prefer_forage: false,
    };
    if !choice.returning || choice.frightened {
        return plan;
    }
    if let Some(id) = carried
        && !switching_from_inert
    {
        plan.candidate = (!blocked_at_den).then_some(id);
        return plan;
    }
    let portable = local_portable_source(
        registries,
        state,
        choice.from,
        choice.den_space,
        choice.at,
        choice.food_only,
        choice.mobilized,
    );
    plan.candidate = portable;
    if !choice.food_only {
        if choice.mobilized {
            plan.preferred_split = preferred_salvage_split(
                registries,
                state,
                choice.from,
                choice.den_space,
                choice.at,
                portable,
            );
        }
        return plan;
    }
    let loose = local_food_drop(
        registries,
        state,
        colony,
        choice.from,
        choice.den_space,
        choice.at,
        portable,
    );
    plan.preferred_split = loose.filter(|drop| !drop.whole).map(|drop| drop.source);
    plan.prefer_forage = colony.hungry
        && forage
            .is_some_and(|mass| forage_beats_loose_food(colony, loose.map(|drop| drop.mass), mass));
    plan
}

fn scout_releases_cargo(frightened: bool, den_full: bool, can_move: bool) -> bool {
    frightened || (den_full && can_move)
}

/// The same canonical splitting operation must account for a tool's actual
/// abrasion, or the scout uses its original bare-hand batch instead.
struct ScoutSplitRequest {
    source: SurfaceResourceId,
    from: VoxelCoord,
    to: VoxelCoord,
    at: SimulationTick,
    bare_batch: Mass,
    den_space: Mass,
}

fn split_scavenged_bulk_with_available_tool(
    registries: &Registries,
    state: &mut AppState,
    request: ScoutSplitRequest,
) -> Option<(SurfaceResourceId, Mass)> {
    let ScoutSplitRequest {
        source,
        from,
        to,
        at,
        bare_batch,
        den_space,
    } = request;
    let tool = state.kobold_state().colony.as_ref().and_then(|colony| {
        (from == colony.route[1])
            .then(|| current_junction_scrap_lever(state, colony))
            .flatten()
    });
    if let Some(tool) = tool
        && let Some(split) = split_bulk_scavenged_drop_with_head_wear(
            registries,
            state,
            ScavengedPortionRequest::new(
                source,
                from,
                to,
                at,
                bare_batch.max(LEVER_SPLIT_BATCH).min(den_space),
            ),
            tool.head(),
            tool.kind().abrasion_per_split(),
            tool.kind().abrasion_form(),
        )
    {
        return Some(split);
    }
    // Tool assistance may fail when its second output (recoverable abrasion)
    // cannot be admitted. That failed atomic attempt leaves the source intact,
    // so ordinary hand splitting remains a legitimate smaller transaction.
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

fn relocated_part_stays_at_junction(
    state: &AppState,
    relocated: Option<SurfaceResourceId>,
) -> bool {
    relocated
        .and_then(|id| state.surface().get(id))
        .is_some_and(|resource| {
            is_staged_scrap_tool_part(state, resource)
                || crate::kobolds::junkworks::is_staged_relay_component(state, resource)
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
struct ScoutCargoExchange {
    from: VoxelCoord,
    to: VoxelCoord,
    carried: Option<SurfaceResourceId>,
    moved: Option<SurfaceResourceId>,
    can_keep_carried: bool,
    new_parcel: Option<SurfaceResourceId>,
}

fn finish_return_cargo_exchange(
    registries: &Registries,
    state: &mut AppState,
    exchange: ScoutCargoExchange,
) -> (Option<SurfaceResourceId>, bool) {
    let ScoutCargoExchange {
        from,
        to,
        carried,
        moved,
        can_keep_carried,
        new_parcel,
    } = exchange;
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

/// One physically ordered return-edge transfer. Observe external drops before moving the
/// scout's existing parcel, and commit at most one successful cargo exchange.
#[derive(Clone, Copy)]
pub(super) struct ScoutReturnCargoRequest {
    pub(super) choice: CargoDecision,
    pub(super) physically_clear: bool,
    pub(super) may_forage_on_return: bool,
    pub(super) forage_ready: bool,
    pub(super) forage_batch: Mass,
    pub(super) salvage_batch: Mass,
    pub(super) ignored_external: [Option<SurfaceResourceId>; 2],
}

#[must_use]
pub(super) struct ScoutReturnCargoOutcome {
    pub(super) can_move: bool,
    pub(super) discovered: bool,
    pub(super) relocated_drop: Option<SurfaceResourceId>,
    pub(super) split_drop: Option<(SurfaceResourceId, Mass)>,
    pub(super) foraged_food: Option<(SurfaceResourceId, Mass)>,
    pub(super) staged_tool_part: bool,
    pub(super) abandoned_drop: Option<SurfaceResourceId>,
}

pub(super) fn execute_scout_return_cargo(
    registries: &Registries,
    state: &mut AppState,
    request: ScoutReturnCargoRequest,
) -> ScoutReturnCargoOutcome {
    let ScoutReturnCargoRequest {
        choice:
            CargoDecision {
                from,
                to,
                returning,
                frightened,
                den_space,
                food_only,
                mobilized,
                at,
            },
        physically_clear,
        may_forage_on_return,
        forage_ready,
        forage_batch,
        salvage_batch,
        ignored_external,
    } = request;
    let forage =
        (physically_clear && returning && may_forage_on_return && !frightened && forage_ready)
            .then(|| best_local_forage(registries, state, from, at, forage_batch.min(den_space)))
            .flatten();
    let colony = state
        .kobold_state()
        .colony
        .as_ref()
        .expect("scout cargo requires a colony");
    let ScoutCargoPlan {
        carried: carried_drop,
        blocked_at_den: cargo_blocked_at_den,
        candidate: dragged_candidate,
        preferred_split,
        prefer_forage,
    } = plan_returning_cargo(
        registries,
        state,
        colony,
        request.choice,
        forage.map(|(mass, _)| mass),
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
    // Observe the destination before any cargo moves.
    let discovered =
        can_move && finds_external_drop(registries, state, to, den_space, at, ignored_external);
    let try_forage = |state: &mut AppState| {
        forage.and_then(|(_, source)| {
            forage_food_to_adjacent_den(
                registries,
                state,
                source,
                from,
                to,
                at,
                forage_batch.min(den_space),
            )
        })
    };
    // An unsuccessful preferred harvest changes no custody. The original
    // whole/split/fallback order remains available, including old cargo.
    let first_forage = if can_move && prefer_forage {
        try_forage(state)
    } else {
        None
    };
    let try_split = |state: &mut AppState, source| {
        split_scavenged_bulk_with_available_tool(
            registries,
            state,
            ScoutSplitRequest {
                source,
                from,
                to,
                at,
                bare_batch: salvage_batch,
                den_space,
            },
        )
    };
    // Try the preferred split before the intact fallback, but do not discard
    // the fallback if the atomic split cannot be committed.
    let first_split = if can_move && first_forage.is_none() {
        preferred_split.and_then(|source| try_split(state, source))
    } else {
        None
    };
    let first_relocated = if can_move && first_forage.is_none() && first_split.is_none() {
        dragged_candidate
            .filter(|id| relocate_scavenged_drop_step(registries, state, *id, from, to))
    } else {
        None
    };
    let split_drop = first_split.or_else(|| {
        if can_move
            && returning
            && !frightened
            && first_forage.is_none()
            && first_relocated.is_none()
            && preferred_split.is_none()
        {
            local_split_source(registries, state, from, den_space, at, food_only, mobilized)
                .and_then(|source| try_split(state, source))
        } else {
            None
        }
    });
    let foraged_food = first_forage.or_else(|| {
        if can_move && !prefer_forage && first_relocated.is_none() && split_drop.is_none() {
            try_forage(state)
        } else {
            None
        }
    });
    // Failed food exchanges must not abandon already-held cargo.
    let (relocated_drop, switched_cargo) = finish_return_cargo_exchange(
        registries,
        state,
        ScoutCargoExchange {
            from,
            to,
            carried: carried_drop,
            moved: first_relocated,
            can_keep_carried: can_move && continuing_carried_custody,
            new_parcel: split_drop
                .map(|(id, _)| id)
                .or(foraged_food.map(|(id, _)| id)),
        },
    );
    let staged_tool_part = relocated_part_stays_at_junction(state, relocated_drop);
    let abandoned_drop = (scout_releases_cargo(frightened, cargo_blocked_at_den, can_move)
        || switched_cargo)
        .then_some(carried_drop)
        .flatten();
    ScoutReturnCargoOutcome {
        can_move,
        discovered,
        relocated_drop,
        split_drop,
        foraged_food,
        staged_tool_part,
        abandoned_drop,
    }
}

#[cfg(test)]
#[path = "cargo_tests.rs"]
mod tests;
