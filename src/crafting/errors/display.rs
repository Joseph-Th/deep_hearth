//! Human-readable diagnostics for public manual-crafting failures.

use std::fmt::{Display, Formatter};

use super::{
    ManualCraftCommitError, ManualCraftEquipmentProjectionError, ManualCraftError,
    ManualCraftHandProjectionError, StartManualCraftError,
};

impl Display for ManualCraftHandProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::EquipmentRequired { process } => write!(
                formatter,
                "manual craft process {} has no equipment-free hand-work route",
                process.value()
            ),
            Self::DurationOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} hand-work duration overflows for {} batches",
                process.value(),
                batches.get()
            ),
            Self::ResourceBudgetOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} physiological hand-work budget overflows for {} batches",
                process.value(),
                batches.get()
            ),
        }
    }
}

impl Display for ManualCraftEquipmentProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::EquipmentNotSupported { process } => write!(
                formatter,
                "manual craft process {} has no authored equipment-assisted path",
                process.value()
            ),
            Self::UnknownEquipmentDefinition { equipment } => write!(
                formatter,
                "manual craft projection references unknown equipment definition {}",
                equipment.value()
            ),
            Self::MissingEquipmentCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "equipment definition {} does not provide required shaping capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "equipment definition {} capability {} has {found:?} value instead of mass throughput",
                equipment.value(),
                capability.value()
            ),
            Self::InputMassOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} input mass overflows when projected for {} batches",
                process.value(),
                batches.get()
            ),
            Self::EquipmentDuration(error) => write!(
                formatter,
                "manual craft projection cannot schedule work: {error}"
            ),
            Self::EquipmentCondition(error) => write!(
                formatter,
                "manual craft projection cannot remain productive: {error}"
            ),
        }
    }
}

impl Display for ManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("manual crafting requires initialized player survival")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot perform manual crafting"),
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "manual craft input is invalid: {error}"),
            Self::EmptyInput => formatter.write_str("manual craft selection is empty"),
            Self::InputCommodityMismatch { expected } => write!(
                formatter,
                "manual craft selection contains matter other than authored material {} form {}",
                expected.material().value(),
                expected.form().value()
            ),
            Self::InputCompositionMismatch { expected } => write!(
                formatter,
                "manual craft selection for material {} form {} must be pure host material",
                expected.material().value(),
                expected.form().value()
            ),
            Self::MixedInputTemperature => formatter.write_str(
                "manual shaping cannot combine different input temperatures without thermal physics",
            ),
            Self::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "manual craft selection contains {} mg, which is not a whole number of {} mg authored batches",
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::DurationOverflow { batches } => write!(
                formatter,
                "manual shaping duration overflows when repeated {} times",
                batches.get()
            ),
            Self::RequiredEquipmentMissing { process } => write!(
                formatter,
                "manual craft process {} requires compatible physical equipment",
                process.value()
            ),
            Self::EquipmentNotSupported { process, equipment } => write!(
                formatter,
                "manual craft process {} has no authored equipment-assisted path for equipment {}",
                process.value(),
                equipment.value()
            ),
            Self::Equipment(error) => {
                write!(formatter, "manual craft equipment is unavailable: {error}")
            }
            Self::MissingEquipmentCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual craft equipment {} does not provide required shaping capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "manual craft equipment {} capability {} has {found:?} value instead of mass throughput",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentDuration(error) => write!(
                formatter,
                "manual craft equipment throughput cannot schedule work: {error}"
            ),
            Self::EquipmentCondition(error) => {
                write!(formatter, "manual craft equipment cannot remain productive: {error}")
            }
            Self::OutputMassOverflow {
                commodity,
                batches,
            } => write!(
                formatter,
                "manual shaping output material {} form {} overflows when repeated {} times",
                commodity.material().value(),
                commodity.form().value(),
                batches.get()
            ),
            Self::Output(error) => write!(formatter, "manual craft output is invalid: {error}"),
            Self::Resolution(error) => {
                write!(formatter, "manual craft resolution is invalid: {error}")
            }
        }
    }
}

impl Display for StartManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(formatter, "manual craft resolution failed: {error}"),
            Self::Process(error) => write!(formatter, "manual craft start failed: {error}"),
            Self::Work(error) => write!(formatter, "manual craft labor is unavailable: {error}"),
        }
    }
}

impl Display for ManualCraftCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => {
                write!(formatter, "manual craft process commit failed: {error}")
            }
            Self::Work(error) => write!(formatter, "manual craft labor commit failed: {error}"),
        }
    }
}
