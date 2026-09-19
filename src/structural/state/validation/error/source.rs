//! Nested error-source routing for trusted structural-state validation failures.

use std::error::Error;

use super::StructureValidationError;

impl Error for StructureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry { error, .. } => Some(error),
            Self::InvalidEmbodiedPhaseState { error, .. } => Some(error),
            Self::InvalidEmbodiedParticleSizeState { error, .. } => Some(error),
            Self::NextElementIdNotAboveAllocated { .. }
            | Self::ElementKeyMismatch { .. }
            | Self::UnknownProfile { .. }
            | Self::UnknownMaterial { .. }
            | Self::NonStructuralMaterial { .. }
            | Self::ZeroCrossSection { .. }
            | Self::ZeroLength { .. }
            | Self::EmbodiedMassGeometryMismatch { .. }
            | Self::UnmaterializedLoadBearingElement { .. }
            | Self::EmbodiedMassOverflow { .. }
            | Self::ZeroEmbodiedTrace { .. }
            | Self::EmbodiedMaterialMismatch { .. }
            | Self::UnsupportedEmbodiedComposition { .. }
            | Self::UnknownEmbodiedCommodity { .. }
            | Self::UnconsolidatedEmbodiedForm { .. }
            | Self::EmbodiedProvenanceInFuture { .. }
            | Self::SelfWeightOverflow { .. }
            | Self::SelfWeightMismatch { .. }
            | Self::ZeroLoadContribution { .. }
            | Self::CreatedInFuture { .. }
            | Self::PlannedElementCracked { .. }
            | Self::FailedElementNotCracked { .. }
            | Self::MissingSupportIndex { .. }
            | Self::OrphanSupportIndex { .. }
            | Self::UnknownSupportReference { .. }
            | Self::SelfSupport { .. }
            | Self::SupportOutOfContact { .. }
            | Self::GroundedElementHasSupport { .. }
            | Self::ReverseIndexMismatch { .. }
            | Self::SupportCycle { .. }
            | Self::ActiveElementUnsupported { .. }
            | Self::ZeroNextElementId
            | Self::ZeroElementId => None,
        }
    }
}
