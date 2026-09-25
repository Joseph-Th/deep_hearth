//! Display formatting for equipment trusted-load failures.

use std::fmt::{Display, Formatter};

use super::EquipmentValidationError;

impl Display for EquipmentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SupportRevisionAfterRevision {
                support_revision,
                revision,
            } => write!(
                formatter,
                "equipment support revision {support_revision} exceeds owner revision {revision}"
            ),
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
            Self::MaintenanceAdmissionRevisionInvalid {
                equipment,
                admission_revision,
                current_revision,
            } => write!(
                formatter,
                "equipment {} maintenance receipt revision {admission_revision} is invalid for current equipment revision {current_revision}",
                equipment.value()
            ),
            Self::MaintenanceAdmissionTickInvalid {
                equipment,
                admitted_at,
                created_at,
                current,
            } => write!(
                formatter,
                "equipment {} maintenance receipt tick {} is outside equipment lifetime {}..={}",
                equipment.value(),
                admitted_at.value(),
                created_at.value(),
                current.value()
            ),
            Self::MaintenanceAdmissionProfileMissing { equipment } => write!(
                formatter,
                "equipment {} persists a maintenance receipt but its definition has no maintenance profile",
                equipment.value()
            ),
            Self::MaintenanceAdmissionOutcomeInvalid {
                equipment,
                before,
                after,
                required,
            } => write!(
                formatter,
                "equipment {} maintenance receipt records {} -> {} ppm but authored service requires an improvement to {} ppm",
                equipment.value(),
                before.parts_per_million(),
                after.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
}
