//! Finite-fluid drinking validation, accounting, and canonical commit.

mod errors;
mod projection;

pub use errors::{DrinkCommitError, DrinkError};
pub use projection::{
    DrinkHydrationProjectionError, MinimumDrinkHydrationProjection,
    project_minimum_drink_to_hydration_target,
};

use crate::core::quantity::{AggregateVolume, Volume};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::fluid::{
    FluidEgressCommitError, FluidEgressError, FluidStoreId, ValidatedFluidEgress,
    validate_fluid_egress,
};
use crate::labor::{
    DrinkingWork, PlayerAttentionError, PlayerWork, ValidatedPlayerAttentionHold,
    validate_player_attention,
};
use crate::registry::Registries;

use super::super::state::PendingDrinking;
use super::direct_consumption_survival_revisions;

pub(super) fn pending_drink_hydration_offer(
    registries: &Registries,
    fluid: crate::fluid::FluidDefinitionId,
    volume: Volume,
) -> Volume {
    let drink = registries.survival().get_drink(fluid).unwrap_or_else(|| {
        panic!(
            "runtime invariant broken: pending drinking references non-drinkable fluid {}",
            fluid.value()
        )
    });
    drink.hydration_offer(volume)
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrinkOutcome {
    store: FluidStoreId,
    volume: Volume,
    hydration_offered: Volume,
    completes_at: SimulationTick,
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
    pub const fn hydration_offered(self) -> Volume {
        self.hydration_offered
    }
    /// Returns the authoritative tick when this admitted drink finishes releasing attention.
    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedDrink {
    attention: ValidatedPlayerAttentionHold,
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedFluidEgress,
    pending: PendingDrinking,
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
    let consumption_temperature = drink.consumption_temperature();
    if !consumption_temperature.contains(contents.temperature()) {
        return Err(DrinkError::TemperatureOutsideConsumptionRange {
            store,
            temperature: contents.temperature(),
            minimum: consumption_temperature.minimum(),
            maximum: consumption_temperature.maximum(),
        });
    }
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player attention requires survival state"));
    if player.hydration() > physiology.maximum_hydration() {
        return Err(DrinkError::HydrationOverflow);
    }
    let direct_consumption = physiology.direct_consumption();
    if volume.is_zero() {
        return Err(DrinkError::ZeroVolume);
    }
    let minimum_drink_volume = direct_consumption.minimum_drink_volume();
    if volume < minimum_drink_volume {
        return Err(DrinkError::DrinkVolumeBelowIntakeMinimum {
            volume,
            minimum: minimum_drink_volume,
        });
    }
    let maximum_drink_volume = direct_consumption.maximum_drink_volume();
    if volume > maximum_drink_volume {
        return Err(DrinkError::DrinkVolumeExceedsIntakeLimit {
            volume,
            maximum: maximum_drink_volume,
        });
    }
    let duration = direct_consumption
        .drink_duration(volume)
        .unwrap_or_else(|| unreachable!("validated drink volume is inside authored bounds"));
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(DrinkError::CompletionTickOverflow { duration })?;
    let attention = attention
        .hold(PlayerWork::Drinking {
            work: DrinkingWork::new(volume, state.tick(), completes_at),
        })
        .ok_or(DrinkError::PlayerWorkRevisionExhausted)?;
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
    let hydration_gain = drink.hydration_offer(egress.volume());
    if hydration_gain.is_zero() {
        return Err(DrinkError::NoHydrationGain { volume });
    }
    let egress_volume = egress.volume();
    let (expected_survival_revision, next_survival_revision) =
        direct_consumption_survival_revisions(state, duration)
            .ok_or(DrinkError::SurvivalRevisionExhausted)?;
    let consumed_before = state.survival().consumed_fluid_volume(contents.fluid());
    let next_consumed_volume = consumed_before
        .checked_add(AggregateVolume::from_volume(egress_volume))
        .ok_or(DrinkError::ConsumedFluidOverflow)?;
    Ok(ValidatedDrink {
        attention,
        expected_survival_revision,
        next_survival_revision,
        egress,
        pending: PendingDrinking::new(
            contents.fluid(),
            egress_volume,
            consumed_before,
            contents.temperature(),
            state.tick(),
            completes_at,
        ),
        fluid: contents.fluid(),
        next_consumed_volume,
        outcome: DrinkOutcome {
            store,
            volume: egress_volume,
            hydration_offered: hydration_gain,
            completes_at,
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
        state.survival().assert_direct_consumption_begin_available(
            self.expected_survival_revision,
            self.next_survival_revision,
        );
        self.egress.commit(state).map_err(|error| match error {
            FluidEgressCommitError::StaleRevision { expected, actual } => {
                DrinkCommitError::StaleFluidRevision { expected, actual }
            }
            FluidEgressCommitError::SourceChanged { store } => {
                DrinkCommitError::FluidSourceChanged { store }
            }
            FluidEgressCommitError::Structure(error) => DrinkCommitError::Structure(error),
        })?;
        state.survival_state_mut().begin_fluid_consumption(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.pending,
            self.fluid,
            self.next_consumed_volume,
        );
        self.attention.apply(state);
        Ok(self.outcome)
    }
}
