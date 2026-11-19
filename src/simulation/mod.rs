//! Canonical control-rate simulation stepping; ordered slow-tick settlement lives in `slow_tick`.

mod control;
mod direct_actions;
mod error;
mod fast_forward;
mod outcome;
mod planning;
mod player_death;
mod slow_tick;

pub use control::{PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent};
pub use error::TickError;
pub use outcome::{
    ActiveSelectionDropOutcome, ControlTickOutcome, PrimaryActionOutcome,
    QuickAccessSelectionOutcome, SecondaryActionOutcome, TickOutcome,
};

/// Authorize one terrain-only cut at the current aim, or return no plan if a
/// tree or structural target takes priority. No state changes during admission.
pub fn validate_pointed_terrain_excavation(
    registries: &crate::registry::Registries,
    state: &crate::core::state::AppState,
    direction: crate::spatial::ViewDirection,
) -> Result<
    Option<crate::world::ValidatedPlayerBlockExcavationStart>,
    crate::world::BlockExcavationError,
> {
    direct_actions::validate_pointed_terrain_excavation(registries, state, direction)
}

/// Validate exact destination placement of a supported timber post or roof
/// panel using a physical carried stack and the normal visible-air reach ray.
pub fn validate_start_player_supported_timber_at(
    registries: &crate::registry::Registries,
    state: &crate::core::state::AppState,
    stack: &crate::inventory::MaterialStackProjection,
    position: crate::spatial::VoxelCoord,
) -> Result<
    crate::structural::ValidatedPlayerSupportedStructuralConstructionStart,
    SecondaryActionOutcome,
> {
    direct_actions::validate_supported_timber_at(registries, state, stack, position)
}

/// Preview the physical destination of a pointed timber cover panel, without
/// committing work or materials. Uses the placement owner's target resolution.
pub fn project_pointed_timber_cover_target(
    registries: &crate::registry::Registries,
    state: &crate::core::state::AppState,
    direction: crate::spatial::ViewDirection,
) -> Result<Option<crate::spatial::VoxelCoord>, SecondaryActionOutcome> {
    direct_actions::project_pointed_timber_cover_target(registries, state, direction)
}

/// Preview the structural member selected by a pointed primary break, if any.
/// This uses the action owner's physical ray selection without starting work.
pub fn project_pointed_structural_break_target(
    registries: &crate::registry::Registries,
    state: &crate::core::state::AppState,
    direction: crate::spatial::ViewDirection,
) -> Result<Option<crate::structural::StructuralElementId>, usize> {
    direct_actions::project_pointed_structural_break_target(registries, state, direction)
}

use crate::core::state::{AppState, apply_control_clock_advance, debug_assert_runtime_invariants};
use crate::core::time::{ControlTick, SimulationTick};
use crate::labor::{PlayerWork, apply_player_work_tick, decide_player_work_control_tick};
use crate::logistics::{
    PlayerControlInput, apply_player_body_motion, decide_player_body_motion,
    player_body_uses_generated_world,
};
use crate::registry::Registries;
use crate::surface::{apply_automatic_surface_drop_pickup, decide_automatic_surface_drop_pickup};
use crate::survival::{
    SurvivalExertion, apply_survival_tick, control_exertion_between,
    decide_control_exertion_settlement,
};
use crate::world::{
    BlockExcavationTickError, apply_block_excavation_tick, decide_block_excavation_completion,
};

use direct_actions::{
    attempt_direct_actions, attempt_direct_actions_after_quick_access,
    attempt_pending_primary_break, attempt_pending_secondary_action,
    attempt_quick_access_selection,
};
use fast_forward::control_stepping_required;
use slow_tick::advance_simulation_tick;

