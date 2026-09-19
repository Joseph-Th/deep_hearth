//! Failure vocabulary for structural trusted-load validation.

use crate::core::quantity::{Force, Mass};
use crate::core::time::SimulationTick;
use crate::material::{FormId, MaterialId, MaterialPhaseStateError, ParticleSizeStateError};

use super::super::super::definitions::StructuralProfileId;
use super::super::super::geometry::StructuralGeometryError;
use super::super::{StructuralElementId, StructuralLifecycle, StructuralLoadKind};

mod display;
mod source;

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
