//! Typed failures for trusted-load validation of persistent equipment state.

use std::error::Error;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::maintenance::Condition;
use crate::material::{MaterialPhaseStateError, ParticleSizeStateError};
use crate::structural::StructuralElementId;

use super::super::super::definitions::EquipmentDefinitionId;
use super::super::EquipmentId;

mod display;

/// Structural or cross-reference failure in decoded persistent equipment state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentValidationError {
    SupportRevisionAfterRevision {
        support_revision: u64,
        revision: u64,
    },
    ZeroNextEquipmentId,
    ZeroEquipmentId,
    KeyIdMismatch {
        key: EquipmentId,
        record: EquipmentId,
    },
    NextEquipmentIdNotAboveAllocated {
        next: u32,
        highest: EquipmentId,
    },
    ZeroDefinitionId {
        equipment: EquipmentId,
    },
    ZeroSupportElementId {
        equipment: EquipmentId,
    },
    ZeroIndexedSupportElementId,
    ZeroIndexedEquipmentId {
        element: StructuralElementId,
    },
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    UnknownIndexedEquipment {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        equipment: EquipmentId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    EmbodiedMassMismatch {
        equipment: EquipmentId,
        stored: Mass,
        authored: Mass,
    },
    MissingAssemblyMaterial {
        equipment: EquipmentId,
    },
    UnexpectedAssemblyMaterial {
        equipment: EquipmentId,
    },
    ZeroEmbodiedTrace {
        equipment: EquipmentId,
    },
    EmbodiedTraceMassOverflow {
        equipment: EquipmentId,
    },
    EmbodiedTraceMassMismatch {
        equipment: EquipmentId,
        stored: Mass,
        traced: Mass,
    },
    UnknownEmbodiedCommodity {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
    },
    ImpureEmbodiedMaterial {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
    },
    InvalidEmbodiedPhaseState {
        equipment: EquipmentId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        equipment: EquipmentId,
        error: ParticleSizeStateError,
    },
    EmbodiedProvenanceInFuture {
        equipment: EquipmentId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    EmbodiedProvenanceAfterConstruction {
        equipment: EquipmentId,
        latest_created_at: SimulationTick,
        created_at: SimulationTick,
    },
    AssemblyMaterialMismatch {
        equipment: EquipmentId,
        commodity: crate::material::CommodityKey,
        stored: Mass,
        authored: Mass,
    },
    CreatedInFuture {
        equipment: EquipmentId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    MaintenanceAdmissionRevisionInvalid {
        equipment: EquipmentId,
        admission_revision: u64,
        current_revision: u64,
    },
    MaintenanceAdmissionTickInvalid {
        equipment: EquipmentId,
        admitted_at: SimulationTick,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    MaintenanceAdmissionProfileMissing {
        equipment: EquipmentId,
    },
    MaintenanceAdmissionOutcomeInvalid {
        equipment: EquipmentId,
        before: Condition,
        after: Condition,
        required: Condition,
    },
}

impl Error for EquipmentValidationError {}