/// Advances one canonical 20 Hz direct-control tick.
///
/// Each control tick advances the high-frequency clock and, when the admitted player's footprint
/// is backed by generated chunks, integrates authoritative body gravity/collision. Every 72nd
/// control tick also executes one complete slow-simulation quantum. Future horizontal movement,
/// direct block interaction, and combat phases belong on this surface rather than forcing slow
/// industrial/survival accounting to 20 Hz.
pub fn advance_control_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::NEUTRAL)
}

/// Advances one canonical 20 Hz direct-control tick with the sampled player input for this step.
pub fn advance_control_tick_with_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerControlInput,
) -> Result<ControlTickOutcome, TickError> {
    advance_control_tick_with_direct_input(registries, state, PlayerDirectControlInput::new(input))
}

/// Advances one canonical 20 Hz control tick with locomotion and held primary-action intent.
pub fn advance_control_tick_with_direct_input(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
) -> Result<ControlTickOutcome, TickError> {
    let current = state.control_tick();
    let next_control = current
        .checked_next()
        .ok_or(TickError::ControlClockExhausted { current })?;
    // The slow boundary settles work and survival before accepting the final input.
    // Normally that subsequent body decision is infallible for validated gameplay
    // state. At exhaustion or a mutable work boundary, however, it can reject
    // after the slow owners have committed. Isolate only these exceptional steps
    // so an error cannot leave the caller with a partially advanced simulation.
    if next_control.is_simulation_boundary()
        && boundary_body_motion_needs_transaction(registries, state, input)
    {
        let mut staged = state.snapshot_for_control_transaction();
        let outcome = advance_control_tick_unstaged(registries, &mut staged, input, next_control)?;
        *state = staged;
        return Ok(outcome);
    }
    advance_control_tick_unstaged(registries, state, input, next_control)
}

fn boundary_body_motion_needs_transaction(
    registries: &Registries,
    state: &AppState,
    input: PlayerDirectControlInput,
) -> bool {
    if !player_body_uses_generated_world(state, registries.survival().physiology().locomotion()) {
        return false;
    }
    !state.can_spend_logistics_revisions(1)
        || (state.player_work().active().is_some()
            && input.movement() != PlayerControlInput::NEUTRAL)
        || decide_player_body_motion(registries, state, input.movement(), true).is_err()
}

