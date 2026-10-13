//! Familiar manual-craft input-planning errors.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::inventory::StockpileId;
use crate::material::CommodityKey;
use crate::production::ProcessId;

/// Failure while turning a familiar recipe/batch choice into exact lot selections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftInputPlanError {
    UnknownManualProcess {
        process: ProcessId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    AgeSensitiveInputRequiresExplicitSelection {
        input: CommodityKey,
    },
    InputMassOverflow {
        process: ProcessId,
        batches: NonZeroU64,
    },
    InsufficientInput {
        input: CommodityKey,
        available: Mass,
        required: Mass,
    },
}

impl Display for ManualCraftInputPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "unknown manual-craft source stockpile {}",
                stockpile.value()
            ),
            Self::AgeSensitiveInputRequiresExplicitSelection { input } => write!(
                formatter,
                "manual craft input material {} form {} is age-sensitive and requires an explicit stack choice",
                input.material().value(),
                input.form().value()
            ),
            Self::InputMassOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} input mass overflows for {} batches",
                process.value(),
                batches.get()
            ),
            Self::InsufficientInput {
                input,
                available,
                required,
            } => write!(
                formatter,
                "manual craft needs {} mg of material {} form {} but only {} mg of compatible input is available",
                required.milligrams(),
                input.material().value(),
                input.form().value(),
                available.milligrams()
            ),
        }
    }
}

impl Error for ManualCraftInputPlanError {}
