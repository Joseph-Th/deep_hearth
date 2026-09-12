//! Typed trusted-load failures for geological deposit ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, MaterialId, MaterialPhase, MaterialPhaseStateError,
};

use super::super::GeologicalDepositId;

/// Persistent-state validation failure for geological matter ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeologyValidationError {
    ZeroNextDepositId,
    NextIdNotAfterExisting {
        next: u32,
        highest: GeologicalDepositId,
    },
    ZeroDepositId,
    IdMismatch {
        key: GeologicalDepositId,
        record: GeologicalDepositId,
    },
    ZeroInitialMass {
        deposit: GeologicalDepositId,
    },
    ZeroExcavationHardness {
        deposit: GeologicalDepositId,
    },
    RemainingMassExceedsInitial {
        deposit: GeologicalDepositId,
        initial: Mass,
        remaining: Mass,
    },
    AvailableWithoutMass {
        deposit: GeologicalDepositId,
    },
    DepletedWithRemainingMass {
        deposit: GeologicalDepositId,
        remaining: Mass,
    },
    InvalidComposition {
        deposit: GeologicalDepositId,
        error: CompositionError,
    },
    CompositionMissingHost {
        deposit: GeologicalDepositId,
        host: MaterialId,
    },
    UnknownCommodityMaterial {
        deposit: GeologicalDepositId,
        material: MaterialId,
    },
    UnknownCommodityForm {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
    },
    UnsupportedCommodity {
        deposit: GeologicalDepositId,
        commodity: CommodityKey,
    },
    UnsupportedCommodityPhase {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
        phase: MaterialPhase,
    },
    UnsupportedCommodityParticulateForm {
        deposit: GeologicalDepositId,
        form: crate::material::FormId,
    },
    InvalidPhaseState {
        deposit: GeologicalDepositId,
        error: MaterialPhaseStateError,
    },
    UnknownCompositionMaterial {
        deposit: GeologicalDepositId,
        material: MaterialId,
    },
    GeneratedInFuture {
        deposit: GeologicalDepositId,
        generated_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for GeologyValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextDepositId => {
                formatter.write_str("next geological deposit id must not be zero")
            }
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next geological deposit id {next} is not after existing id {}",
                highest.value()
            ),
            Self::ZeroDepositId => formatter.write_str("geological deposit id must not be zero"),
            Self::IdMismatch { key, record } => write!(
                formatter,
                "geological deposit map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroInitialMass { deposit } => write!(
                formatter,
                "geological deposit {} has zero initial mass",
                deposit.value()
            ),
            Self::ZeroExcavationHardness { deposit } => write!(
                formatter,
                "geological deposit {} has zero excavation hardness",
                deposit.value()
            ),
            Self::RemainingMassExceedsInitial {
                deposit,
                initial,
                remaining,
            } => write!(
                formatter,
                "geological deposit {} has {} mg remaining above initial {} mg",
                deposit.value(),
                remaining.milligrams(),
                initial.milligrams()
            ),
            Self::AvailableWithoutMass { deposit } => write!(
                formatter,
                "available geological deposit {} has no remaining mass",
                deposit.value()
            ),
            Self::DepletedWithRemainingMass { deposit, remaining } => write!(
                formatter,
                "depleted geological deposit {} still owns {} mg",
                deposit.value(),
                remaining.milligrams()
            ),
            Self::InvalidComposition { deposit, error } => write!(
                formatter,
                "geological deposit {} has invalid composition: {error}",
                deposit.value()
            ),
            Self::CompositionMissingHost { deposit, host } => write!(
                formatter,
                "geological deposit {} composition omits host material {}",
                deposit.value(),
                host.value()
            ),
            Self::UnknownCommodityMaterial { deposit, material } => write!(
                formatter,
                "geological deposit {} references unknown host material {}",
                deposit.value(),
                material.value()
            ),
            Self::UnknownCommodityForm { deposit, form } => write!(
                formatter,
                "geological deposit {} references unknown form {}",
                deposit.value(),
                form.value()
            ),
            Self::UnsupportedCommodity { deposit, commodity } => write!(
                formatter,
                "geological deposit {} uses unauthored material {} form {}",
                deposit.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnsupportedCommodityParticulateForm { deposit, form } => write!(
                formatter,
                "geological deposit {} uses particulate form {}; natural geological ownership does not carry processed particle-size state",
                deposit.value(),
                form.value()
            ),
            Self::UnsupportedCommodityPhase {
                deposit,
                form,
                phase,
            } => write!(
                formatter,
                "geological deposit {} uses {phase:?} form {}; finite geological deposits must be solid",
                deposit.value(),
                form.value()
            ),
            Self::InvalidPhaseState { deposit, error } => write!(
                formatter,
                "geological deposit {} has invalid material phase state: {error}",
                deposit.value()
            ),
            Self::UnknownCompositionMaterial { deposit, material } => write!(
                formatter,
                "geological deposit {} composition references unknown material {}",
                deposit.value(),
                material.value()
            ),
            Self::GeneratedInFuture {
                deposit,
                generated_at,
                current,
            } => write!(
                formatter,
                "geological deposit {} was generated at tick {} after current tick {}",
                deposit.value(),
                generated_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for GeologyValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition { error, .. } => Some(error),
            Self::InvalidPhaseState { error, .. } => Some(error),
            Self::ZeroNextDepositId
            | Self::NextIdNotAfterExisting { .. }
            | Self::ZeroDepositId
            | Self::IdMismatch { .. }
            | Self::ZeroInitialMass { .. }
            | Self::ZeroExcavationHardness { .. }
            | Self::RemainingMassExceedsInitial { .. }
            | Self::AvailableWithoutMass { .. }
            | Self::DepletedWithRemainingMass { .. }
            | Self::CompositionMissingHost { .. }
            | Self::UnknownCommodityMaterial { .. }
            | Self::UnknownCommodityForm { .. }
            | Self::UnsupportedCommodity { .. }
            | Self::UnsupportedCommodityPhase { .. }
            | Self::UnsupportedCommodityParticulateForm { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::GeneratedInFuture { .. } => None,
        }
    }
}
