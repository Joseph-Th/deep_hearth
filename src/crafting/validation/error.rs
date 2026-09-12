//! Typed failures for trusted-load replay of in-flight manual craft jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::MaterialLotSpecError;
use crate::ore_processing::MassFlowDurationError;
use crate::production::ProductionJobId;

/// Corruption or semantic drift in an in-flight manual shaping job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftJobValidationError {
    UnexpectedEnergy {
        job: ProductionJobId,
    },
    UnexpectedEquipment {
        job: ProductionJobId,
    },
    MissingRequiredEquipment {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
        definition: EquipmentDefinitionId,
    },
    MissingEquipmentCapability {
        job: ProductionJobId,
        capability: CapabilityId,
    },
    EquipmentCapabilityKindMismatch {
        job: ProductionJobId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    EquipmentDuration {
        job: ProductionJobId,
        error: MassFlowDurationError,
    },
    EquipmentCondition {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    EquipmentConditionMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    InputCommodityMismatch {
        job: ProductionJobId,
    },
    EmptyInput {
        job: ProductionJobId,
    },
    InputCompositionMismatch {
        job: ProductionJobId,
    },
    MixedInputTemperature {
        job: ProductionJobId,
    },
    InputMassNotWholeBatches {
        job: ProductionJobId,
        consumed: Mass,
        batch_mass: Mass,
    },
    DurationOverflow {
        job: ProductionJobId,
        batches: NonZeroU64,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    OutputMassOverflow {
        job: ProductionJobId,
        batches: NonZeroU64,
    },
    OutputConstruction {
        job: ProductionJobId,
        error: MaterialLotSpecError,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ManualCraftJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnergy { job } => write!(
                formatter,
                "manual craft job {} carries energy despite having no authored energy resource",
                job.value()
            ),
            Self::UnexpectedEquipment { job } => write!(
                formatter,
                "manual craft job {} carries equipment despite having no authored tool-assisted path",
                job.value()
            ),
            Self::MissingRequiredEquipment { job } => write!(
                formatter,
                "manual craft job {} omits equipment required by its authored shaping process",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job, definition } => write!(
                formatter,
                "manual craft job {} references unknown equipment definition {}",
                job.value(),
                definition.value()
            ),
            Self::MissingEquipmentCapability { job, capability } => write!(
                formatter,
                "manual craft job {} equipment no longer provides capability {}",
                job.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                job,
                capability,
                found,
            } => write!(
                formatter,
                "manual craft job {} equipment capability {} has {found:?} value instead of mass throughput",
                job.value(),
                capability.value()
            ),
            Self::EquipmentDuration { job, error } => write!(
                formatter,
                "manual craft job {} equipment throughput cannot reproduce its duration: {error}",
                job.value()
            ),
            Self::EquipmentCondition { job, error } => write!(
                formatter,
                "manual craft job {} equipment cannot reproduce its condition schedule: {error}",
                job.value()
            ),
            Self::EquipmentConditionMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "manual craft job {} stores equipment condition {} ppm but tool-assisted work requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::InputCommodityMismatch { job } => write!(
                formatter,
                "manual craft job {} consumed a commodity outside its authored hand recipe",
                job.value()
            ),
            Self::EmptyInput { job } => write!(
                formatter,
                "manual craft job {} has no consumed material",
                job.value()
            ),
            Self::InputCompositionMismatch { job } => write!(
                formatter,
                "manual craft job {} consumed non-pure material that its hand-shaping resolver cannot transform",
                job.value()
            ),
            Self::MixedInputTemperature { job } => write!(
                formatter,
                "manual craft job {} combines different input temperatures without thermal physics",
                job.value()
            ),
            Self::InputMassNotWholeBatches {
                job,
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "manual craft job {} consumed {} mg, which is not a whole number of {} mg authored batches",
                job.value(),
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::DurationOverflow { job, batches } => write!(
                formatter,
                "manual craft job {} repeats {} batches beyond the authoritative duration range",
                job.value(),
                batches.get()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "manual craft job {} stores {} active ticks but its repeated hand work requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::OutputMassOverflow { job, batches } => write!(
                formatter,
                "manual craft job {} output mass overflows when replaying {} batches",
                job.value(),
                batches.get()
            ),
            Self::OutputConstruction { job, error } => write!(
                formatter,
                "manual craft job {} authored output cannot be reconstructed: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "manual craft job {} output snapshot disagrees with authored shaping semantics",
                job.value()
            ),
        }
    }
}

impl Error for ManualCraftJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OutputConstruction { error, .. } => Some(error),
            Self::EquipmentDuration { error, .. } => Some(error),
            Self::EquipmentCondition { error, .. } => Some(error),
            Self::UnexpectedEnergy { .. }
            | Self::UnexpectedEquipment { .. }
            | Self::MissingRequiredEquipment { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::EquipmentConditionMismatch { .. }
            | Self::InputCommodityMismatch { .. }
            | Self::EmptyInput { .. }
            | Self::InputCompositionMismatch { .. }
            | Self::MixedInputTemperature { .. }
            | Self::InputMassNotWholeBatches { .. }
            | Self::DurationOverflow { .. }
            | Self::DurationMismatch { .. }
            | Self::OutputMassOverflow { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}
