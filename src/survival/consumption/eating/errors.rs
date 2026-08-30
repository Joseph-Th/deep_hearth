//! Validation and commit failures for exact food consumption.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::TickSpan;
use crate::inventory::{MaterialLotId, StockpileId, StockpileStructuralLoadError};
use crate::labor::PlayerWork;
use crate::material::{CommodityKey, MaterialId};
use crate::structural::StructuralCommitError;

/// Failure while validating one exact eating action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatError {
    SurvivalNotInitialized,
    PlayerDead,
    PlayerBusy {
        active: PlayerWork,
    },
    EmptySelection,
    DuplicateLot {
        lot: MaterialLotId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownLot {
        lot: MaterialLotId,
    },
    LotOwnedElsewhere {
        lot: MaterialLotId,
        requested_source: StockpileId,
        actual_source: StockpileId,
    },
    ZeroMass {
        lot: MaterialLotId,
    },
    InsufficientLotMass {
        lot: MaterialLotId,
        available: Mass,
        requested: Mass,
    },
    InventoryMassOverflow {
        stockpile: StockpileId,
    },
    NotEdible {
        commodity: CommodityKey,
    },
    TemperatureOutsideConsumptionRange {
        lot: MaterialLotId,
        temperature: Temperature,
        minimum: Temperature,
        maximum: Temperature,
    },
    Spoiled {
        lot: MaterialLotId,
        age: TickSpan,
    },
    ShelfLifeOverflow,
    MetabolicEnergyOverflow,
    HydrationOverflow,
    NutritionOverflow,
    MealMassExceedsIntakeLimit {
        mass: Mass,
        maximum: Mass,
    },
    UnsupportedComposition {
        lot: MaterialLotId,
        material: MaterialId,
    },
    ConsumedMatterOverflow {
        material: MaterialId,
    },
    InventoryRevisionExhausted,
    SurvivalRevisionExhausted,
    PlayerWorkRevisionExhausted,
    CompletionTickOverflow {
        duration: TickSpan,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("player survival is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot eat"),
            Self::PlayerBusy { active } => {
                write!(formatter, "player cannot eat while occupied by {active:?}")
            }
            Self::EmptySelection => formatter.write_str("eating selection must not be empty"),
            Self::DuplicateLot { lot } => write!(
                formatter,
                "food lot {} is selected more than once in one meal",
                lot.value()
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "unknown food source stockpile {}",
                stockpile.value()
            ),
            Self::UnknownLot { lot } => write!(formatter, "unknown food lot {}", lot.value()),
            Self::LotOwnedElsewhere {
                lot,
                requested_source,
                actual_source,
            } => write!(
                formatter,
                "food lot {} is in stockpile {} rather than requested stockpile {}",
                lot.value(),
                actual_source.value(),
                requested_source.value()
            ),
            Self::ZeroMass { lot } => write!(
                formatter,
                "food lot {} selection has zero mass",
                lot.value()
            ),
            Self::InsufficientLotMass {
                lot,
                available,
                requested,
            } => write!(
                formatter,
                "food lot {} contains {} mg but {} mg was requested",
                lot.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::InventoryMassOverflow { stockpile } => write!(
                formatter,
                "food consumption mass accounting overflowed stockpile {}",
                stockpile.value()
            ),
            Self::NotEdible { commodity } => write!(
                formatter,
                "material {} form {} is not authored as edible",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::TemperatureOutsideConsumptionRange {
                lot,
                temperature,
                minimum,
                maximum,
            } => write!(
                formatter,
                "food lot {} at {} mK lies outside its direct-consumption range {}..={} mK",
                lot.value(),
                temperature.millikelvin(),
                minimum.millikelvin(),
                maximum.millikelvin()
            ),
            Self::Spoiled { lot, age } => write!(
                formatter,
                "food lot {} is spoiled after {} ticks",
                lot.value(),
                age.value()
            ),
            Self::ShelfLifeOverflow => {
                formatter.write_str("food shelf-life calculation overflowed")
            }
            Self::MetabolicEnergyOverflow => {
                formatter.write_str("food metabolic-energy calculation overflowed")
            }
            Self::HydrationOverflow => formatter.write_str("food hydration calculation overflowed"),
            Self::NutritionOverflow => formatter.write_str("food nutrition calculation overflowed"),
            Self::MealMassExceedsIntakeLimit { mass, maximum } => write!(
                formatter,
                "meal mass {} mg exceeds the direct-consumption limit of {} mg",
                mass.milligrams(),
                maximum.milligrams()
            ),
            Self::UnsupportedComposition { lot, material } => write!(
                formatter,
                "food lot {} is not pure material {} and cannot enter the current survival-consumption boundary",
                lot.value(),
                material.value()
            ),
            Self::ConsumedMatterOverflow { material } => write!(
                formatter,
                "consumed food matter accounting overflowed material {}",
                material.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::PlayerWorkRevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
            Self::CompletionTickOverflow { duration } => write!(
                formatter,
                "meal attention duration of {} ticks exceeds the simulation clock range",
                duration.value()
            ),
            Self::StructuralLoad(error) => {
                write!(formatter, "food withdrawal structural load failed: {error}")
            }
        }
    }
}

impl Error for EatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::PlayerBusy { .. }
            | Self::EmptySelection
            | Self::DuplicateLot { .. }
            | Self::UnknownStockpile { .. }
            | Self::UnknownLot { .. }
            | Self::LotOwnedElsewhere { .. }
            | Self::ZeroMass { .. }
            | Self::InsufficientLotMass { .. }
            | Self::InventoryMassOverflow { .. }
            | Self::NotEdible { .. }
            | Self::TemperatureOutsideConsumptionRange { .. }
            | Self::Spoiled { .. }
            | Self::ShelfLifeOverflow
            | Self::MetabolicEnergyOverflow
            | Self::HydrationOverflow
            | Self::NutritionOverflow
            | Self::MealMassExceedsIntakeLimit { .. }
            | Self::UnsupportedComposition { .. }
            | Self::ConsumedMatterOverflow { .. }
            | Self::InventoryRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::PlayerWorkRevisionExhausted
            | Self::CompletionTickOverflow { .. } => None,
        }
    }
}

/// Failure when a validated eating action is committed against changed owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatCommitError {
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EatCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "validated eating expected player-work revision {expected} but current revision is {actual}"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated eating expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated eating expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated eating structural commit failed: {error}"
            ),
        }
    }
}

impl Error for EatCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StalePlayerWorkRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::StaleInventoryRevision { .. } => None,
        }
    }
}
