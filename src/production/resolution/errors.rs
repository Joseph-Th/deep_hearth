//! Public errors for operation-specific process resolution.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionError, MaterialId};

use super::ProcessOutputStreamId;

/// Invalid operation-specific output plan produced by a physical resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessResolutionError {
    ZeroDuration,
    NoOutputs,
    ZeroOutputStreamId,
    DuplicateOutputStreamId {
        stream: ProcessOutputStreamId,
    },
    EmptyOutputStream,
    ZeroOutputMass {
        commodity: CommodityKey,
    },
    InvalidOutputComposition {
        commodity: CommodityKey,
        error: CompositionError,
    },
    OutputCompositionMissingHost {
        commodity: CommodityKey,
        host: MaterialId,
    },
    DuplicateOutputSpecification {
        commodity: CommodityKey,
    },
    OutputMassOverflow,
    MatterBalanceMismatch {
        input_mass: Mass,
        output_mass: Mass,
    },
    EquipmentConditionImproved {
        before: Condition,
        after: Condition,
    },
}

impl Display for ProcessResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDuration => formatter.write_str("resolved process duration must be nonzero"),
            Self::NoOutputs => formatter.write_str("resolved process must own output matter"),
            Self::ZeroOutputStreamId => {
                formatter.write_str("resolved process output stream id must be nonzero")
            }
            Self::DuplicateOutputStreamId { stream } => write!(
                formatter,
                "resolved process contains duplicate output stream id {}",
                stream.value()
            ),
            Self::EmptyOutputStream => {
                formatter.write_str("resolved process output stream must own material")
            }
            Self::ZeroOutputMass { commodity } => write!(
                formatter,
                "resolved output material {} form {} has zero mass",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InvalidOutputComposition { commodity, error } => write!(
                formatter,
                "resolved output material {} form {} has invalid composition: {error}",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputCompositionMissingHost { commodity, host } => write!(
                formatter,
                "resolved output material {} form {} composition omits host material {}",
                commodity.material().value(),
                commodity.form().value(),
                host.value()
            ),
            Self::DuplicateOutputSpecification { commodity } => write!(
                formatter,
                "resolved output repeats material {} form {} with identical physical state",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputMassOverflow => {
                formatter.write_str("resolved process output mass overflows authoritative storage")
            }
            Self::MatterBalanceMismatch {
                input_mass,
                output_mass,
            } => write!(
                formatter,
                "resolved process accounts for {} mg of output from {} mg of input",
                output_mass.milligrams(),
                input_mass.milligrams()
            ),
            Self::EquipmentConditionImproved { before, after } => write!(
                formatter,
                "production operation cannot improve equipment condition from {} ppm to {} ppm",
                before.parts_per_million(),
                after.parts_per_million()
            ),
        }
    }
}

impl Error for ProcessResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOutputComposition { error, .. } => Some(error),
            Self::ZeroDuration
            | Self::NoOutputs
            | Self::ZeroOutputStreamId
            | Self::DuplicateOutputStreamId { .. }
            | Self::EmptyOutputStream
            | Self::ZeroOutputMass { .. }
            | Self::OutputCompositionMissingHost { .. }
            | Self::DuplicateOutputSpecification { .. }
            | Self::OutputMassOverflow
            | Self::MatterBalanceMismatch { .. }
            | Self::EquipmentConditionImproved { .. } => None,
        }
    }
}
