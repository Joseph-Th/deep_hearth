//! Typed failures for trusted-load validation of persistent equipment state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{MaterialPhaseStateError, ParticleSizeStateError};
use crate::structural::StructuralElementId;

use super::super::super::definitions::EquipmentDefinitionId;
use super::super::EquipmentId;

/// Structural or cross-reference failure in decoded persistent equipment state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentValidationError {
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
    InvalidEmbodiedProvenanceRange {
        equipment: EquipmentId,
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
}

impl Display for EquipmentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextEquipmentId => {
                formatter.write_str("equipment next-id cursor must be nonzero")
            }
            Self::ZeroEquipmentId => formatter.write_str("equipment record id must be nonzero"),
            Self::KeyIdMismatch { key, record } => write!(
                formatter,
                "equipment map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroSupportElementId { equipment } => write!(
                formatter,
                "equipment {} references zero structural support id",
                equipment.value()
            ),
            Self::ZeroIndexedSupportElementId => {
                formatter.write_str("equipment support reverse index contains zero structural id")
            }
            Self::ZeroIndexedEquipmentId { element } => write!(
                formatter,
                "equipment support reverse index for element {} contains zero equipment id",
                element.value()
            ),
            Self::EmptySupportIndex { element } => write!(
                formatter,
                "equipment support reverse index contains empty entry for element {}",
                element.value()
            ),
            Self::MissingSupportIndex { equipment, element } => write!(
                formatter,
                "equipment {} references support element {} but is absent from the reverse index",
                equipment.value(),
                element.value()
            ),
            Self::UnknownIndexedEquipment { equipment, element } => write!(
                formatter,
                "equipment support reverse index element {} references missing equipment {}",
                element.value(),
                equipment.value()
            ),
            Self::SupportIndexMismatch {
                equipment,
                indexed,
                actual,
            } => write!(
                formatter,
                "equipment support reverse index places equipment {} on element {} but record support is {actual:?}",
                equipment.value(),
                indexed.value()
            ),
            Self::NextEquipmentIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "equipment next-id cursor {next} is not above allocated id {}",
                highest.value()
            ),
            Self::ZeroDefinitionId { equipment } => write!(
                formatter,
                "equipment {} has zero definition id",
                equipment.value()
            ),
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
            Self::EmbodiedMassMismatch {
                equipment,
                stored,
                authored,
            } => write!(
                formatter,
                "equipment {} owns {} mg but definition requires {} mg",
                equipment.value(),
                stored.milligrams(),
                authored.milligrams()
            ),
            Self::MissingAssemblyMaterial { equipment } => write!(
                formatter,
                "equipment {} has an authored assembly profile but no persisted embodied material",
                equipment.value()
            ),
            Self::UnexpectedAssemblyMaterial { equipment } => write!(
                formatter,
                "equipment {} persists assembled material but its definition has no assembly profile",
                equipment.value()
            ),
            Self::ZeroEmbodiedTrace { equipment } => write!(
                formatter,
                "equipment {} contains a zero-mass embodied material trace",
                equipment.value()
            ),
            Self::EmbodiedTraceMassOverflow { equipment } => write!(
                formatter,
                "equipment {} embodied material trace mass overflows",
                equipment.value()
            ),
            Self::EmbodiedTraceMassMismatch {
                equipment,
                stored,
                traced,
            } => write!(
                formatter,
                "equipment {} stores {} mg embodied mass but traces own {} mg",
                equipment.value(),
                stored.milligrams(),
                traced.milligrams()
            ),
            Self::UnknownEmbodiedCommodity {
                equipment,
                commodity,
            } => write!(
                formatter,
                "equipment {} embodied material references unknown commodity {}",
                equipment.value(),
                commodity.value()
            ),
            Self::ImpureEmbodiedMaterial {
                equipment,
                commodity,
            } => write!(
                formatter,
                "equipment {} embodied commodity {} is not pure authored material",
                equipment.value(),
                commodity.value()
            ),
            Self::InvalidEmbodiedPhaseState { equipment, error } => write!(
                formatter,
                "equipment {} contains embodied matter with invalid phase state: {error}",
                equipment.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { equipment, error } => write!(
                formatter,
                "equipment {} contains embodied matter with invalid particle-size state: {error}",
                equipment.value()
            ),
            Self::InvalidEmbodiedProvenanceRange { equipment } => write!(
                formatter,
                "equipment {} embodied material has an invalid provenance range",
                equipment.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                equipment,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "equipment {} embodied material provenance ends at tick {} after current tick {}",
                equipment.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::EmbodiedProvenanceAfterConstruction {
                equipment,
                latest_created_at,
                created_at,
            } => write!(
                formatter,
                "equipment {} embodied material provenance ends at tick {} after construction at tick {} without enough authored upgrade or component-replacement allowance",
                equipment.value(),
                latest_created_at.value(),
                created_at.value()
            ),
            Self::AssemblyMaterialMismatch {
                equipment,
                commodity,
                stored,
                authored,
            } => write!(
                formatter,
                "equipment {} owns {} mg of assembly commodity {} but definition requires {} mg",
                equipment.value(),
                stored.milligrams(),
                commodity.value(),
                authored.milligrams()
            ),
            Self::CreatedInFuture {
                equipment,
                created_at,
                current,
            } => write!(
                formatter,
                "equipment {} was created at tick {} after current tick {}",
                equipment.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for EquipmentValidationError {}
