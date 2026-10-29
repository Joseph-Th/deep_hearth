//! One conserved, interruptible scout cargo exchange across a real patrol edge.

use super::*;

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

/// Existing carried matter must be checked for identity and location before
/// selecting new loot. Capacity limits the last den edge, not intermediate
/// carriage: a newly filled den cannot cause a parcel to drop miles away.
pub(super) fn plan_returning_cargo(
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
/// scout's existing parcel; later splits/forage must see the committed earlier custody
/// changes, not an independent pre-movement snapshot.
#[derive(Clone, Copy)]
pub(super) struct ScoutReturnCargoRequest {
    pub(super) choice: CargoDecision,
    pub(super) physically_clear: bool,
    pub(super) carried_drop: Option<SurfaceResourceId>,
    pub(super) cargo_blocked_at_den: bool,
    pub(super) dragged_candidate: Option<SurfaceResourceId>,
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
        carried_drop,
        cargo_blocked_at_den,
        dragged_candidate,
        may_forage_on_return,
        forage_ready,
        forage_batch,
        salvage_batch,
        ignored_external,
    } = request;
    let (can_move, continuing_carried_custody) = scout_cargo_movement_allowed(
        state,
        physically_clear,
        returning,
        frightened,
        cargo_blocked_at_den,
        carried_drop,
        dragged_candidate,
    );
    // Do not mistake the parcel dragged into the destination for new player loot.
    let discovered =
        can_move && finds_external_drop(registries, state, to, den_space, at, ignored_external);
    let first_relocated = if can_move {
        dragged_candidate
            .filter(|id| relocate_scavenged_drop_step(registries, state, *id, from, to))
    } else {
        None
    };
    // Whole parcels retain identity; bulky piles yield one conserved daughter parcel.
    let bulk_source = (can_move && returning && !frightened && first_relocated.is_none())
        .then(|| local_split_source(registries, state, from, den_space, at, food_only, mobilized))
        .flatten();
    let split_drop = bulk_source.and_then(|source| {
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
    });
    // Forage only if the whole/split drop transactions left the return edge unused.
    let forage_source = (can_move
        && returning
        && may_forage_on_return
        && first_relocated.is_none()
        && split_drop.is_none()
        && !frightened
        && forage_ready)
        .then(|| {
            best_local_forage(registries, state, from, at, forage_batch.min(den_space))
                .map(|(_, id)| id)
        })
        .flatten();
    let foraged_food = forage_source.and_then(|source| {
        forage_food_to_adjacent_den(
            registries,
            state,
            source,
            from,
            to,
            at,
            forage_batch.min(den_space),
        )
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
