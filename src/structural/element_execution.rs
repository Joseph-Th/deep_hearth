//! Canonical admission of inert planned structural members.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::player::{PlayerActionGuard, PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::definitions::StructuralProfileId;
use super::geometry::StructuralGeometryError;
use super::state::{
    StructuralElementConfiguration, StructuralElementGeometry, StructuralElementId,
    StructuralElementRecord, StructuralLifecycle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddStructuralElementError {
    UnknownProfile { profile: StructuralProfileId },
    UnknownMaterial { material: MaterialId },
    NonStructuralMaterial { material: MaterialId },
    Geometry(StructuralGeometryError),
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddStructuralElementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { profile } => {
                write!(formatter, "unknown structural profile {}", profile.value())
            }
            Self::UnknownMaterial { material } => write!(
                formatter,
                "unknown structural material {}",
                material.value()
            ),
            Self::NonStructuralMaterial { material } => write!(
                formatter,
                "material {} has no authored structural strengths",
                material.value()
            ),
            Self::Geometry(error) => write!(formatter, "invalid structural geometry: {error}"),
            Self::IdExhausted => {
                formatter.write_str("structural element identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
        }
    }
}

impl Error for AddStructuralElementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            Self::UnknownProfile { .. }
            | Self::UnknownMaterial { .. }
            | Self::NonStructuralMaterial { .. }
            | Self::IdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

fn validate_definition(
    registries: &Registries,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
) -> Result<(), AddStructuralElementError> {
    registries
        .structural()
        .get_profile(profile)
        .ok_or(AddStructuralElementError::UnknownProfile { profile })?;
    let material_definition = registries
        .materials()
        .get_material(material)
        .ok_or(AddStructuralElementError::UnknownMaterial { material })?;
    if material_definition.properties().structural().is_none() {
        return Err(AddStructuralElementError::NonStructuralMaterial { material });
    }
    geometry
        .validate()
        .map_err(AddStructuralElementError::Geometry)
}

fn build_record(
    state: &AppState,
    id: StructuralElementId,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
    is_grounded: bool,
) -> StructuralElementRecord {
    StructuralElementRecord {
        id,
        configuration: StructuralElementConfiguration {
            profile,
            material,
            geometry,
            is_grounded,
        },
        embodied_material: Vec::new(),
        loads: Default::default(),
        lifecycle: StructuralLifecycle::Planned,
        is_cracked: false,
        created_at: state.tick(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroundedStructuralElementPlan {
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
}

impl GroundedStructuralElementPlan {
    #[must_use]
    pub const fn new(
        profile: StructuralProfileId,
        material: MaterialId,
        geometry: StructuralElementGeometry,
    ) -> Self {
        Self {
            profile,
            material,
            geometry,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanPlayerStructuralElementError {
    PlayerNotAdmitted,
    PlayerDead,
    TargetOverlapsPlayer { player: VoxelCoord },
    TargetNotSingleVoxel,
    TargetNotAdjacent { player: VoxelCoord },
    Element(AddStructuralElementError),
}

impl Display for PlanPlayerStructuralElementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("structural planning requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot plan structural work"),
            Self::TargetOverlapsPlayer { player } => write!(
                formatter,
                "grounded structural target overlaps player voxel ({}, {}, {})",
                player.x(),
                player.y(),
                player.z()
            ),
            Self::TargetNotSingleVoxel => formatter
                .write_str("ordinary grounded structural target must occupy exactly one voxel"),
            Self::TargetNotAdjacent { player } => write!(
                formatter,
                "grounded structural target is not face-adjacent to player voxel ({}, {}, {})",
                player.x(),
                player.y(),
                player.z()
            ),
            Self::Element(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for PlanPlayerStructuralElementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Element(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::PlayerDead
            | Self::TargetOverlapsPlayer { .. }
            | Self::TargetNotSingleVoxel
            | Self::TargetNotAdjacent { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanPlayerStructuralElementCommitError {
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
    TickChanged {
        expected: SimulationTick,
        actual: SimulationTick,
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
    StructureIdentityChanged {
        expected: u32,
        actual: u32,
    },
}

impl Display for PlanPlayerStructuralElementCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "structural planning survival revision changed from {expected} to {actual} before commit"
            ),
            Self::TickChanged { expected, actual } => write!(
                formatter,
                "structural planning expected tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structural revision changed from {expected} to {actual} before planning commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "structural planning logistics revision changed from {expected} to {actual} before commit"
            ),
            Self::PlayerMoved {
                expected,
                actual: Some(actual),
            } => write!(
                formatter,
                "player moved from structural planning origin ({},{},{}) to ({},{},{}) before commit",
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
                "player disappeared from structural planning origin ({},{},{}) before commit",
                expected.x(),
                expected.y(),
                expected.z()
            ),
            Self::StructureIdentityChanged { expected, actual } => write!(
                formatter,
                "next structural element id changed from {expected} to {actual} before planning commit"
            ),
        }
    }
}

impl Error for PlanPlayerStructuralElementCommitError {}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerStructuralElementPlan {
    player_action: PlayerActionGuard,
    record: StructuralElementRecord,
    expected_tick: SimulationTick,
    expected_structure_revision: u64,
    expected_logistics_revision: u64,
    expected_player_position: VoxelCoord,
    expected_next_element_id: u32,
    next_element_id: u32,
    next_structure_revision: u64,
}

impl ValidatedPlayerStructuralElementPlan {
    #[must_use]
    pub const fn element(&self) -> StructuralElementId {
        self.record.id
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralElementId, PlanPlayerStructuralElementCommitError> {
        if state.tick() != self.expected_tick {
            return Err(PlanPlayerStructuralElementCommitError::TickChanged {
                expected: self.expected_tick,
                actual: state.tick(),
            });
        }
        if state.structures().revision() != self.expected_structure_revision {
            return Err(
                PlanPlayerStructuralElementCommitError::StaleStructureRevision {
                    expected: self.expected_structure_revision,
                    actual: state.structures().revision(),
                },
            );
        }
        self.player_action.precheck(state).map_err(|stale| {
            PlanPlayerStructuralElementCommitError::StaleSurvivalRevision {
                expected: stale.expected,
                actual: stale.actual,
            }
        })?;
        if state.logistics().revision() != self.expected_logistics_revision {
            return Err(
                PlanPlayerStructuralElementCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: state.logistics().revision(),
                },
            );
        }
        let actual_position = state.logistics().player().map(|player| player.position());
        if actual_position != Some(self.expected_player_position) {
            return Err(PlanPlayerStructuralElementCommitError::PlayerMoved {
                expected: self.expected_player_position,
                actual: actual_position,
            });
        }
        if state.structures().next_element_id() != self.expected_next_element_id {
            return Err(
                PlanPlayerStructuralElementCommitError::StructureIdentityChanged {
                    expected: self.expected_next_element_id,
                    actual: state.structures().next_element_id(),
                },
            );
        }
        let id = self.record.id;
        state.structure_state_mut().insert_element(
            self.record,
            self.next_element_id,
            self.next_structure_revision,
        );
        Ok(id)
    }
}

fn target_is_face_adjacent(bounds: crate::spatial::VoxelBounds, player: VoxelCoord) -> bool {
    [
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

pub fn validate_plan_player_grounded_structural_element(
    registries: &Registries,
    state: &AppState,
    plan: GroundedStructuralElementPlan,
) -> Result<ValidatedPlayerStructuralElementPlan, PlanPlayerStructuralElementError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlanPlayerStructuralElementError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlanPlayerStructuralElementError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlanPlayerStructuralElementError::PlayerNotAdmitted)?;
    let position = player.position();
    let bounds = plan.geometry.bounds();
    if bounds.has_voxel(position) {
        return Err(PlanPlayerStructuralElementError::TargetOverlapsPlayer { player: position });
    }
    if bounds.voxel_count() != Some(1) {
        return Err(PlanPlayerStructuralElementError::TargetNotSingleVoxel);
    }
    if !target_is_face_adjacent(bounds, position) {
        return Err(PlanPlayerStructuralElementError::TargetNotAdjacent { player: position });
    }
    validate_definition(registries, plan.profile, plan.material, plan.geometry)
        .map_err(PlanPlayerStructuralElementError::Element)?;
    if !state.can_spend_structure_revisions(1) {
        return Err(PlanPlayerStructuralElementError::Element(
            AddStructuralElementError::RevisionExhausted,
        ));
    }
    let expected_next_element_id = state.structures().next_element_id();
    let id = StructuralElementId::new(expected_next_element_id);
    let next_element_id = expected_next_element_id.checked_add(1).ok_or(
        PlanPlayerStructuralElementError::Element(AddStructuralElementError::IdExhausted),
    )?;
    let next_structure_revision = state.structures().revision().checked_add(1).ok_or(
        PlanPlayerStructuralElementError::Element(AddStructuralElementError::RevisionExhausted),
    )?;
    Ok(ValidatedPlayerStructuralElementPlan {
        player_action,
        record: build_record(state, id, plan.profile, plan.material, plan.geometry, true),
        expected_tick: state.tick(),
        expected_structure_revision: state.structures().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_player_position: position,
        expected_next_element_id,
        next_element_id,
        next_structure_revision,
    })
}

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn add_structural_element(
    registries: &Registries,
    state: &mut AppState,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
    is_grounded: bool,
) -> Result<StructuralElementId, AddStructuralElementError> {
    validate_definition(registries, profile, material, geometry)?;
    let structures = state.structures();
    let id = StructuralElementId::new(structures.next_element_id());
    let next_element_id = structures
        .next_element_id()
        .checked_add(1)
        .ok_or(AddStructuralElementError::IdExhausted)?;
    let next_revision = structures
        .revision()
        .checked_add(1)
        .ok_or(AddStructuralElementError::RevisionExhausted)?;
    let record = build_record(state, id, profile, material, geometry, is_grounded);
    state
        .structure_state_mut()
        .insert_element(record, next_element_id, next_revision);
    Ok(id)
}
