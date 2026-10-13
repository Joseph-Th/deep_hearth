//! Ordinary player planning, material commitment, labor, and activation for grounded members.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::MaterialLotSelection;
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, StructuralConstructionMethodId,
    StructuralConstructionWork, ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::construction_execution::ValidatedStructuralConstruction;
use super::{
    StructuralConstructionBindingError, StructuralConstructionCommitError,
    StructuralConstructionError, StructuralElementId, StructuralLifecycle,
    ValidatedStructuralMutation, bind_structural_construction_selection,
    resolve_structural_material_requirement, validate_activate_structural_element_in,
    validate_structural_construction,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralConstructionError {
    PlayerNotAdmitted,
    UnknownMethod {
        method: StructuralConstructionMethodId,
    },
    UnknownElement {
        element: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
    },
    ElementNotGrounded {
        element: StructuralElementId,
    },
    ElementNotLocal {
        element: StructuralElementId,
    },
    MemberTooHeavy {
        required: Mass,
        maximum: Mass,
    },
    CompletionTickOverflow,
    UnexpectedMaterialSelection,
    MaterialStateChanged {
        element: StructuralElementId,
    },
    StructureRevisionExhausted,
    Binding(StructuralConstructionBindingError),
    Material(StructuralConstructionError),
    Work(PlayerWorkStartError),
}

impl Display for PlayerStructuralConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "player structural construction failed: {self:?}")
    }
}

impl Error for PlayerStructuralConstructionError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralConstructionCommitError {
    StaleLogistics {
        expected: u64,
        actual: u64,
    },
    PlayerMoved {
        expected: VoxelCoord,
        actual: Option<VoxelCoord>,
    },
    Material(StructuralConstructionCommitError),
    Work(PlayerWorkCommitError),
}

impl Display for PlayerStructuralConstructionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "validated player construction changed before commit: {self:?}"
        )
    }
}

impl Error for PlayerStructuralConstructionCommitError {}

#[must_use]
pub struct ValidatedPlayerStructuralConstructionStart {
    expected_logistics_revision: u64,
    expected_player_position: VoxelCoord,
    material: Option<ValidatedStructuralConstruction>,
    work_start: ValidatedPlayerWorkStart,
    work: StructuralConstructionWork,
}

impl ValidatedPlayerStructuralConstructionStart {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.work.element()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralConstructionWork, PlayerStructuralConstructionCommitError> {
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(PlayerStructuralConstructionCommitError::StaleLogistics {
                expected: self.expected_logistics_revision,
                actual: state.logistics().revision(),
            });
        }
        let actual_position = state.logistics().player().map(|player| player.position());
        if actual_position != Some(self.expected_player_position) {
            return Err(PlayerStructuralConstructionCommitError::PlayerMoved {
                expected: self.expected_player_position,
                actual: actual_position,
            });
        }
        if let Some(material) = &self.material {
            material
                .precheck(state)
                .map_err(PlayerStructuralConstructionCommitError::Material)?;
        }
        self.work_start
            .precheck(state)
            .map_err(PlayerStructuralConstructionCommitError::Work)?;
        if let Some(material) = self.material {
            material.apply(state);
        }
        self.work_start.apply(state);
        Ok(self.work)
    }
}

pub(crate) fn bounds_are_local(bounds: crate::spatial::VoxelBounds, player: VoxelCoord) -> bool {
    bounds.voxel_count() == Some(1)
        && !bounds.has_voxel(player)
        && [
            player
                .x()
                .checked_sub(1)
                .map(|x| VoxelCoord::new(x, player.y(), player.z())),
            player
                .x()
                .checked_add(1)
                .map(|x| VoxelCoord::new(x, player.y(), player.z())),
            player
                .y()
                .checked_sub(1)
                .map(|y| VoxelCoord::new(player.x(), y, player.z())),
            player
                .y()
                .checked_add(1)
                .map(|y| VoxelCoord::new(player.x(), y, player.z())),
            player
                .z()
                .checked_sub(1)
                .map(|z| VoxelCoord::new(player.x(), player.y(), z)),
            player
                .z()
                .checked_add(1)
                .map(|z| VoxelCoord::new(player.x(), player.y(), z)),
        ]
        .into_iter()
        .flatten()
        .any(|neighbor| bounds.has_voxel(neighbor))
}

