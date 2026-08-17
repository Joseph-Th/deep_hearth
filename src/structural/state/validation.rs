//! Persistent-state validation for structural; this child audits private owner data without exposing mutation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Acceleration, AggregateMass, Force, Mass};
use crate::core::time::SimulationTick;
use crate::material::{
    MaterialId, MaterialPhase, MaterialPhaseStateError, MaterialRegistry, ParticleSizeStatePolicy,
    validate_material_phase_state,
};

use super::super::definitions::{StructuralProfileId, StructuralRegistry};
use super::super::geometry::{StructuralGeometryError, calculate_prismatic_material_mass_ceiling};
use super::super::load::calculate_aggregate_weight_force_ceiling;
use super::{StructuralElementId, StructuralLifecycle, StructuralLoadKind, StructureState};

/// Exhaustive failure found while validating decoded structural state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureValidationError {
    ZeroNextElementId,
    NextElementIdNotAboveAllocated {
        next: u32,
        highest: StructuralElementId,
    },
    ElementKeyMismatch {
        key: StructuralElementId,
        record: StructuralElementId,
    },
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    UnknownMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    ZeroCrossSection {
        element: StructuralElementId,
    },
    ZeroLength {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
    EmbodiedMassGeometryMismatch {
        element: StructuralElementId,
        stored: Mass,
        required: Mass,
    },
    UnmaterializedLoadBearingElement {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    EmbodiedMassMismatch {
        element: StructuralElementId,
        stored: Mass,
        traced: Mass,
    },
    EmbodiedMassOverflow {
        element: StructuralElementId,
    },
    ZeroEmbodiedTrace {
        element: StructuralElementId,
    },
    EmbodiedMaterialMismatch {
        element: StructuralElementId,
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedEmbodiedComposition {
        element: StructuralElementId,
        material: MaterialId,
    },
    UnknownEmbodiedCommodity {
        element: StructuralElementId,
    },
    UnsupportedEmbodiedPhase {
        element: StructuralElementId,
        form: crate::material::FormId,
        phase: MaterialPhase,
    },
    UnsupportedEmbodiedParticulateForm {
        element: StructuralElementId,
        form: crate::material::FormId,
    },
    InvalidEmbodiedPhaseState {
        element: StructuralElementId,
        error: MaterialPhaseStateError,
    },
    UnknownEmbodiedCompositionMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    InvalidEmbodiedProvenanceRange {
        element: StructuralElementId,
    },
    EmbodiedProvenanceInFuture {
        element: StructuralElementId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    SelfWeightOverflow {
        element: StructuralElementId,
    },
    SelfWeightMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    ZeroLoadContribution {
        element: StructuralElementId,
        kind: StructuralLoadKind,
    },
    CreatedInFuture {
        element: StructuralElementId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    PlannedElementCracked {
        element: StructuralElementId,
    },
    FailedElementNotCracked {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        element: StructuralElementId,
    },
    OrphanSupportIndex {
        element: StructuralElementId,
    },
    UnknownSupportReference {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SelfSupport {
        element: StructuralElementId,
    },
    GroundedElementHasSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ReverseIndexMismatch {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SupportCycle {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ActiveElementUnsupported {
        element: StructuralElementId,
    },
}

impl Display for StructureValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextElementId => {
                formatter.write_str("structural next-id cursor must be nonzero")
            }
            Self::NextElementIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "structural next-id cursor {next} is not above allocated element {}",
                highest.value()
            ),
            Self::UnsupportedEmbodiedParticulateForm { element, form } => write!(
                formatter,
                "structural element {} directly embodies particulate form {} without consolidation physics",
                element.value(),
                form.value()
            ),
            Self::ZeroLength { element } => write!(
                formatter,
                "structural element {} has zero physical length",
                element.value()
            ),
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} has invalid physical geometry: {error}",
                element.value()
            ),
            Self::EmbodiedMassGeometryMismatch {
                element,
                stored,
                required,
            } => write!(
                formatter,
                "structural element {} owns {} mg but its geometry and material density require {} mg",
                element.value(),
                stored.milligrams(),
                required.milligrams()
            ),
            Self::ElementKeyMismatch { key, record } => write!(
                formatter,
                "structural element map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {}",
                element.value(),
                profile.value()
            ),
            Self::UnknownMaterial { element, material } => write!(
                formatter,
                "structural element {} references unknown material {}",
                element.value(),
                material.value()
            ),
            Self::ZeroCrossSection { element } => write!(
                formatter,
                "structural element {} has zero cross-sectional area",
                element.value()
            ),
            Self::UnmaterializedLoadBearingElement { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} without embodied construction matter",
                element.value()
            ),
            Self::EmbodiedMassMismatch {
                element,
                stored,
                traced,
            } => write!(
                formatter,
                "structural element {} stores {} mg embodied mass but traces own {} mg",
                element.value(),
                stored.milligrams(),
                traced.milligrams()
            ),
            Self::EmbodiedMassOverflow { element } => write!(
                formatter,
                "structural element {} embodied traces overflow single-member mass storage",
                element.value()
            ),
            Self::ZeroEmbodiedTrace { element } => write!(
                formatter,
                "structural element {} contains a zero-mass embodied trace",
                element.value()
            ),
            Self::EmbodiedMaterialMismatch {
                element,
                expected,
                found,
            } => write!(
                formatter,
                "structural element {} is authored as material {} but owns commodity material {}",
                element.value(),
                expected.value(),
                found.value()
            ),
            Self::UnsupportedEmbodiedComposition { element, material } => write!(
                formatter,
                "structural element {} uses single-material strength for material {} but its embodied matter is not pure",
                element.value(),
                material.value()
            ),
            Self::UnknownEmbodiedCommodity { element } => write!(
                formatter,
                "structural element {} owns an unknown material/form commodity",
                element.value()
            ),
            Self::UnsupportedEmbodiedPhase {
                element,
                form,
                phase,
            } => write!(
                formatter,
                "structural element {} owns {phase:?} material form {}; structural embodiment must be solid",
                element.value(),
                form.value()
            ),
            Self::InvalidEmbodiedPhaseState { element, error } => write!(
                formatter,
                "structural element {} has invalid embodied material phase state: {error}",
                element.value()
            ),
            Self::UnknownEmbodiedCompositionMaterial { element, material } => write!(
                formatter,
                "structural element {} embodied composition references unknown material {}",
                element.value(),
                material.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { element } => write!(
                formatter,
                "structural element {} embodied material has an inverted provenance range",
                element.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                element,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "structural element {} owns material provenance through tick {} after current tick {}",
                element.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::SelfWeightOverflow { element } => write!(
                formatter,
                "structural element {} embodied mass exceeds self-weight force range",
                element.value()
            ),
            Self::SelfWeightMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN self-weight but embodied matter requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::ZeroLoadContribution { element, kind } => write!(
                formatter,
                "structural element {} stores redundant zero {kind:?} load contribution",
                element.value()
            ),
            Self::CreatedInFuture {
                element,
                created_at,
                current,
            } => write!(
                formatter,
                "structural element {} was created at tick {} after current tick {}",
                element.value(),
                created_at.value(),
                current.value()
            ),
            Self::PlannedElementCracked { element } => write!(
                formatter,
                "planned structural element {} cannot already contain irreversible crack damage",
                element.value()
            ),
            Self::FailedElementNotCracked { element } => write!(
                formatter,
                "failed structural element {} is missing persistent crack damage",
                element.value()
            ),
            Self::MissingSupportIndex { element } => write!(
                formatter,
                "structural element {} is missing synchronized support index entries",
                element.value()
            ),
            Self::OrphanSupportIndex { element } => write!(
                formatter,
                "structural support index contains missing element {}",
                element.value()
            ),
            Self::UnknownSupportReference { element, support } => write!(
                formatter,
                "structural element {} references missing support {}",
                element.value(),
                support.value()
            ),
            Self::SelfSupport { element } => write!(
                formatter,
                "structural element {} cannot support itself",
                element.value()
            ),
            Self::GroundedElementHasSupport { element, support } => write!(
                formatter,
                "ground-anchored structural element {} cannot also route load through support {}",
                element.value(),
                support.value()
            ),
            Self::ReverseIndexMismatch { element, support } => write!(
                formatter,
                "structural support indexes disagree for element {} and support {}",
                element.value(),
                support.value()
            ),
            Self::SupportCycle { element, support } => write!(
                formatter,
                "structural support edge {} -> {} participates in a cycle",
                element.value(),
                support.value()
            ),
            Self::ActiveElementUnsupported { element } => write!(
                formatter,
                "active structural element {} has no active support or ground anchor",
                element.value()
            ),
        }
    }
}

impl Error for StructureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry {
                element: _element,
                error,
            } => Some(error),
            Self::InvalidEmbodiedPhaseState {
                element: _element,
                error,
            } => Some(error),
            Self::NextElementIdNotAboveAllocated {
                next: _next,
                highest: _highest,
            } => None,
            Self::ElementKeyMismatch {
                key: _key,
                record: _record,
            } => None,
            Self::UnknownProfile {
                element: _element,
                profile: _profile,
            } => None,
            Self::UnknownMaterial {
                element: _element,
                material: _material,
            }
            | Self::UnsupportedEmbodiedComposition {
                element: _element,
                material: _material,
            }
            | Self::UnknownEmbodiedCompositionMaterial {
                element: _element,
                material: _material,
            } => None,
            Self::ZeroCrossSection { element: _element }
            | Self::ZeroLength { element: _element }
            | Self::EmbodiedMassOverflow { element: _element }
            | Self::ZeroEmbodiedTrace { element: _element }
            | Self::UnknownEmbodiedCommodity { element: _element }
            | Self::InvalidEmbodiedProvenanceRange { element: _element }
            | Self::SelfWeightOverflow { element: _element }
            | Self::PlannedElementCracked { element: _element }
            | Self::FailedElementNotCracked { element: _element }
            | Self::MissingSupportIndex { element: _element }
            | Self::OrphanSupportIndex { element: _element }
            | Self::SelfSupport { element: _element }
            | Self::ActiveElementUnsupported { element: _element } => None,
            Self::EmbodiedMassGeometryMismatch {
                element: _element,
                stored: _stored,
                required: _required,
            } => None,
            Self::UnmaterializedLoadBearingElement {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::EmbodiedMassMismatch {
                element: _element,
                stored: _stored,
                traced: _traced,
            } => None,
            Self::EmbodiedMaterialMismatch {
                element: _element,
                expected: _expected,
                found: _found,
            } => None,
            Self::UnsupportedEmbodiedPhase {
                element: _element,
                form: _form,
                phase: _phase,
            } => None,
            Self::UnsupportedEmbodiedParticulateForm {
                element: _element,
                form: _form,
            } => None,
            Self::EmbodiedProvenanceInFuture {
                element: _element,
                latest_created_at: _latest_created_at,
                current: _current,
            } => None,
            Self::SelfWeightMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
            Self::ZeroLoadContribution {
                element: _element,
                kind: _kind,
            } => None,
            Self::CreatedInFuture {
                element: _element,
                created_at: _created_at,
                current: _current,
            } => None,
            Self::UnknownSupportReference {
                element: _element,
                support: _support,
            }
            | Self::GroundedElementHasSupport {
                element: _element,
                support: _support,
            }
            | Self::ReverseIndexMismatch {
                element: _element,
                support: _support,
            }
            | Self::SupportCycle {
                element: _element,
                support: _support,
            } => None,
            Self::ZeroNextElementId => None,
        }
    }
}

