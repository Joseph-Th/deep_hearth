//! Primary combat, terrain work, and structural breaking through canonical owner operations.

use crate::combat::{
    validate_cancel_primitive_preparation, validate_prepare_primitive_strike,
    validate_primitive_shove, validate_primitive_strike, validate_release_prepared_strike,
};
use crate::core::state::AppState;
use crate::logistics::{take_player_pending_primary_break, validate_buffer_player_primary_break};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::ViewDirection;
use crate::world::validate_start_player_block_excavation_active;

use super::super::control::PrimaryControlIntent;
use super::super::outcome::PrimaryActionOutcome;
use super::placement::attempt_structural_dismantling_now;
use super::structural_target::{DirectStructuralTargetError, resolve_direct_structural_target};

pub(super) fn attempt_primary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<PrimaryControlIntent>,
) -> Option<PrimaryActionOutcome> {
    let intent = intent?;
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return Some(PrimaryActionOutcome::PlayerNotAdmitted);
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return Some(PrimaryActionOutcome::PlayerDead);
        }
    }
    match intent {
        PrimaryControlIntent::Shove { direction } => Some(
            match validate_primitive_shove(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Shove(validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous primitive shove became stale: {error:?}")
                    }))
                }
                Err(error) => PrimaryActionOutcome::ShoveRejected(error),
            },
        ),
        PrimaryControlIntent::Strike { direction } => Some(
            match validate_primitive_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Strike(validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous primitive weapon contact became stale: {error:?}")
                    }))
                }
                Err(error) => PrimaryActionOutcome::StrikeRejected(error),
            },
        ),
        PrimaryControlIntent::PrepareStrike { direction } => Some(
            match validate_prepare_primitive_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Preparation(validated.commit(state).unwrap_or_else(
                        |error| panic!("synchronous strike preparation became stale: {error:?}"),
                    ))
                }
                Err(error) => PrimaryActionOutcome::PreparationRejected(error),
            },
        ),
        PrimaryControlIntent::ReleaseStrike { direction } => Some(
            match validate_release_prepared_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::PreparedStrike(validated.commit(state).unwrap_or_else(
                        |error| panic!("synchronous prepared strike became stale: {error:?}"),
                    ))
                }
                Err(error) => PrimaryActionOutcome::PreparedStrikeRejected(error),
            },
        ),
        PrimaryControlIntent::CancelStrike => {
            Some(match validate_cancel_primitive_preparation(state) {
                Ok(validated) => PrimaryActionOutcome::Preparation(
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous preparation cancellation became stale: {error:?}")
                    }),
                ),
                Err(error) => PrimaryActionOutcome::PreparationRejected(error),
            })
        }
        PrimaryControlIntent::BreakTarget { direction } => {
            Some(attempt_primary_break(registries, state, direction))
        }
    }
}

pub(in crate::simulation) fn attempt_pending_primary_break(
    registries: &Registries,
    state: &mut AppState,
) -> Option<PrimaryActionOutcome> {
    let pending = take_player_pending_primary_break(state)?;
    let current_position = state
        .logistics()
        .player()
        .map(|player| player.body().foot_position());
    if current_position != Some(pending.position())
        || state.logistics().active_selection() != pending.selection()
    {
        return Some(PrimaryActionOutcome::BufferedContextChanged);
    }
    let direction = pending.direction().unwrap_or_else(|| {
        panic!("runtime invariant broken: trusted buffered structural break has invalid direction")
    });
    match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) if element == pending.element() => Some(attempt_structural_dismantling_now(
            registries, state, element,
        )),
        Ok(_)
        | Err(DirectStructuralTargetError::TargetUnavailable)
        | Err(DirectStructuralTargetError::NoTarget)
        | Err(DirectStructuralTargetError::Ambiguous { .. }) => {
            Some(PrimaryActionOutcome::BufferedContextChanged)
        }
    }
}

fn attempt_primary_break(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> PrimaryActionOutcome {
    if let Ok(Some(root)) =
        crate::world::raycast_player_air_voxel(registries, state, direction, |voxel, _face| {
            state.world().tree_trunk_at(voxel).map(|tree| tree.root())
        })
    {
        // Tree chopping executes immediately rather than using the structural
        // boundary queue. It supersedes a previously buffered break even when
        // the newly selected chopping tool cannot start this different work.
        let _ = take_player_pending_primary_break(state);
        return match crate::world::validate_start_player_tree_felling(registries, state, root) {
            Ok(validated) => {
                PrimaryActionOutcome::TreeFellingStarted(validated.commit(state).unwrap_or_else(
                    |error| panic!("synchronous tree chopping became stale: {error:?}"),
                ))
            }
            Err(error) => PrimaryActionOutcome::TreeFellingRejected(error),
        };
    }
    if let Some(pending) = state.logistics().pending_primary_break() {
        if pending.direction() == Some(direction)
            && pending.selection() == state.logistics().active_selection()
            && state
                .logistics()
                .player()
                .map(|player| player.body().foot_position())
                == Some(pending.position())
        {
            // Held breaking is one scheduled action, not repeated failed admissions.
            // Target/world changes are still revalidated on its due boundary.
            return PrimaryActionOutcome::QueuedUntilSimulationBoundary;
        }
        // A new direction, hand selection or actor location replaces the stale request.
        let _ = take_player_pending_primary_break(state);
    }
    let element = match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) => Some(element),
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return PrimaryActionOutcome::AmbiguousStructuralTarget { count };
        }
        Err(
            DirectStructuralTargetError::TargetUnavailable | DirectStructuralTargetError::NoTarget,
        ) => None,
    };

    if let Some(element) = element {
        if !state.control_tick().is_simulation_boundary() {
            return match validate_buffer_player_primary_break(state, element, direction) {
                Ok(validated) => {
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous structural-break buffering became stale: {error}"
                        )
                    });
                    PrimaryActionOutcome::QueuedUntilSimulationBoundary
                }
                Err(error) => PrimaryActionOutcome::BufferRejected(error),
            };
        }
        return attempt_structural_dismantling_now(registries, state, element);
    }

    match validate_start_player_block_excavation_active(registries, state, direction) {
        Ok(validated) => {
            let started = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous primary block excavation became stale: {error}"
                )
            });
            PrimaryActionOutcome::BlockExcavationStarted(started)
        }
        Err(error) => PrimaryActionOutcome::BlockExcavationRejected(error),
    }
}