pub fn validate_start_player_structural_construction(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    element: StructuralElementId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    let definition = registries
        .labor()
        .get_structural_construction(method)
        .copied()
        .ok_or(PlayerStructuralConstructionError::UnknownMethod { method })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerStructuralConstructionError::PlayerNotAdmitted)?;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(PlayerStructuralConstructionError::UnknownElement { element })?;
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(PlayerStructuralConstructionError::ElementNotPlanned { element });
    }
    if !record.is_grounded() {
        return Err(PlayerStructuralConstructionError::ElementNotGrounded { element });
    }
    if !bounds_are_local(record.bounds(), player.position()) {
        return Err(PlayerStructuralConstructionError::ElementNotLocal { element });
    }
    let requirement = resolve_structural_material_requirement(registries, state, element)
        .map_err(|_| PlayerStructuralConstructionError::MaterialStateChanged { element })?;
    let required = requirement.required_mass();
    let duration = definition.duration_for_mass(required).ok_or(
        PlayerStructuralConstructionError::MemberTooHeavy {
            required,
            maximum: definition.maximum_member_mass(),
        },
    )?;
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(PlayerStructuralConstructionError::CompletionTickOverflow)?;

    let material = if record.embodied_mass().is_zero() {
        if !state.can_spend_structure_revisions(2) {
            return Err(PlayerStructuralConstructionError::StructureRevisionExhausted);
        }
        let resolution = bind_structural_construction_selection(
            state,
            element,
            player.carried_stockpile(),
            selections,
        )
        .map_err(PlayerStructuralConstructionError::Binding)?;
        Some(
            validate_structural_construction(registries, state, resolution)
                .map_err(PlayerStructuralConstructionError::Material)?,
        )
    } else {
        if record.embodied_mass() != required {
            return Err(PlayerStructuralConstructionError::MaterialStateChanged { element });
        }
        if !selections.is_empty() {
            return Err(PlayerStructuralConstructionError::UnexpectedMaterialSelection);
        }
        if !state.can_spend_structure_revisions(1) {
            return Err(PlayerStructuralConstructionError::StructureRevisionExhausted);
        }
        None
    };

    let work = StructuralConstructionWork::new(element, method, state.tick(), completes_at);
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::StructuralConstruction { work },
        duration,
        definition.exertion(),
    )
    .map_err(PlayerStructuralConstructionError::Work)?;
    Ok(ValidatedPlayerStructuralConstructionStart {
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: player.position(),
        material,
        work_start,
        work,
    })
}

#[must_use]
pub(crate) struct StructuralConstructionTickPlan {
    element: StructuralElementId,
    activation: ValidatedStructuralMutation,
}

pub(crate) fn decide_structural_construction_tick(
    registries: &Registries,
    state: &AppState,
    projected_structures: &crate::structural::StructureState,
    next_tick: SimulationTick,
) -> Option<StructuralConstructionTickPlan> {
    let work = state
        .player_work()
        .structural_construction_due_at(next_tick)?;
    let activation =
        validate_activate_structural_element_in(registries, projected_structures, work.element())
            .unwrap_or_else(|error| {
                panic!("admitted structural construction became invalid: {error}")
            });
    Some(StructuralConstructionTickPlan {
        element: work.element(),
        activation,
    })
}

pub(crate) fn apply_structural_construction_tick(
    state: &mut AppState,
    plan: Option<StructuralConstructionTickPlan>,
) -> Option<StructuralElementId> {
    let plan = plan?;
    let _ = plan
        .activation
        .commit(state)
        .unwrap_or_else(|error| panic!("prechecked structural activation failed: {error}"));
    Some(plan.element)
}

#[cfg(test)]
#[path = "player_construction_tests.rs"]
mod tests;
