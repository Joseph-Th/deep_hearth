//! Human-readable diagnostics for trusted structural-state validation failures.

use std::fmt::{Display, Formatter};

use super::StructureValidationError;

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
                "structural support edge {} -> {} lacks positive-area voxel contact; edge-only and corner-only touches cannot carry support",
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