/// Executes the already-admitted control step; the caller guards exceptional
/// boundary movement with a staging copy before this may commit slow owners.
fn advance_control_tick_unstaged(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
    next_control: ControlTick,
) -> Result<ControlTickOutcome, TickError> {
    let world_revision_before = state.world().revision();
    if next_control.simulation_tick() == state.tick() {
        let body_plan = decide_player_body_motion(registries, state, input.movement(), false)
            .map_err(TickError::PlayerBodyMotion)?;
        let control_excavation = match state.player_work().active() {
            Some(PlayerWork::BlockExcavation { work }) if work.completes_at() == next_control => {
                let elapsed_before = work
                    .elapsed_control_ticks_at_boundary(state.tick())
                    .unwrap_or_else(|| unreachable!("current excavation boundary must be valid"));
                let elapsed_after = work
                    .duration_control_ticks()
                    .unwrap_or_else(|| unreachable!("excavation completion cannot precede start"));
                let method = registries
                    .mining()
                    .get_method(work.method())
                    .unwrap_or_else(|| {
                        panic!("runtime invariant broken: excavation method disappeared")
                    });
                let tail_exertion =
                    control_exertion_between(method.exertion(), elapsed_before, elapsed_after)
                        .unwrap_or_else(|| panic!("excavation control-rate exertion overflowed"));
                let survival_plan =
                    decide_control_exertion_settlement(registries, state, tail_exertion).map_err(
                        |error| match error {
                            crate::survival::SurvivalTickError::RevisionExhausted => {
                                TickError::SurvivalRevisionExhausted
                            }
                            crate::survival::SurvivalTickError::EnergyCostOverflow => {
                                TickError::SurvivalEnergyCostOverflow
                            }
                            crate::survival::SurvivalTickError::HydrationCostOverflow => {
                                TickError::SurvivalHydrationCostOverflow
                            }
                        },
                    )?;
                let excavation_plan = decide_block_excavation_completion(
                    registries,
                    state,
                    state.inventory(),
                    next_control,
                    state.tick(),
                )
                .map_err(map_block_excavation_tick_error)?
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: due control-rate excavation produced no completion plan")
                });
                let player_work_plan = decide_player_work_control_tick(state, next_control)
                    .map_err(|_| TickError::PlayerWorkRevisionExhausted)?;
                Some((excavation_plan, survival_plan, player_work_plan))
            }
            _ => None,
        };
        let body_motion = apply_player_body_motion(state, body_plan);
        let block_excavation =
            if let Some((excavation_plan, survival_plan, player_work_plan)) = control_excavation {
                let outcome = apply_block_excavation_tick(registries, state, Some(excavation_plan));
                let _ = apply_survival_tick(state, survival_plan);
                apply_player_work_tick(state, player_work_plan);
                outcome
            } else {
                None
            };
        apply_control_clock_advance(state, next_control);
        let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
            .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
        let direct_actions = attempt_direct_actions(registries, state, input, None, None);
        debug_assert_runtime_invariants(registries, state, world_revision_before);
        return Ok(ControlTickOutcome::new(
            next_control,
            None,
            body_motion,
            surface_drop_pickup,
            block_excavation,
            direct_actions,
        ));
    }
    debug_assert!(next_control.is_simulation_boundary());
    let settlement_plan =
        decide_player_body_motion(registries, state, PlayerControlInput::NEUTRAL, true)
            .map_err(TickError::PlayerBodyMotion)?;
    let locomotion_exertion = settlement_plan
        .as_ref()
        .map_or(SurvivalExertion::REST, |plan| {
            plan.settled_locomotion_exertion()
        });
    let fall_vitality_loss_ppm = settlement_plan
        .as_ref()
        .map_or(0, |plan| plan.settled_fall_vitality_loss_ppm());
    // The slow tick may be fatal. Record buffered intent before the death transition clears it:
    // the outer control step still owes the actor an observable rejection.
    let has_pending_primary = state.logistics().pending_primary_break().is_some();
    let has_pending_secondary = state.logistics().pending_secondary_use().is_some();
    let simulation = advance_simulation_tick(
        registries,
        state,
        next_control.simulation_tick(),
        locomotion_exertion,
        fall_vitality_loss_ppm,
    )?;
    let player_dead = state
        .survival()
        .player()
        .is_some_and(|player| player.vitality() == crate::survival::Vitality::ZERO);
    // Buffered interactions are tied to the physical position at admission. Do not
    // suppress a new movement sample to keep a queued gesture artificially valid:
    // its owner rejects the stale context after the body actually moves.
    let body_plan = decide_player_body_motion(registries, state, input.movement(), true)
        .map_err(TickError::PlayerBodyMotion)?;
    let body_motion = apply_player_body_motion(state, body_plan);
    let surface_drop_pickup = decide_automatic_surface_drop_pickup(registries, state)
        .map(|plan| apply_automatic_surface_drop_pickup(state, plan));
    let quick_access = attempt_quick_access_selection(state, input.quick_access_slot());
    let pending_primary = if has_pending_primary && player_dead {
        (input.primary().is_none() && input.secondary().is_none() && !input.drop_active_selection())
            .then_some(PrimaryActionOutcome::PlayerDead)
    } else if has_pending_primary
        && (input.primary().is_some()
            || input.secondary().is_some()
            || input.drop_active_selection())
    {
        let _ = crate::logistics::take_player_pending_primary_break(state);
        None
    } else {
        attempt_pending_primary_break(registries, state)
    };
    let pending_secondary = if has_pending_secondary && player_dead {
        (input.primary().is_none() && input.secondary().is_none() && !input.drop_active_selection())
            .then_some(SecondaryActionOutcome::PlayerDead)
    } else if has_pending_secondary
        && (input.primary().is_some()
            || input.secondary().is_some()
            || input.drop_active_selection())
    {
        // A new command on either button, or dropping the selection,
        // supersedes its buffered use even on the exact due boundary,
        // before that use can start eating or drinking.
        let _ = crate::logistics::take_player_pending_secondary_use(state);
        None
    } else {
        attempt_pending_secondary_action(registries, state)
    };
    let direct_actions = attempt_direct_actions_after_quick_access(
        registries,
        state,
        input,
        quick_access,
        pending_primary,
        pending_secondary,
    );
    let block_excavation = simulation.block_excavation();
    debug_assert_runtime_invariants(registries, state, world_revision_before);
    debug_assert_eq!(state.control_tick(), next_control);
    Ok(ControlTickOutcome::new(
        next_control,
        Some(simulation),
        body_motion,
        surface_drop_pickup,
        block_excavation,
        direct_actions,
    ))
}

