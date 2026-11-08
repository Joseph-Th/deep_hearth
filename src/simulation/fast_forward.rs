//! Admission for neutral slow-tick fast-forward without suppressing control-rate consequences.

use std::collections::BTreeSet;

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::labor::PlayerWork;
use crate::logistics::{
    PlayerStance, player_body_is_grounded, player_body_stands_above_structure,
    player_body_stands_above_temple_caste, player_body_uses_generated_world,
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
    due_stored_matter_can_change_player_support(registries, state)
}

/// A completion can load one structural support and, through shared load-bearing
/// ancestors, fracture a different member beneath the player. Only those connected
/// support chains require replaying the boundary's final gravity sample.
fn due_stored_matter_can_change_player_support(registries: &Registries, state: &AppState) -> bool {
    let Some(next_tick) = state.tick().checked_add_executable_span(TickSpan::new(1)) else {
        return false;
    };
    let mut output_supports = BTreeSet::new();
    collect_due_production_supports(state, next_tick, &mut output_supports);
    collect_due_mining_supports(state, next_tick, &mut output_supports);
    if output_supports.is_empty() {
        return false;
    }
    let mut floor_ancestors = BTreeSet::new();
    for member in state.structures().elements() {
        if player_body_stands_above_structure(registries, state, member.id()) {
            collect_support_ancestors(state, member.id(), &mut floor_ancestors);
        }
    }
    if floor_ancestors.is_empty() {
        return false;
    }
    let mut output_ancestors = BTreeSet::new();
    for support in output_supports {
        collect_support_ancestors(state, support, &mut output_ancestors);
    }
    !floor_ancestors.is_disjoint(&output_ancestors)
}

fn collect_due_production_supports(
    state: &AppState,
    next_tick: SimulationTick,
    supports: &mut BTreeSet<StructuralElementId>,
) {
    if state.production().earliest_due_tick() != Some(next_tick) {
        return;
    }
    for job in state.production().jobs() {
        if job.is_suspended() || job.completes_at() != next_tick {
            continue;
        }
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
}

fn collect_due_mining_supports(
    state: &AppState,
    next_tick: SimulationTick,
    supports: &mut BTreeSet<StructuralElementId>,
) {
    for job in state.mining().jobs() {
        if (job.is_awaiting_settlement() || (job.is_working() && job.completes_at() == next_tick))
            && let Some(support) = state
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
