//! Atomic ordinary-player composition of grounded planning, carried matter, and hand-setting work.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::inventory::{MaterialStackProjection, MaterialStackProjectionError};
use crate::labor::{StructuralConstructionMethodId, StructuralConstructionWork};
use crate::registry::Registries;

use super::{
    GroundedStructuralElementPlan, PlanPlayerStructuralElementCommitError,
    PlanPlayerStructuralElementError, PlayerStructuralConstructionCommitError,
    PlayerStructuralConstructionError, StructuralElementId,
    ValidatedPlayerStructuralConstructionStart, ValidatedPlayerStructuralElementPlan,
    resolve_required_mass, validate_plan_player_grounded_structural_element,
    validate_start_player_structural_construction_for_planned_member,
};

/// Failure while composing one visible carried stack into a new grounded member and timed work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerGroundedStructuralConstructionError {
    Plan(PlanPlayerStructuralElementError),
    Stack(MaterialStackProjectionError),
    Construction(PlayerStructuralConstructionError),
}

impl Display for PlayerGroundedStructuralConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "grounded member planning failed: {error}"),
            Self::Stack(error) => {
                write!(formatter, "grounded member material stack failed: {error}")
            }
            Self::Construction(error) => {
                write!(
                    formatter,
                    "grounded member construction start failed: {error}"
                )
            }
        }
    }
}

impl Error for PlayerGroundedStructuralConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::Stack(error) => Some(error),
            Self::Construction(error) => Some(error),
        }
    }
}

/// Failure when the atomic grounded-member action becomes stale before commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerGroundedStructuralConstructionCommitError {
    Plan(PlanPlayerStructuralElementCommitError),
    Construction(PlayerStructuralConstructionCommitError),
}

impl Display for PlayerGroundedStructuralConstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "grounded member plan became stale: {error}"),
            Self::Construction(error) => {
                write!(
                    formatter,
                    "grounded member construction became stale: {error}"
                )
            }
        }
    }
}

impl Error for PlayerGroundedStructuralConstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::Construction(error) => Some(error),
        }
    }
}

/// One revision-bound action that creates an inert grounded member, embodies exact carried matter,
/// and starts its authored hand-setting interval without exposing an orphan blueprint state.
#[must_use]
pub struct ValidatedPlayerGroundedStructuralConstructionStart {
    plan: ValidatedPlayerStructuralElementPlan,
    construction: ValidatedPlayerStructuralConstructionStart,
}

impl ValidatedPlayerGroundedStructuralConstructionStart {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.plan.element()
    }

    /// Commits only after both halves have been prechecked against the same pre-plan snapshot.
    /// After the first mutation, the remaining applications are infallible consequences of those
    /// checks, so a rejected action cannot leave a planned member behind.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralConstructionWork, PlayerGroundedStructuralConstructionCommitError> {
        self.plan
            .precheck(state)
            .map_err(PlayerGroundedStructuralConstructionCommitError::Plan)?;
        self.construction
            .precheck_before_planned_member(
                state,
                self.plan.record(),
                self.plan.next_structure_revision(),
            )
            .map_err(PlayerGroundedStructuralConstructionCommitError::Construction)?;
        let _ = self.plan.apply(state);
        Ok(self.construction.apply(state))
    }
}

/// Atomically starts one grounded structural member from a visible carried material stack.
///
/// The caller chooses profile/material/geometry and one revision-bound projected stack. Exact lot
/// identity stays private: the structural owner derives the member's geometry-backed mass, inventory
/// resolves that mass inside the chosen stack, and the existing construction owner retains locality,
/// exertion, attention, material-state, conservation, and activation semantics.
pub fn validate_start_player_grounded_structural_construction_from_stack(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    plan: GroundedStructuralElementPlan,
    stack: &MaterialStackProjection,
) -> Result<
    ValidatedPlayerGroundedStructuralConstructionStart,
    PlayerGroundedStructuralConstructionError,
> {
    let validated_plan = validate_plan_player_grounded_structural_element(registries, state, plan)
        .map_err(PlayerGroundedStructuralConstructionError::Plan)?;
    let record = validated_plan.record();
    let required = resolve_required_mass(registries.materials(), record).map_err(|_| {
        PlayerGroundedStructuralConstructionError::Construction(
            PlayerStructuralConstructionError::MaterialStateChanged {
                element: record.id(),
            },
        )
    })?;
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| unreachable!("validated grounded player plan has admitted logistics"));
    let selections = stack
        .selections_for_mass(state, carried, required)
        .map_err(PlayerGroundedStructuralConstructionError::Stack)?;
    let construction = validate_start_player_structural_construction_for_planned_member(
        registries,
        state,
        method,
        record,
        validated_plan.next_structure_revision(),
        &selections,
    )
    .map_err(PlayerGroundedStructuralConstructionError::Construction)?;
    Ok(ValidatedPlayerGroundedStructuralConstructionStart {
        plan: validated_plan,
        construction,
    })
}

#[cfg(test)]
#[path = "player_placement_tests.rs"]
mod tests;