fn map_block_excavation_tick_error(error: BlockExcavationTickError) -> TickError {
    match error {
        BlockExcavationTickError::MaterialLotId => TickError::MaterialLotIdExhausted,
        BlockExcavationTickError::InventoryRevision => TickError::InventoryRevisionExhausted,
        BlockExcavationTickError::SurfaceResourceId => TickError::SurfaceResourceIdExhausted,
        BlockExcavationTickError::SurfaceRevision => TickError::SurfaceRevisionExhausted,
        BlockExcavationTickError::WorldRevision => TickError::WorldRevisionExhausted,
        BlockExcavationTickError::EquipmentRevision => TickError::EquipmentRevisionExhausted,
    }
}

/// Advances atomically to the next slow-simulation boundary when no control-rate consequence can
/// occur during the skipped neutral-control interval.
///
/// Headless/process-only states, players outside generated body truth, and a standing grounded
/// motionless generated-world player with settled locomotion may use this fast path. Buffered
/// input, unsettled fall injury, crouch/air/motion, control-rate excavation, movable temple
/// support, due construction or stored-matter loads on the player's support chain, and local
/// collectible drops still require exact 50 ms stepping and return
/// `TickError::ControlSteppingRequired`.
pub fn advance_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<TickOutcome, TickError> {
    if control_stepping_required(registries, state) {
        return Err(TickError::ControlSteppingRequired {
            current: state.control_tick(),
        });
    }
    let current = state.tick();
    let next_tick = SimulationTick::new(current.value().checked_add(1).ok_or(
        TickError::ControlClockExhausted {
            current: state.control_tick(),
        },
    )?);
    if crate::core::time::ControlTick::from_simulation_tick(next_tick).is_none() {
        return Err(TickError::ControlClockExhausted {
            current: state.control_tick(),
        });
    }
    advance_simulation_tick(registries, state, next_tick, SurvivalExertion::REST, 0)
}

/// Advance neutral input by at most the supplied control-frame budget.
///
/// When the next slow boundary fits the budget, reuse the guarded slow-tick
/// path. Otherwise, or when any control-rate consequence could occur, execute
/// exactly one control frame. Never cross more than one slow boundary, so
/// callers can observe all events and stop at the first changed condition.
pub fn advance_neutral_control_span(
    registries: &Registries,
    state: &mut AppState,
    limit: std::num::NonZeroU64,
) -> Result<ControlTickOutcome, TickError> {
    let scale = crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
    let remaining = scale - state.control_tick().value() % scale;
    if limit.get() > 1 && remaining <= limit.get() {
        match advance_tick(registries, state) {
            Ok(simulation) => {
                let block_excavation = simulation.block_excavation();
                return Ok(ControlTickOutcome::new(
                    state.control_tick(),
                    Some(simulation),
                    None,
                    None,
                    block_excavation,
                    outcome::DirectActionOutcomes::new(None, None, None, None),
                ));
            }
            Err(TickError::ControlSteppingRequired { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    advance_control_tick(registries, state)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
