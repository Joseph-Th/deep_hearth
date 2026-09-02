//! Resolves deterministic structural loads, utilization, damage, and failure cascades.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::arithmetic::{
    scale_u128_fraction_ceil, scale_u128_fraction_floor, scaled_ratio_floor_saturating,
};
use crate::core::quantity::{Area, Force};
use crate::material::{MaterialDefinition, MaterialId, MaterialRegistry};

use super::definitions::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, StructuralProfileDefinition,
    StructuralProfileId, StructuralRegistry,
};
use super::state::{StructuralElementId, StructureState};

mod cascade;
mod overlay;
mod topology;

pub(crate) use overlay::StructuralAnalysisOverlay;
use topology::collect_connected_scope;

/// Player-readable structural state derived from load, material capacity, and persistent damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StructuralStage {
    Stable,
    Strained,
    Cracking,
    Failed,
}

/// Projects structural utilization using the same normalized ratio as authoritative analysis.
#[must_use]
pub fn calculate_structural_utilization_ppm(load: Force, capacity: Force) -> u128 {
    if capacity.is_zero() {
        return if load.is_zero() { 0 } else { u128::MAX };
    }
    scaled_ratio_floor_saturating(
        load.millinewtons(),
        capacity.millinewtons(),
        STRUCTURAL_PARTS_PER_MILLION,
    )
}

/// Projects the pristine axial capacity of one material/profile cross-section.
///
/// This is the same capacity calculation used by authoritative structural analysis. Planning,
/// presentation, and gameplay evaluation should use this projection instead of reproducing
/// strength-axis selection or unit conversion outside the structural owner.
#[must_use]
pub fn calculate_pristine_member_capacity(
    profile: &StructuralProfileDefinition,
    material: &MaterialDefinition,
    cross_section: Area,
) -> Option<Force> {
    let structural = material.properties().structural()?;
    let strength_kpa = match profile.load_mode() {
        StructuralLoadMode::Compression => structural.compressive_strength_kpa(),
        StructuralLoadMode::Tension => structural.tensile_strength_kpa(),
    };

    // 1 kPa * 1 mm^2 = 1 mN, so authored strength and cross-section multiply exactly.
    Some(Force::from_millinewtons(
        u128::from(strength_kpa) * u128::from(cross_section.square_millimeters()),
    ))
}

/// Why a structural member crossed into irreversible failure during one analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralFailureCause {
    Unsupported,
    Overloaded {
        carried_load: Force,
        effective_capacity: Force,
    },
}

/// Irreversible structural damage discovered by deterministic analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralDamageEvent {
    Cracked {
        element: StructuralElementId,
        carried_load: Force,
        pristine_capacity: Force,
    },
    Failed {
        element: StructuralElementId,
        cause: StructuralFailureCause,
    },
}

impl StructuralDamageEvent {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        match self {
            Self::Cracked {
                element,
                carried_load: _carried_load,
                pristine_capacity: _pristine_capacity,
            } => element,
            Self::Failed {
                element,
                cause: _cause,
            } => element,
        }
    }
}

/// Read-only load and capacity projection for one active or failed structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralAssessment {
    element: StructuralElementId,
    carried_load: Force,
    pristine_capacity: Force,
    effective_capacity: Force,
    utilization_ppm: u128,
    stage: StructuralStage,
}

impl StructuralAssessment {
    #[must_use]
    pub const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn carried_load(self) -> Force {
        self.carried_load
    }

    #[must_use]
    pub const fn pristine_capacity(self) -> Force {
        self.pristine_capacity
    }

    #[must_use]
    pub const fn effective_capacity(self) -> Force {
        self.effective_capacity
    }

    #[must_use]
    pub const fn utilization_ppm(self) -> u128 {
        self.utilization_ppm
    }

    #[must_use]
    pub const fn stage(self) -> StructuralStage {
        self.stage
    }
}

/// Complete deterministic structural projection plus irreversible damage that must be committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralAnalysis {
    assessments: Vec<StructuralAssessment>,
    damage_events: Vec<StructuralDamageEvent>,
}

