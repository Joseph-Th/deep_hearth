//! Scout patrol planning and conserved physical scavenging on the slow tick.

mod cargo;
mod raid;
mod tick;

pub(crate) use tick::apply_kobold_scout_tick;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    ScavengedGroundStockpileRequest, ScavengedPortionRequest, SurfaceResourceId,
    SurfaceResourceLifecycle, forage_food_to_adjacent_den, relocate_scavenged_drop_step,
    scavenge_unenclosed_ground_stockpile_material, split_bulk_scavenged_drop_step,
    split_bulk_scavenged_drop_with_head_wear,
};
use crate::survival::Vitality;

use super::feeding::apply_scheduled_den_care;
use super::junkworks::KoboldAlarmPulse;
use super::mobilization::observe_progress;
use super::scavenging::{
    best_local_forage, den_cache_space, finds_external_drop, has_local_scout_interest,
    is_portable_drop, local_portable_source, local_split_source, local_unenclosed_stockpile_target,
    scout_goal_has_supplies, scout_wants_loose_drop, should_seek_local_food,
};
use super::scrap_tools::{
    LEVER_SPLIT_BATCH, apply_pry_fitting_tick, current_junction_scrap_lever,
    is_staged_scrap_tool_part,
};
use super::visibility::scout_visible_from;
use super::watchman::apply_watchman_tick;
use super::{
    KoboldColony, KoboldScoutEvent, KoboldTrailKind, SCOUT_DETERRENT_LIGHT_LEVEL,
    SCOUT_SAFE_SCAVENGE_TEMPERATURE, SCOUT_STEP_TICKS, SCOUT_WARY_INTERVAL_TICKS,
};

use cargo::{
    CargoDecision, ScoutReturnCargoOutcome, ScoutReturnCargoRequest, execute_scout_return_cargo,
    plan_returning_cargo,
};
use raid::{ScoutRaidInterest, choose_outbound_detour, scout_raid_interest};

pub(super) fn scout_can_enter(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    if player_position == Some(position) {
        return false;
    }
    let Some(head_y) = position.y().checked_add(1) else {
        return false;
    };
    // A generation profile is world-wide, but collision authority is local to
    // generated chunks. Generating a distant column must not make an existing
    // opening-graph route cell impassable. Built enclosures and structures are
    // physical in both modes; the graph checks same-node player occupancy.
    let generated_here = state.world().has_generated_terrain_at(position);
    (!generated_here
        || state
            .world()
            .has_standing_clearance(registries.world(), position))
        && [
            position,
            VoxelCoord::new(position.x(), head_y, position.z()),
        ]
        .into_iter()
        .all(|cell| {
            !super::temple_caste_occupies_voxel(registries, state, cell)
                && (!generated_here
                    || !crate::logistics::player_body_occupies_voxel(registries, state, cell))
                // A physical enclosure obstructs passage. Co-located loose
                // equipment and energy/fluid stores are locality markers,
                // not full-cube terrain; illumination owns fire deterrence.
                && !state.logistics().stockpiles_at(cell).any(|id| {
                    state
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

/// One observed patrol decision frame. Reuse these facts across raid selection
/// and movement so their safety, capacity, and timing gates cannot diverge.
#[derive(Clone, Copy)]
struct ScoutPatrolContext {
    from: VoxelCoord,
    den_space: Mass,
    forage_ready: bool,
    at: SimulationTick,
    player_position: Option<VoxelCoord>,
    pressured: bool,
}

/// Movement and retreat planning does not mutate the authoritative colony.
/// An obstructed outbound scout returns by real edges; at the den it waits
/// for a cleared route rather than inventing a bypass.
fn plan_scout_move(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    context: ScoutPatrolContext,
    raid_interest: ScoutRaidInterest,
) -> ScoutMovePlan {
    let ScoutPatrolContext {
        from,
        den_space,
        forage_ready,
        at: next_tick,
        player_position,
        pressured,
    } = context;
    let wary_at_den = colony.scout_step == 0
        && !pressured
        && colony.wary_until.is_some_and(|until| next_tick < until);
    let (proposed_step, proposed_returning) = if wary_at_den {
        (0, false)
    } else {
        colony.next_scout_step(pressured, raid_interest.exposed_at_current)
    };
    let proposed_detour = choose_outbound_detour(
        registries,
        state,
        colony,
        context,
        proposed_step,
        proposed_returning,
        raid_interest,
    );
    let proposed_position = proposed_detour.map_or_else(
        || colony.position_at_step(proposed_step),
        |(first, _)| first,
    );
    let ahead_is_lit = assess_local_illumination(registries, state, proposed_position).level()
        >= SCOUT_DETERRENT_LIGHT_LEVEL
        && (!proposed_returning || scout_visible_from(registries, state, from, proposed_position));
    // A scout can see a guarded destination from the previous route cell.
    // Approaching that clearing should prompt caution, not a free step into
    // the player's reach. This is checked only on the resident's slow clock.
    let ahead_is_guarded = proposed_step != colony.scout_step
        && scout_sees_guarded_cell(registries, state, from, proposed_position);
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
            colony.mobilized_at.is_some(),
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
        physically_clear: !colony.is_staggered(super::KoboldRole::Scout)
            && step != colony.scout_step
            // A calm returning scavenger must not carry food or scrap into a
            // newly bright approach. A scout already in danger may still flee
            // along its real edge instead of freezing beside the light.
            && (pressured || !returning_into_light)
            && (returning || !ahead_is_lit)
            // Immediate flight already has priority over hypothetical next-cell
            // deterrence. Only an unpressured scout can plan to hold short of a
            // guarded homeward edge; a panicked scout still uses its real exit.
            && (pressured
                || !scout_sees_guarded_cell(registries, state, from, position))
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
