//! Admission for neutral slow-tick fast-forward without suppressing control-rate consequences.

use std::collections::BTreeSet;

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::labor::PlayerWork;
use crate::logistics::{
    PlayerStance, player_body_floor_structures, player_body_is_grounded,
    player_body_stands_above_structure, player_body_stands_above_temple_caste,
    player_body_uses_generated_world,
};
use crate::registry::Registries;
use crate::structural::StructuralElementId;
use crate::surface::automatic_surface_pickup_requires_control_stepping;
use crate::survival::SurvivalExertion;

/// Reject fast-forward whenever the 20 Hz control path can change the outcome
/// of the next slow boundary, even without a new movement input.
pub(super) fn control_stepping_required(registries: &Registries, state: &AppState) -> bool {
    if state.logistics().has_pending_direct_action()
        || matches!(
            state.player_work().active(),
            Some(PlayerWork::BlockExcavation { .. })
        )
        || automatic_surface_pickup_requires_control_stepping(registries, state)
    {
        return true;
    }
    let locomotion = registries.survival().physiology().locomotion();
    if !player_body_uses_generated_world(state, locomotion) {
        return false;
    }
    let player =
        state.logistics().player().copied().unwrap_or_else(|| {
            unreachable!("generated-world body requires admitted player logistics")
        });
    if let Some(PlayerWork::StructuralDismantling { work }) = state.player_work().active()
        && work.completes_at().value() == state.tick().value().saturating_add(1)
        && player_body_stands_above_structure(registries, state, work.element())
    {
        // The slow phase removes this floor before the boundary's final
        // control sample. Skipping that sample would erase the first fall step.
        return true;
    }
    // A real temple resident is a solid vertical support, but can walk away
    // during the slow phase. The final control sample must then start the
    // player's fall instead of leaving a falsely grounded frozen body.
    if state
        .tick()
        .value()
        .checked_add(1)
        .map(SimulationTick::new)
        .is_some_and(crate::kobolds::temple_resident_step_due)
        && player_body_stands_above_temple_caste(registries, state)
    {
        return true;
    }
    let body = player.body();
    let horizontal = body.horizontal_velocity();
    if player.pending_fall_vitality_loss_ppm() != 0
        || body.stance() != PlayerStance::Standing
        || horizontal.x_nanometers_per_second() != 0
        || horizontal.z_nanometers_per_second() != 0
        || body.vertical_velocity().nanometers_per_second() != 0
        || !player_body_is_grounded(registries, state)
        || player.locomotion_exertion().pending() != SurvivalExertion::REST
    {
        return true;
    }
    due_structural_load_can_change_player_support(registries, state)
}

/// Scheduled construction, processing, or mining can load one structural support
/// and, through shared load-bearing ancestors, fracture a different member beneath
/// the player. Only connected support chains require the final gravity sample.
fn due_structural_load_can_change_player_support(
    registries: &Registries,
    state: &AppState,
) -> bool {
    // Most players stand on terrain. Find real floor members through the
    // spatial index first; no due-job or global structure scan is needed if
    // none can carry the player's body.
    let floor_members = player_body_floor_structures(registries, state);
    if floor_members.is_empty() {
        return false;
    }
    let Some(next_tick) = state.tick().checked_add_executable_span(TickSpan::new(1)) else {
        return false;
    };
    let mut affected_members = BTreeSet::new();
    if collect_due_production_supports(registries, state, next_tick, &mut affected_members).is_err()
    {
        // A failed provider/labor decision cannot authorize eliding the
        // boundary's last gravity sample. Control stepping must decide it.
        return true;
    }
    collect_due_mining_supports(state, next_tick, &mut affected_members);
    if let Some(work) = state
        .player_work()
        .structural_construction_due_at(next_tick)
    {
        affected_members.insert(work.element());
    }
    if affected_members.is_empty() {
        return false;
    }
    // No scheduled load means no fracture of a floor-support chain. On an
    // idle settlement floor, avoid traversing its potentially large support
    // ancestry every slow tick merely to discover there was no due work.
    let mut floor_ancestors = BTreeSet::new();
    for member in floor_members {
        collect_support_ancestors(state, member, &mut floor_ancestors);
    }
    let mut affected_ancestors = BTreeSet::new();
    for member in affected_members {
        collect_support_ancestors(state, member, &mut affected_ancestors);
    }
    !floor_ancestors.is_disjoint(&affected_ancestors)
}

fn collect_due_production_supports(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    supports: &mut BTreeSet<StructuralElementId>,
) -> Result<(), crate::production::CompletionPlanError> {
    let mut due = if state.production().earliest_due_tick() == Some(next_tick) {
        state.production().jobs_due_at(next_tick)
    } else {
        BTreeSet::new()
    };
    // A paused job with one tick remaining may become available and finish
    // on this boundary even though it is absent from the pre-tick due index.
    // Reuse production's canonical availability decision for that case.
    due.extend(crate::production::resumed_jobs_completing_next_tick(
        registries, state, next_tick,
    )?);
    for id in due {
        let job = state
            .production()
            .get_job(id)
            .unwrap_or_else(|| unreachable!("indexed due production job must exist"));
        for stream in job.output_streams() {
            if let Some(support) = state
                .inventory()
                .get_stockpile(stream.destination())
                .and_then(|stockpile| stockpile.supported_by())
            {
                supports.insert(support);
            }
        }
    }
    Ok(())
}

fn collect_due_mining_supports(
    state: &AppState,
    next_tick: SimulationTick,
    supports: &mut BTreeSet<StructuralElementId>,
) {
    for job in state.mining().structurally_settling_jobs_at(next_tick) {
        if let Some(support) = state
            .inventory()
            .get_stockpile(job.destination())
            .and_then(|stockpile| stockpile.supported_by())
        {
            supports.insert(support);
        }
    }
}

fn collect_support_ancestors(
    state: &AppState,
    member: StructuralElementId,
    visited: &mut BTreeSet<StructuralElementId>,
) {
    let mut pending = vec![member];
    while let Some(element) = pending.pop() {
        if !visited.insert(element) {
            continue;
        }
        if let Some(supports) = state.structures().supports(element) {
            pending.extend(supports);
        }
    }
}
