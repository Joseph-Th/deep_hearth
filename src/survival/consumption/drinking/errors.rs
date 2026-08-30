//! Validation and commit failures for finite-fluid drinking.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::time::TickSpan;
use crate::fluid::{FluidStoreId, FluidStructuralLoadError};
use crate::labor::PlayerWork;
use crate::structural::StructuralCommitError;

/// Failure while validating finite-fluid drinking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrinkError {
    SurvivalNotInitialized,
    PlayerDead,
    PlayerBusy {
        active: PlayerWork,
    },
    UnknownStore {
        store: FluidStoreId,
    },
    EmptyStore {
        store: FluidStoreId,
    },
    NotDrinkable,
    TemperatureOutsideConsumptionRange {
        store: FluidStoreId,
        temperature: Temperature,
        minimum: Temperature,
        maximum: Temperature,
    },
    ZeroVolume,
    InsufficientVolume {
        store: FluidStoreId,
        available: Volume,
        requested: Volume,
    },
    FluidRevisionExhausted,
    SurvivalRevisionExhausted,
    PlayerWorkRevisionExhausted,
    CompletionTickOverflow {
        duration: TickSpan,
    },
    HydrationOverflow,
    NoHydrationGain {
        volume: Volume,
    },
    DrinkVolumeExceedsIntakeLimit {
        volume: Volume,
        maximum: Volume,
    },
    ConsumedFluidOverflow,
    StructuralLoad(FluidStructuralLoadError),
}

impl Display for DrinkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("player survival is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot drink"),
            Self::PlayerBusy { active } => {
                write!(
                    formatter,
                    "player cannot drink while occupied by {active:?}"
                )
            }
            Self::UnknownStore { store } => {
                write!(formatter, "unknown drink source store {}", store.value())
            }
            Self::EmptyStore { store } => {
                write!(formatter, "drink source store {} is empty", store.value())
            }
            Self::NotDrinkable => formatter.write_str("stored fluid is not authored as drinkable"),
            Self::TemperatureOutsideConsumptionRange {
                store,
                temperature,
                minimum,
                maximum,
            } => write!(
                formatter,
                "drink source {} at {} mK lies outside its direct-consumption range {}..={} mK",
                store.value(),
                temperature.millikelvin(),
                minimum.millikelvin(),
                maximum.millikelvin()
            ),
            Self::ZeroVolume => formatter.write_str("drink volume must be nonzero"),
            Self::InsufficientVolume {
                store,
                available,
                requested,
            } => write!(
                formatter,
                "drink source {} contains {} uL but {} uL was requested",
                store.value(),
                available.microliters(),
                requested.microliters()
            ),
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid revision space is exhausted")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::PlayerWorkRevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
            Self::CompletionTickOverflow { duration } => write!(
                formatter,
                "drink attention duration of {} ticks exceeds the simulation clock range",
                duration.value()
            ),
            Self::HydrationOverflow => {
                formatter.write_str("drink hydration calculation overflowed")
            }
            Self::NoHydrationGain { volume } => write!(
                formatter,
                "drink volume {} uL resolves to zero whole microliters of hydration",
                volume.microliters()
            ),
            Self::DrinkVolumeExceedsIntakeLimit { volume, maximum } => write!(
                formatter,
                "drink volume {} uL exceeds the direct-consumption limit of {} uL",
                volume.microliters(),
                maximum.microliters()
            ),
            Self::ConsumedFluidOverflow => {
                formatter.write_str("consumed fluid accounting overflowed")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "drink withdrawal structural load failed: {error}"
                )
            }
        }
    }
}

impl Error for DrinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::PlayerBusy { .. }
            | Self::UnknownStore { .. }
            | Self::EmptyStore { .. }
            | Self::NotDrinkable
            | Self::TemperatureOutsideConsumptionRange { .. }
            | Self::ZeroVolume
            | Self::InsufficientVolume { .. }
            | Self::FluidRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::PlayerWorkRevisionExhausted
            | Self::CompletionTickOverflow { .. }
            | Self::HydrationOverflow
            | Self::NoHydrationGain { .. }
            | Self::DrinkVolumeExceedsIntakeLimit { .. }
            | Self::ConsumedFluidOverflow => None,
        }
    }
}

/// Failure when a validated drinking action is committed against changed owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrinkCommitError {
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleFluidRevision { expected: u64, actual: u64 },
    FluidSourceChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
}

impl Display for DrinkCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "validated drinking expected player-work revision {expected} but current revision is {actual}"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated drinking expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "validated drinking expected fluid revision {expected} but current revision is {actual}"
            ),
            Self::FluidSourceChanged { store } => write!(
                formatter,
                "drink source store {} changed after validation",
                store.value()
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "validated drinking structural commit failed: {error}"
                )
            }
        }
    }
}

impl Error for DrinkCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StalePlayerWorkRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::StaleFluidRevision { .. }
            | Self::FluidSourceChanged { .. } => None,
        }
    }
}
