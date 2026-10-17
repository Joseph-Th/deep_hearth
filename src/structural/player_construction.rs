//! Ordinary player planning, material commitment, labor, and activation for grounded members.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::MaterialLotSelection;
use crate::labor::{
    PlayerWork, PlayerWorkCommitError, PlayerWorkStartError, StructuralConstructionDefinition,
    StructuralConstructionMethodId, StructuralConstructionWork, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::construction_execution::ValidatedStructuralConstruction;
use super::{
    StructuralConstructionBindingError, StructuralConstructionCommitError,
    StructuralConstructionError, StructuralElementId, StructuralElementRecord, StructuralLifecycle,
    ValidatedStructuralMutation, bind_structural_construction_selection,
    bounds_are_within_player_reach, resolve_required_mass, validate_activate_structural_element_in,
    validate_structural_construction, validate_structural_construction_for_planned_member,
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
    ElementUnsupported {
        element: StructuralElementId,
    },
    ElementOutOfReach {
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
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("structural construction requires an admitted player"),
            Self::UnknownMethod { method } => write!(formatter, "unknown structural construction method {}", method.value()),
            Self::UnknownElement { element } => write!(formatter, "unknown structural element {}", element.value()),
            Self::ElementNotPlanned { element } => write!(formatter, "structural element {} is not awaiting construction", element.value()),
            Self::ElementUnsupported { element } => write!(formatter, "structural element {} is neither grounded nor attached to an active support", element.value()),
            Self::ElementOutOfReach { element } => write!(formatter, "structural element {} is outside player interaction reach", element.value()),
            Self::MemberTooHeavy { required, maximum } => write!(
                formatter,
                "structural member requires {} mg but the construction method permits at most {} mg",
                required.milligrams(), maximum.milligrams()
            ),
            Self::CompletionTickOverflow => formatter.write_str("structural construction completion tick overflowed"),
            Self::UnexpectedMaterialSelection => formatter.write_str("already-materialized structural construction cannot accept another material selection"),
            Self::MaterialStateChanged { element } => write!(formatter, "structural element {} material state is inconsistent with its geometry", element.value()),
            Self::StructureRevisionExhausted => formatter.write_str("structural revision space is exhausted for construction"),
            Self::Binding(error) => write!(formatter, "structural construction material selection is invalid: {error}"),
            Self::Material(error) => write!(formatter, "structural construction materialization failed: {error}"),
            Self::Work(error) => write!(formatter, "structural construction cannot reserve player work: {error}"),
        }
    }
}

impl Error for PlayerStructuralConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Binding(error) => Some(error),
            Self::Material(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::UnknownMethod { .. }
            | Self::UnknownElement { .. }
            | Self::ElementNotPlanned { .. }
            | Self::ElementUnsupported { .. }
            | Self::ElementOutOfReach { .. }
            | Self::MemberTooHeavy { .. }
            | Self::CompletionTickOverflow
            | Self::UnexpectedMaterialSelection
            | Self::MaterialStateChanged { .. }
            | Self::StructureRevisionExhausted => None,
        }
    }
}

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
        match self {
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "structural construction logistics revision changed from {expected} to {actual} before commit"
            ),
            Self::PlayerMoved {
                expected,
                actual: Some(actual),
            } => write!(
                formatter,
                "player moved from expected construction position ({},{},{}) to ({},{},{}) before commit",
                expected.x(),
                expected.y(),
                expected.z(),
                actual.x(),
                actual.y(),
                actual.z()
            ),
            Self::PlayerMoved {
                expected,
                actual: None,
            } => write!(
                formatter,
                "player disappeared from expected construction position ({},{},{}) before commit",
                expected.x(),
                expected.y(),
                expected.z()
            ),
            Self::Material(error) => write!(
                formatter,
                "structural construction material commit failed: {error}"
            ),
            Self::Work(error) => write!(
                formatter,
                "structural construction player-work commit failed: {error}"
            ),
        }
    }
}

impl Error for PlayerStructuralConstructionCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Material(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::StaleLogistics { .. } | Self::PlayerMoved { .. } => None,
        }
    }
}

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

    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), PlayerStructuralConstructionCommitError> {
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
        Ok(())
    }

    pub(crate) fn precheck_before_planned_member(
        &self,
        state: &AppState,
        record: &StructuralElementRecord,
        projected_structure_revision: u64,
    ) -> Result<(), PlayerStructuralConstructionCommitError> {
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
        let material = self.material.as_ref().unwrap_or_else(|| {
            panic!("atomic planned-member construction must include materialization")
        });
        material
            .precheck_before_planned_member(state, record, projected_structure_revision)
            .map_err(PlayerStructuralConstructionCommitError::Material)?;
        self.work_start
            .precheck(state)
            .map_err(PlayerStructuralConstructionCommitError::Work)?;
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) -> StructuralConstructionWork {
        if let Some(material) = self.material {
            material.apply(state);
        }
        self.work_start.apply(state);
        self.work
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralConstructionWork, PlayerStructuralConstructionCommitError> {
        self.precheck(state)?;
        Ok(self.apply(state))
    }
}

#[derive(Clone, Copy)]
struct PlayerStructuralConstructionContext {
    definition: StructuralConstructionDefinition,
    player_position: VoxelCoord,
    carried: crate::inventory::StockpileId,
    required: Mass,
    duration: crate::core::time::TickSpan,
    completes_at: SimulationTick,
}

