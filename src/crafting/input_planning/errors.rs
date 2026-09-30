//! Familiar manual-craft input-planning errors.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::{Mass, Temperature};
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
    SplitTemperatureInput {
        input: CommodityKey,
        available: Mass,
        largest_compatible: Mass,
        required: Mass,
    },
    MultipleCompatibleInputTemperatures {
        input: CommodityKey,
        required: Mass,
        temperatures: Vec<Temperature>,
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
            Self::SplitTemperatureInput {
                input,
                available,
                largest_compatible,
                required,
            } => write!(
                formatter,
                "manual craft has {} mg of material {} form {} in total, but only {} mg shares one temperature and {} mg is required",
                available.milligrams(),
                input.material().value(),
                input.form().value(),
                largest_compatible.milligrams(),
                required.milligrams()
            ),
            Self::MultipleCompatibleInputTemperatures {
                input,
                required,
                temperatures,
            } => write!(
                formatter,
                "manual craft has {} separate temperature cohorts of material {} form {} that can each supply the required {} mg; choose an input stack explicitly",
                temperatures.len(),
                input.material().value(),
                input.form().value(),
                required.milligrams()
            ),
        }
    }
}

impl Error for ManualCraftInputPlanError {}
