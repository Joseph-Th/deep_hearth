//! Read-only player projection of one proposed supported structural placement.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::analysis::{
    StructuralAnalysisError, StructuralAssessment, StructuralDamageEvent, analyze_structure,
};
use super::construction_execution::resolve_required_mass;
use super::element_execution::{
    MultiSupportedStructuralElementPlan, PlanPlayerStructuralElementError,
    validate_plan_player_multi_supported_structural_element,
};
use super::load::calculate_aggregate_weight_force_ceiling;
use super::state::{StructuralElementId, StructuralLoadKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStructuralPlacementProjectionError {
    Plan(PlanPlayerStructuralElementError),
    Geometry(super::StructuralGeometryError),
    SelfWeightOverflow,
    Analysis(StructuralAnalysisError),
}

impl Display for PlayerStructuralPlacementProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(error) => write!(formatter, "structural placement is not legal: {error}"),
            Self::Geometry(error) => {
                write!(
                    formatter,
                    "structural placement geometry cannot be projected: {error}"
                )
            }
            Self::SelfWeightOverflow => {
                formatter.write_str("structural placement self-weight exceeds representable force")
            }
            Self::Analysis(error) => {
                write!(
                    formatter,
                    "structural placement load analysis failed: {error}"
                )
            }
        }
    }
}

impl Error for PlayerStructuralPlacementProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            Self::Geometry(error) => Some(error),
            Self::Analysis(error) => Some(error),
            Self::SelfWeightOverflow => None,
        }
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralSupportPlacementProjection {
    support: StructuralElementId,
    before: StructuralAssessment,
    after: StructuralAssessment,
}

impl StructuralSupportPlacementProjection {
    #[must_use]
    pub const fn support(self) -> StructuralElementId {
        self.support
    }

    #[must_use]
    pub const fn before(self) -> StructuralAssessment {
        self.before
    }

    #[must_use]
    pub const fn after(self) -> StructuralAssessment {
        self.after
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerStructuralPlacementProjection {
    prospective_element: StructuralElementId,
    required_mass: Mass,
    self_weight: Force,
    member: StructuralAssessment,
    supports: Vec<StructuralSupportPlacementProjection>,
    damage_events: Vec<StructuralDamageEvent>,
}

impl PlayerStructuralPlacementProjection {
    #[must_use]
    pub const fn prospective_element(&self) -> StructuralElementId {
        self.prospective_element
    }

    #[must_use]
    pub const fn required_mass(&self) -> Mass {
        self.required_mass
    }

    #[must_use]
    pub const fn self_weight(&self) -> Force {
        self.self_weight
    }

    #[must_use]
    pub const fn member(&self) -> StructuralAssessment {
        self.member
    }

    #[must_use]
    pub fn supports(&self) -> &[StructuralSupportPlacementProjection] {
        &self.supports
    }

    #[must_use]
    pub fn damage_events(&self) -> &[StructuralDamageEvent] {
        &self.damage_events
    }
}

/// Projects exact structural consequences for one currently legal multi-supported player placement.
///
/// This is a read-only engineering surface. It reuses player planning legality, derives the same
/// geometry-backed mass and self-weight as construction, then analyzes a structure-owner clone with
/// the proposed member active. It does not reserve matter, authorize a later commit, or rank options.
pub fn project_player_multi_supported_structural_placement(
    registries: &Registries,
    state: &AppState,
    plan: MultiSupportedStructuralElementPlan,
) -> Result<PlayerStructuralPlacementProjection, PlayerStructuralPlacementProjectionError> {
    let validated =
        validate_plan_player_multi_supported_structural_element(registries, state, plan)
            .map_err(PlayerStructuralPlacementProjectionError::Plan)?;
    let record = validated.record().clone();
    let prospective_element = record.id();
    let supports = validated.supports().to_vec();
    let required_mass = resolve_required_mass(registries.materials(), &record)
        .map_err(PlayerStructuralPlacementProjectionError::Geometry)?;
    let self_weight = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(required_mass),
        registries.core().gravity(),
    )
    .ok_or(PlayerStructuralPlacementProjectionError::SelfWeightOverflow)?;
    let before = analyze_structure(
        registries.structural(),
        registries.materials(),
        state.structures(),
    )
    .map_err(PlayerStructuralPlacementProjectionError::Analysis)?;

    let mut projected = state.structures().clone();
    let next_element_id = prospective_element
        .value()
        .checked_add(1)
        .unwrap_or_else(|| {
            unreachable!("validated structural plan reserved element identity headroom")
        });
    projected.insert_element(record, next_element_id, validated.next_structure_revision());
    for support in supports.iter().copied() {
        projected.link_support(prospective_element, support);
    }
    projected.set_load(
        prospective_element,
        StructuralLoadKind::SelfWeight,
        self_weight,
    );
    projected.activate_element(prospective_element);
    let after = analyze_structure(registries.structural(), registries.materials(), &projected)
        .map_err(PlayerStructuralPlacementProjectionError::Analysis)?;

    let member = after
        .assessments()
        .iter()
        .copied()
        .find(|assessment| assessment.element() == prospective_element)
        .unwrap_or_else(|| {
            panic!("projected active structural member disappeared from canonical analysis")
        });
    let support_projections = supports
        .into_iter()
        .map(|support| {
            let before = before
                .assessments()
                .iter()
                .copied()
                .find(|assessment| assessment.element() == support)
                .unwrap_or_else(|| {
                    panic!("validated active support disappeared from current structural analysis")
                });
            let after = after
                .assessments()
                .iter()
                .copied()
                .find(|assessment| assessment.element() == support)
                .unwrap_or_else(|| {
                    panic!("validated support disappeared from projected structural analysis")
                });
            StructuralSupportPlacementProjection {
                support,
                before,
                after,
            }
        })
        .collect();
    let damage_events = after
        .damage_events()
        .iter()
        .copied()
        .filter(|event| !before.damage_events().contains(event))
        .collect();
    Ok(PlayerStructuralPlacementProjection {
        prospective_element,
        required_mass,
        self_weight,
        member,
        supports: support_projections,
        damage_events,
    })
}

#[cfg(test)]
#[path = "placement_projection_tests.rs"]
mod tests;