impl StructuralAnalysis {
    #[must_use]
    pub fn assessments(&self) -> &[StructuralAssessment] {
        &self.assessments
    }

    #[must_use]
    pub fn damage_events(&self) -> &[StructuralDamageEvent] {
        &self.damage_events
    }
}

/// Structural analysis cannot be completed because authoritative references or arithmetic are invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralAnalysisError {
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    UnknownMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    NonStructuralMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    LoadOverflow {
        support: StructuralElementId,
    },
    AppliedLoadOverflow {
        element: StructuralElementId,
    },
    UnsupportedActiveElement {
        element: StructuralElementId,
    },
    ActiveGraphCycle,
}

impl Display for StructuralAnalysisError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {} during analysis",
                element.value(),
                profile.value()
            ),
            Self::UnknownMaterial { element, material } => write!(
                formatter,
                "structural element {} references unknown material {} during analysis",
                element.value(),
                material.value()
            ),
            Self::NonStructuralMaterial { element, material } => write!(
                formatter,
                "structural element {} uses material {} without authored structural strengths",
                element.value(),
                material.value()
            ),
            Self::LoadOverflow { support } => write!(
                formatter,
                "structural load accumulation overflowed support {}",
                support.value()
            ),
            Self::AppliedLoadOverflow { element } => write!(
                formatter,
                "structural load contributions overflowed element {}",
                element.value()
            ),
            Self::UnsupportedActiveElement { element } => write!(
                formatter,
                "structural analysis reached unsupported active element {} after collapse closure",
                element.value()
            ),
            Self::ActiveGraphCycle => {
                formatter.write_str("active structural support graph contains a cycle")
            }
        }
    }
}

impl Error for StructuralAnalysisError {}

fn pristine_capacity(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    element: StructuralElementId,
) -> Result<Force, StructuralAnalysisError> {
    let record = &state.element_map()[&element];
    let Some(profile) = profiles.get_profile(record.profile()) else {
        return Err(StructuralAnalysisError::UnknownProfile {
            element,
            profile: record.profile(),
        });
    };
    let Some(material) = materials.get_material(record.material()) else {
        return Err(StructuralAnalysisError::UnknownMaterial {
            element,
            material: record.material(),
        });
    };
    calculate_pristine_member_capacity(profile, material, record.cross_section()).ok_or(
        StructuralAnalysisError::NonStructuralMaterial {
            element,
            material: record.material(),
        },
    )
}

fn scale_capacity(capacity: Force, ppm: u32) -> Force {
    Force::from_millinewtons(scale_u128_fraction_floor(
        capacity.millinewtons(),
        ppm,
        STRUCTURAL_PARTS_PER_MILLION,
    ))
}

fn is_at_or_above_fraction(load: Force, capacity: Force, threshold_ppm: u32) -> bool {
    if capacity.is_zero() {
        return !load.is_zero();
    }
    let threshold_load = scale_u128_fraction_ceil(
        capacity.millinewtons(),
        threshold_ppm,
        STRUCTURAL_PARTS_PER_MILLION,
    );
    load.millinewtons() >= threshold_load
}

/// Calculates load distribution, warning stages, cracks, and cascading failures without mutation.
pub fn analyze_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
) -> Result<StructuralAnalysis, StructuralAnalysisError> {
    let scope: BTreeSet<_> = state.element_ids().collect();
    let overlay = StructuralAnalysisOverlay::default();
    cascade::analyze_structure_scoped(profiles, materials, state, &overlay, &scope)
}

pub(crate) fn analyze_structure_components_with_overlay(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    overlay: StructuralAnalysisOverlay,
    seeds: &BTreeSet<StructuralElementId>,
) -> Result<StructuralAnalysis, StructuralAnalysisError> {
    let scope = collect_connected_scope(state, &overlay, seeds);
    cascade::analyze_structure_scoped(profiles, materials, state, &overlay, &scope)
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
