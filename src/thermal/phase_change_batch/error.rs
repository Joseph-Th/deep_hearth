//! Diagnostics for pure-material phase-change batch resolution.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Temperature;
use crate::material::{CommodityKey, FormId, MaterialId, MaterialLotSpecError, MaterialPhase};

use crate::thermal::{FusionHeatError, PhaseSensibleHeatError, SensibleHeatError};

/// Failure while resolving conserved matter and energy for a pure-material phase transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurePhaseChangeBatchError {
    EmptyInput,
    UnknownInputForm {
        form: FormId,
    },
    InputPhaseMismatch {
        form: FormId,
        expected: MaterialPhase,
        found: MaterialPhase,
    },
    InputFormNotAccepted {
        found: FormId,
    },
    ImpureInput {
        commodity: CommodityKey,
    },
    PureMaterialDoesNotMatchCommodity {
        commodity: CommodityKey,
        pure: MaterialId,
    },
    UnexpectedMaterial {
        expected: MaterialId,
        found: MaterialId,
    },
    InputTemperatureOutsidePhaseRange {
        material: MaterialId,
        phase: MaterialPhase,
        current: Temperature,
        melting_point: Temperature,
    },
    SensibleHeat {
        material: MaterialId,
        error: SensibleHeatError,
    },
    SolidCooling {
        material: MaterialId,
        error: PhaseSensibleHeatError,
    },
    FusionHeat {
        material: MaterialId,
        error: FusionHeatError,
    },
    EnergyOverflow,
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for PurePhaseChangeBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("phase-change batch contains no material"),
            Self::UnknownInputForm { form } => write!(
                formatter,
                "phase-change batch references unknown form {}",
                form.value()
            ),
            Self::InputPhaseMismatch {
                form,
                expected,
                found,
            } => write!(
                formatter,
                "phase-change input form {} is {found:?} rather than required {expected:?}",
                form.value(),
            ),
            Self::InputFormNotAccepted { found } => write!(
                formatter,
                "phase-change process does not accept selected input form {}",
                found.value()
            ),
            Self::ImpureInput { commodity } => write!(
                formatter,
                "phase-change input material {} in form {} is compositionally mixed; alloy phase diagrams are not modeled",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::PureMaterialDoesNotMatchCommodity { commodity, pure } => write!(
                formatter,
                "phase-change input material {} in form {} claims pure material {} instead",
                commodity.material().value(),
                commodity.form().value(),
                pure.value()
            ),
            Self::UnexpectedMaterial { expected, found } => write!(
                formatter,
                "phase-change process is authored for material {} but selected material {} was provided",
                expected.value(),
                found.value()
            ),
            Self::InputTemperatureOutsidePhaseRange {
                material,
                phase,
                current,
                melting_point,
            } => write!(
                formatter,
                "{phase:?} material {} at {} mK is on the wrong side of its {} mK melting point",
                material.value(),
                current.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::SensibleHeat { material, error } => write!(
                formatter,
                "material {} cannot reach its fusion boundary: {error}",
                material.value()
            ),
            Self::SolidCooling { material, error } => write!(
                formatter,
                "solid material {} cannot reach its authored casting output temperature: {error}",
                material.value()
            ),
            Self::FusionHeat { material, error } => write!(
                formatter,
                "material {} cannot resolve latent heat: {error}",
                material.value()
            ),
            Self::EnergyOverflow => formatter.write_str("phase-change energy overflowed"),
            Self::MassOverflow => formatter.write_str("phase-change batch mass overflowed"),
            Self::Output(error) => write!(
                formatter,
                "phase-change output construction failed: {error}"
            ),
        }
    }
}

impl Error for PurePhaseChangeBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat { error, .. } => Some(error),
            Self::SolidCooling { error, .. } => Some(error),
            Self::FusionHeat { error, .. } => Some(error),
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::UnknownInputForm { .. }
            | Self::InputPhaseMismatch { .. }
            | Self::InputFormNotAccepted { .. }
            | Self::ImpureInput { .. }
            | Self::PureMaterialDoesNotMatchCommodity { .. }
            | Self::UnexpectedMaterial { .. }
            | Self::InputTemperatureOutsidePhaseRange { .. }
            | Self::EnergyOverflow
            | Self::MassOverflow => None,
        }
    }
}