pub(crate) fn validate_loaded_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    current_tick: SimulationTick,
    gravity: Acceleration,
) -> Result<(), StructureValidationError> {
    if state.next_element_id == 0 {
        return Err(StructureValidationError::ZeroNextElementId);
    }
    if let Some(highest) = state.elements.keys().next_back().copied()
        && highest.value() >= state.next_element_id
    {
        return Err(StructureValidationError::NextElementIdNotAboveAllocated {
            next: state.next_element_id,
            highest,
        });
    }

    for (id, record) in &state.elements {
        if *id != record.id {
            return Err(StructureValidationError::ElementKeyMismatch {
                key: *id,
                record: record.id,
            });
        }
        if profiles.get_profile(record.profile()).is_none() {
            return Err(StructureValidationError::UnknownProfile {
                element: record.id,
                profile: record.profile(),
            });
        }
        if materials.get_material(record.material()).is_none() {
            return Err(StructureValidationError::UnknownMaterial {
                element: record.id,
                material: record.material(),
            });
        }
        if record.cross_section().is_zero() {
            return Err(StructureValidationError::ZeroCrossSection { element: record.id });
        }
        if record.length().is_zero() {
            return Err(StructureValidationError::ZeroLength { element: record.id });
        }
        if record.lifecycle != StructuralLifecycle::Planned && record.embodied_mass.is_zero() {
            return Err(StructureValidationError::UnmaterializedLoadBearingElement {
                element: record.id,
                lifecycle: record.lifecycle,
            });
        }
        let mut traced_mass = Mass::ZERO;
        for trace in &record.embodied_material {
            if trace.mass().is_zero() {
                return Err(StructureValidationError::ZeroEmbodiedTrace { element: record.id });
            }
            traced_mass = traced_mass
                .checked_add(trace.mass())
                .ok_or(StructureValidationError::EmbodiedMassOverflow { element: record.id })?;
            let commodity = trace.profile().commodity();
            if !materials.has_commodity(commodity) {
                return Err(StructureValidationError::UnknownEmbodiedCommodity {
                    element: record.id,
                });
            }
            let form = match materials.get_form(commodity.form()) {
                Some(form) => form,
                None => {
                    return Err(StructureValidationError::UnknownEmbodiedCommodity {
                        element: record.id,
                    });
                }
            };
            if form.phase() != MaterialPhase::Solid {
                return Err(StructureValidationError::UnsupportedEmbodiedPhase {
                    element: record.id,
                    form: commodity.form(),
                    phase: form.phase(),
                });
            }
            if form.particle_size_policy() == ParticleSizeStatePolicy::Required {
                return Err(
                    StructureValidationError::UnsupportedEmbodiedParticulateForm {
                        element: record.id,
                        form: commodity.form(),
                    },
                );
            }
            validate_material_phase_state(
                materials,
                commodity,
                trace.profile().composition(),
                trace.profile().temperature(),
            )
            .map_err(
                |error| StructureValidationError::InvalidEmbodiedPhaseState {
                    element: record.id,
                    error,
                },
            )?;
            if commodity.material() != record.material() {
                return Err(StructureValidationError::EmbodiedMaterialMismatch {
                    element: record.id,
                    expected: record.material(),
                    found: commodity.material(),
                });
            }
            if trace.profile().composition()
                != &crate::material::MaterialComposition::pure(record.material())
            {
                return Err(StructureValidationError::UnsupportedEmbodiedComposition {
                    element: record.id,
                    material: record.material(),
                });
            }
            for component in trace.profile().composition().components() {
                if materials.get_material(component.material()).is_none() {
                    return Err(
                        StructureValidationError::UnknownEmbodiedCompositionMaterial {
                            element: record.id,
                            material: component.material(),
                        },
                    );
                }
            }
            let provenance = trace.provenance();
            if provenance.latest_created_at() < provenance.earliest_created_at() {
                return Err(StructureValidationError::InvalidEmbodiedProvenanceRange {
                    element: record.id,
                });
            }
            if provenance.latest_created_at() > current_tick {
                return Err(StructureValidationError::EmbodiedProvenanceInFuture {
                    element: record.id,
                    latest_created_at: provenance.latest_created_at(),
                    current: current_tick,
                });
            }
        }
        if traced_mass != record.embodied_mass {
            return Err(StructureValidationError::EmbodiedMassMismatch {
                element: record.id,
                stored: record.embodied_mass,
                traced: traced_mass,
            });
        }
        if !record.embodied_mass.is_zero() {
            let required = calculate_prismatic_material_mass_ceiling(
                materials,
                record.material(),
                record.cross_section(),
                record.length(),
            )
            .map_err(|error| StructureValidationError::Geometry {
                element: record.id,
                error,
            })?;
            if record.embodied_mass != required {
                return Err(StructureValidationError::EmbodiedMassGeometryMismatch {
                    element: record.id,
                    stored: record.embodied_mass,
                    required,
                });
            }
        }
        let expected_self_weight = calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_mass(record.embodied_mass),
            gravity,
        )
        .ok_or(StructureValidationError::SelfWeightOverflow { element: record.id })?;
        let stored_self_weight = record.load(StructuralLoadKind::SelfWeight);
        if stored_self_weight != expected_self_weight {
            return Err(StructureValidationError::SelfWeightMismatch {
                element: record.id,
                stored: stored_self_weight,
                expected: expected_self_weight,
            });
        }
        if let Some((kind, _)) = record.loads.iter().find(|(_, load)| load.is_zero()) {
            return Err(StructureValidationError::ZeroLoadContribution {
                element: record.id,
                kind: *kind,
            });
        }
        if record.created_at > current_tick {
            return Err(StructureValidationError::CreatedInFuture {
                element: record.id,
                created_at: record.created_at,
                current: current_tick,
            });
        }
        if record.lifecycle == StructuralLifecycle::Planned && record.is_cracked {
            return Err(StructureValidationError::PlannedElementCracked { element: record.id });
        }
        if record.lifecycle == StructuralLifecycle::Failed && !record.is_cracked {
            return Err(StructureValidationError::FailedElementNotCracked { element: record.id });
        }
        if !state.supports_by_element.contains_key(id)
            || !state.dependents_by_support.contains_key(id)
        {
            return Err(StructureValidationError::MissingSupportIndex { element: *id });
        }
    }

    for id in state
        .supports_by_element
        .keys()
        .chain(state.dependents_by_support.keys())
    {
        if !state.elements.contains_key(id) {
            return Err(StructureValidationError::OrphanSupportIndex { element: *id });
        }
    }

    for (element, supports) in &state.supports_by_element {
        for support in supports {
            if element == support {
                return Err(StructureValidationError::SelfSupport { element: *element });
            }
            if state.elements[element].is_grounded() {
                return Err(StructureValidationError::GroundedElementHasSupport {
                    element: *element,
                    support: *support,
                });
            }
            if !state.elements.contains_key(support) {
                return Err(StructureValidationError::UnknownSupportReference {
                    element: *element,
                    support: *support,
                });
            }
            if !state
                .dependents_by_support
                .get(support)
                .is_some_and(|dependents| dependents.contains(element))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
            if state.has_path(*support, *element) {
                return Err(StructureValidationError::SupportCycle {
                    element: *element,
                    support: *support,
                });
            }
        }
    }
    for (support, dependents) in &state.dependents_by_support {
        for element in dependents {
            if !state
                .supports_by_element
                .get(element)
                .is_some_and(|supports| supports.contains(support))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
        }
    }

    for record in state.elements.values() {
        if record.lifecycle != StructuralLifecycle::Active || record.is_grounded() {
            continue;
        }
        let has_active_support =
            state
                .supports_by_element
                .get(&record.id)
                .is_some_and(|supports| {
                    supports.iter().any(|support| {
                        state.elements.get(support).is_some_and(|candidate| {
                            candidate.lifecycle == StructuralLifecycle::Active
                        })
                    })
                });
        if !has_active_support {
            return Err(StructureValidationError::ActiveElementUnsupported { element: record.id });
        }
    }

    Ok(())
}
