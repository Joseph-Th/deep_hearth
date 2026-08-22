//! Finite-fluid drinking validation, accounting, and canonical commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateVolume, Volume};
use crate::core::state::AppState;
use crate::fluid::{
    FluidEgressCommitError, FluidEgressError, FluidStoreId, FluidStructuralLoadError,
    ValidatedFluidEgress, validate_fluid_egress,
};
use crate::labor::{
    PlayerAttentionError, PlayerWork, ValidatedPlayerAttention, validate_player_attention,
};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::super::state::{PlayerSurvivalRecord, player_record};

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
    ZeroVolume,
    InsufficientVolume {
        store: FluidStoreId,
        available: Volume,
        requested: Volume,
    },
    FluidRevisionExhausted,
    SurvivalRevisionExhausted,
    HydrationOverflow,
    NoHydrationGain {
        volume: Volume,
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
            Self::HydrationOverflow => {
                formatter.write_str("drink hydration calculation overflowed")
            }
            Self::NoHydrationGain { volume } => write!(
                formatter,
                "drink volume {} uL would not increase player hydration",
                volume.microliters()
            ),
            Self::ConsumedFluidOverflow => {
                formatter.write_str("consumed fluid accounting overflowed")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "drink withdrawal structural load failed: {error}"
            ),
        }
    }
}

impl Error for DrinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::PlayerBusy { active: _ }
            | Self::UnknownStore { store: _ }
            | Self::EmptyStore { store: _ }
            | Self::NotDrinkable
            | Self::ZeroVolume
            | Self::InsufficientVolume { .. }
            | Self::FluidRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::HydrationOverflow
            | Self::NoHydrationGain { .. }
            | Self::ConsumedFluidOverflow => None,
        }
    }
}

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
            Self::Structure(error) => write!(
                formatter,
                "validated drinking structural commit failed: {error}"
            ),
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
            | Self::FluidSourceChanged { store: _ } => None,
        }
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrinkOutcome {
    store: FluidStoreId,
    volume: Volume,
    hydration_gained: Volume,
}

impl DrinkOutcome {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }
    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }
    #[must_use]
    pub const fn hydration_gained(self) -> Volume {
        self.hydration_gained
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedDrink {
    attention: ValidatedPlayerAttention,
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedFluidEgress,
    after: PlayerSurvivalRecord,
    fluid: crate::fluid::FluidDefinitionId,
    next_consumed_volume: AggregateVolume,
    outcome: DrinkOutcome,
}

pub fn validate_drink(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    volume: Volume,
) -> Result<ValidatedDrink, DrinkError> {
    let attention = validate_player_attention(state).map_err(|error| match error {
        PlayerAttentionError::SurvivalNotInitialized => DrinkError::SurvivalNotInitialized,
        PlayerAttentionError::PlayerDead => DrinkError::PlayerDead,
        PlayerAttentionError::Busy { active } => DrinkError::PlayerBusy { active },
    })?;
    let Some(player) = state.survival().player().copied() else {
        return Err(DrinkError::SurvivalNotInitialized);
    };
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(DrinkError::UnknownStore { store })?;
    let contents = record.contents().ok_or(DrinkError::EmptyStore { store })?;
    let drink = registries
        .survival()
        .get_drink(contents.fluid())
        .copied()
        .ok_or(DrinkError::NotDrinkable)?;
    let egress =
        validate_fluid_egress(registries, state, store, volume).map_err(|error| match error {
            FluidEgressError::UnknownStore { store } => DrinkError::UnknownStore { store },
            FluidEgressError::EmptyStore { store } => DrinkError::EmptyStore { store },
            FluidEgressError::UnknownFluidDefinition { definition: _ } => DrinkError::NotDrinkable,
            FluidEgressError::ZeroVolume => DrinkError::ZeroVolume,
            FluidEgressError::InsufficientVolume {
                store,
                available,
                requested,
            } => DrinkError::InsufficientVolume {
                store,
                available,
                requested,
            },
            FluidEgressError::RevisionExhausted => DrinkError::FluidRevisionExhausted,
            FluidEgressError::StructuralLoad(error) => DrinkError::StructuralLoad(error),
        })?;
    let hydration_numerator = u128::from(egress.volume().microliters())
        .checked_mul(u128::from(drink.hydration_multiplier_ppm()))
        .ok_or(DrinkError::HydrationOverflow)?;
    let hydration_gain = u64::try_from(hydration_numerator / 1_000_000)
        .map_err(|_| DrinkError::HydrationOverflow)?;
    let hydration_gain = Volume::from_microliters(hydration_gain);
    if hydration_gain.is_zero() {
        return Err(DrinkError::NoHydrationGain { volume });
    }
    let physiology = registries.survival().physiology();
    let available_hydration = physiology
        .maximum_hydration()
        .checked_sub(player.hydration())
        .ok_or(DrinkError::HydrationOverflow)?;
    let hydration_gained = hydration_gain.min(available_hydration);
    if hydration_gained.is_zero() {
        return Err(DrinkError::NoHydrationGain { volume });
    }
    let hydration_after = player
        .hydration()
        .checked_add(hydration_gained)
        .ok_or(DrinkError::HydrationOverflow)?;
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(DrinkError::SurvivalRevisionExhausted)?;
    let next_consumed_volume = state
        .survival()
        .consumed_fluid_volume(contents.fluid())
        .checked_add(AggregateVolume::from_volume(volume))
        .ok_or(DrinkError::ConsumedFluidOverflow)?;
    Ok(ValidatedDrink {
        attention,
        expected_survival_revision,
        next_survival_revision,
        egress,
        after: player_record(
            player.metabolic_energy(),
            hydration_after,
            player.vitality(),
            player.nutrition(),
        ),
        fluid: contents.fluid(),
        next_consumed_volume,
        outcome: DrinkOutcome {
            store,
            volume,
            hydration_gained,
        },
    })
}

impl ValidatedDrink {
    pub fn commit(self, state: &mut AppState) -> Result<DrinkOutcome, DrinkCommitError> {
        if let Err(conflict) = self.attention.precheck(state) {
            return Err(DrinkCommitError::StalePlayerWorkRevision {
                expected: conflict.expected(),
                actual: conflict.actual(),
            });
        }
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(DrinkCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        self.egress.commit(state).map_err(|error| match error {
            FluidEgressCommitError::StaleRevision { expected, actual } => {
                DrinkCommitError::StaleFluidRevision { expected, actual }
            }
            FluidEgressCommitError::SourceChanged { store } => {
                DrinkCommitError::FluidSourceChanged { store }
            }
            FluidEgressCommitError::Structure(error) => DrinkCommitError::Structure(error),
        })?;
        state.survival_state_mut().apply_fluid_consumption(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.after,
            self.fluid,
            self.next_consumed_volume,
        );
        Ok(self.outcome)
    }
}
