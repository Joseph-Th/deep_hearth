//! Failure vocabulary for structural trusted-load validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Force, Mass};
use crate::core::time::SimulationTick;
use crate::material::{FormId, MaterialId, MaterialPhaseStateError, ParticleSizeStateError};

use super::super::super::definitions::StructuralProfileId;
use super::super::super::geometry::StructuralGeometryError;
use super::super::{StructuralElementId, StructuralLifecycle, StructuralLoadKind};

/// Exhaustive failure found while validating decoded structural state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureValidationError {
    ZeroNextElementId,
    ZeroElementId,
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
    NonStructuralMaterial {
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
        embodied: Mass,
        required: Mass,
    },
    UnmaterializedLoadBearingElement {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
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
    UnconsolidatedEmbodiedForm {
        element: StructuralElementId,
        form: FormId,
    },
    InvalidEmbodiedPhaseState {
        element: StructuralElementId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        element: StructuralElementId,
        error: ParticleSizeStateError,
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
    SupportOutOfContact {
        element: StructuralElementId,
        support: StructuralElementId,
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
            Self::ZeroElementId => formatter.write_str("structural element ID must be nonzero"),
            Self::NextElementIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "structural next-id cursor {next} is not above allocated element {}",
                highest.value()
            ),
            Self::UnconsolidatedEmbodiedForm { element, form } => write!(
                formatter,
                "structural element {} directly embodies unconsolidated form {} without first producing rigid construction stock",
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
                embodied,
                required,
            } => write!(
                formatter,
                "structural element {} owns {} mg but its geometry and material density require {} mg",
                element.value(),
                embodied.milligrams(),
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
            Self::NonStructuralMaterial { element, material } => write!(
                formatter,
                "structural element {} uses material {} without authored structural strengths",
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
            Self::InvalidEmbodiedPhaseState { element, error } => write!(
                formatter,
                "structural element {} has invalid embodied material phase state: {error}",
                element.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { element, error } => write!(
                formatter,
                "structural element {} has invalid embodied particle-size state: {error}",
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
            Self::SupportOutOfContact { element, support } => write!(
                formatter,
                "structural support edge {} -> {} crosses empty space; member bounds do not touch or overlap",
                element.value(),
                support.value()
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
            Self::InvalidEmbodiedParticleSizeState {
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
            Self::UnknownMaterial { .. }
            | Self::NonStructuralMaterial { .. }
            | Self::UnsupportedEmbodiedComposition { .. }
            | Self::UnknownEmbodiedCompositionMaterial { .. } => None,
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
                embodied: _embodied,
                required: _required,
            } => None,
            Self::UnmaterializedLoadBearingElement {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::EmbodiedMaterialMismatch {
                element: _element,
                expected: _expected,
                found: _found,
            } => None,
            Self::UnconsolidatedEmbodiedForm {
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
            | Self::SupportOutOfContact {
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
            Self::ZeroNextElementId | Self::ZeroElementId => None,
        }
    }
}