fn validate_player_structural_construction_context(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    record: &StructuralElementRecord,
    transient_supports: Option<&[StructuralElementId]>,
) -> Result<PlayerStructuralConstructionContext, PlayerStructuralConstructionError> {
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
    let element = record.id();
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(PlayerStructuralConstructionError::ElementNotPlanned { element });
    }
    let has_active_support = transient_supports.map_or_else(
        || {
            state
                .structures()
                .supports(element)
                .is_some_and(|supports| {
                    supports.into_iter().any(|support| {
                        state
                            .structures()
                            .get_element(support)
                            .is_some_and(|record| record.lifecycle() == StructuralLifecycle::Active)
                    })
                })
        },
        |supports| {
            !supports.is_empty()
                && supports.iter().copied().all(|support| {
                    state
                        .structures()
                        .get_element(support)
                        .is_some_and(|support_record| {
                            support_record.lifecycle() == StructuralLifecycle::Active
                                && record.bounds().has_face_contact(support_record.bounds())
                        })
                })
        },
    );
    if !record.is_grounded() && !has_active_support {
        return Err(PlayerStructuralConstructionError::ElementUnsupported { element });
    }
    if !bounds_are_within_player_reach(registries, state, record.bounds()) {
        return Err(PlayerStructuralConstructionError::ElementOutOfReach { element });
    }
    let required = resolve_required_mass(registries.materials(), record)
        .map_err(|_| PlayerStructuralConstructionError::MaterialStateChanged { element })?;
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
    Ok(PlayerStructuralConstructionContext {
        definition,
        player_position: player.position(),
        carried: player.carried_stockpile(),
        required,
        duration,
        completes_at,
    })
}

pub fn validate_start_player_structural_construction(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    element: StructuralElementId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(PlayerStructuralConstructionError::UnknownElement { element })?;
    let context =
        validate_player_structural_construction_context(registries, state, method, record, None)?;

    let material = if record.embodied_mass().is_zero() {
        if !state.can_spend_structure_revisions(2) {
            return Err(PlayerStructuralConstructionError::StructureRevisionExhausted);
        }
        let resolution =
            bind_structural_construction_selection(state, element, context.carried, selections)
                .map_err(PlayerStructuralConstructionError::Binding)?;
        Some(
            validate_structural_construction(registries, state, resolution)
                .map_err(PlayerStructuralConstructionError::Material)?,
        )
    } else {
        if record.embodied_mass() != context.required {
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

    let work = StructuralConstructionWork::new(element, method, state.tick(), context.completes_at);
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::StructuralConstruction { work },
        context.duration,
        context.definition.exertion(),
    )
    .map_err(PlayerStructuralConstructionError::Work)?;
    Ok(ValidatedPlayerStructuralConstructionStart {
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: context.player_position,
        material,
        work_start,
        work,
    })
}

pub(crate) fn validate_start_player_structural_construction_for_planned_member(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    record: &StructuralElementRecord,
    expected_structure_revision: u64,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    validate_start_player_structural_construction_for_planned_member_with_support(
        registries,
        state,
        method,
        record,
        expected_structure_revision,
        selections,
        None,
    )
}

pub(crate) fn validate_start_player_structural_construction_for_supported_planned_member(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    record: &StructuralElementRecord,
    expected_structure_revision: u64,
    selections: &[MaterialLotSelection],
    support: StructuralElementId,
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    validate_start_player_structural_construction_for_multi_supported_planned_member(
        registries,
        state,
        method,
        record,
        expected_structure_revision,
        selections,
        &[support],
    )
}

pub(crate) fn validate_start_player_structural_construction_for_multi_supported_planned_member(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    record: &StructuralElementRecord,
    expected_structure_revision: u64,
    selections: &[MaterialLotSelection],
    supports: &[StructuralElementId],
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    validate_start_player_structural_construction_for_planned_member_with_support(
        registries,
        state,
        method,
        record,
        expected_structure_revision,
        selections,
        Some(supports),
    )
}

fn validate_start_player_structural_construction_for_planned_member_with_support(
    registries: &Registries,
    state: &AppState,
    method: StructuralConstructionMethodId,
    record: &StructuralElementRecord,
    expected_structure_revision: u64,
    selections: &[MaterialLotSelection],
    transient_supports: Option<&[StructuralElementId]>,
) -> Result<ValidatedPlayerStructuralConstructionStart, PlayerStructuralConstructionError> {
    let context = validate_player_structural_construction_context(
        registries,
        state,
        method,
        record,
        transient_supports,
    )?;
    if !record.embodied_mass().is_zero() {
        return Err(PlayerStructuralConstructionError::MaterialStateChanged {
            element: record.id(),
        });
    }
    // Planning, materialization, and the already-admitted future activation each consume one
    // structure revision over this action's lifetime.
    if !state.can_spend_structure_revisions(3) {
        return Err(PlayerStructuralConstructionError::StructureRevisionExhausted);
    }
    let resolution =
        bind_structural_construction_selection(state, record.id(), context.carried, selections)
            .map_err(PlayerStructuralConstructionError::Binding)?;
    let material = validate_structural_construction_for_planned_member(
        registries,
        state,
        record,
        expected_structure_revision,
        resolution,
    )
    .map_err(PlayerStructuralConstructionError::Material)?;
    let work =
        StructuralConstructionWork::new(record.id(), method, state.tick(), context.completes_at);
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::StructuralConstruction { work },
        context.duration,
        context.definition.exertion(),
    )
    .map_err(PlayerStructuralConstructionError::Work)?;
    Ok(ValidatedPlayerStructuralConstructionStart {
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: context.player_position,
        material: Some(material),
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
