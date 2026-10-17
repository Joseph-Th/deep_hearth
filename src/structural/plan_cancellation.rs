//! Player-authorized cancellation of empty persisted structural blueprints.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::element_execution::bounds_are_within_player_reach;
use super::state::{StructuralElementId, StructuralLifecycle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelPlayerStructuralPlanError {
    PlayerNotAdmitted,
    PlayerDead,
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    ElementMaterialized {
        element: StructuralElementId,
    },
    ElementHasDependents {
        element: StructuralElementId,
    },
    ElementOutOfReach {
        element: StructuralElementId,
    },
    RevisionExhausted,
}

impl Display for CancelPlayerStructuralPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("structural plan cancellation requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot cancel structural plans"),
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural plan {}", element.value())
            }
            Self::ElementNotPlanned { element, lifecycle } => write!(
                formatter,
                "structural element {} is {:?}, not an uncommitted plan",
                element.value(),
                lifecycle
            ),
            Self::ElementMaterialized { element } => write!(
                formatter,
                "structural plan {} already owns material and requires construction or dismantling",
                element.value()
            ),
            Self::ElementHasDependents { element } => write!(
                formatter,
                "structural plan {} is referenced by dependent members",
                element.value()
            ),
            Self::ElementOutOfReach { element } => write!(
                formatter,
                "structural plan {} is outside player interaction reach",
                element.value()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
        }
    }
}

impl Error for CancelPlayerStructuralPlanError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelPlayerStructuralPlanCommitError {
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
}

impl Display for CancelPlayerStructuralPlanCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "structural plan cancellation became stale: {self:?}"
        )
    }
}

impl Error for CancelPlayerStructuralPlanCommitError {}

/// Revision-bound cancellation of one local, empty planned structural member.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerStructuralPlanCancellation {
    player_action: PlayerActionGuard,
    element: StructuralElementId,
    expected_structure_revision: u64,
    next_structure_revision: u64,
    expected_logistics_revision: u64,
    expected_player_position: VoxelCoord,
}

impl ValidatedPlayerStructuralPlanCancellation {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralElementId, CancelPlayerStructuralPlanCommitError> {
        self.player_action.precheck(state).map_err(|stale| {
            CancelPlayerStructuralPlanCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.structures().revision() != self.expected_structure_revision {
            return Err(
                CancelPlayerStructuralPlanCommitError::StaleStructureRevision {
                    expected: self.expected_structure_revision,
                    actual: state.structures().revision(),
                },
            );
        }
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(
                CancelPlayerStructuralPlanCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: state.logistics().revision(),
                },
            );
        }
        let actual_position = state.logistics().player().map(|player| player.position());
        if actual_position != Some(self.expected_player_position) {
            return Err(CancelPlayerStructuralPlanCommitError::PlayerMoved {
                expected: self.expected_player_position,
                actual: actual_position,
            });
        }
        let structures = state.structure_state_mut();
        structures.remove_element(self.element);
        structures.apply_revision(self.next_structure_revision);
        Ok(self.element)
    }
}

/// Cancels one unmaterialized structural blueprint without creating or deleting matter.
pub fn validate_cancel_player_structural_plan(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<ValidatedPlayerStructuralPlanCancellation, CancelPlayerStructuralPlanError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            CancelPlayerStructuralPlanError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => CancelPlayerStructuralPlanError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(CancelPlayerStructuralPlanError::PlayerNotAdmitted)?;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(CancelPlayerStructuralPlanError::UnknownElement { element })?;
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(CancelPlayerStructuralPlanError::ElementNotPlanned {
            element,
            lifecycle: record.lifecycle(),
        });
    }
    if !record.embodied_mass().is_zero() || !record.embodied_material().is_empty() {
        return Err(CancelPlayerStructuralPlanError::ElementMaterialized { element });
    }
    if state
        .structures()
        .dependents(element)
        .is_some_and(|mut dependents| dependents.next().is_some())
    {
        return Err(CancelPlayerStructuralPlanError::ElementHasDependents { element });
    }
    if !bounds_are_within_player_reach(registries, state, record.bounds()) {
        return Err(CancelPlayerStructuralPlanError::ElementOutOfReach { element });
    }
    if !state.can_spend_structure_revisions(1) {
        return Err(CancelPlayerStructuralPlanError::RevisionExhausted);
    }
    let expected_structure_revision = state.structures().revision();
    let next_structure_revision = expected_structure_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("shared structural headroom was prevalidated"));
    Ok(ValidatedPlayerStructuralPlanCancellation {
        player_action,
        element,
        expected_structure_revision,
        next_structure_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: player.position(),
    })
}
