//! Finite-fluid drinking validation, accounting, and canonical commit.

mod errors;
mod projection;

use crate::logistics::validate_player_fluid_store_access;
pub use errors::{DrinkCommitError, DrinkError};
pub use projection::{
    DrinkHydrationProjectionError, MinimumDrinkHydrationProjection,
    project_minimum_drink_to_hydration_target,
};

use crate::core::quantity::{AggregateVolume, Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::fluid::{
    FluidDefinitionId, FluidEgressCommitError, FluidEgressError, FluidStoreId,
    ValidatedFluidEgress, validate_fluid_egress,
};
use crate::labor::{
    DrinkingWork, PlayerAttentionError, PlayerWork, ValidatedPlayerAttentionHold,
    validate_player_attention,
};
use crate::registry::Registries;
use crate::survival::{DirectConsumptionDefinition, DrinkDefinition};

use super::super::state::PendingDrinking;
use super::direct_consumption_survival_revisions;

#[derive(Clone, Copy)]
struct ResolvedDrinkSource {
    fluid: FluidDefinitionId,
    temperature: Temperature,
    definition: DrinkDefinition,
}

fn map_player_attention_error(error: PlayerAttentionError) -> DrinkError {
    match error {
        PlayerAttentionError::SurvivalNotInitialized => DrinkError::SurvivalNotInitialized,
        PlayerAttentionError::PlayerDead => DrinkError::PlayerDead,
        PlayerAttentionError::Busy { active } => DrinkError::PlayerBusy { active },
    }
}

fn resolve_drink_source(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> Result<ResolvedDrinkSource, DrinkError> {
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(DrinkError::UnknownStore { store })?;
    let contents = record.contents().ok_or(DrinkError::EmptyStore { store })?;
    let definition = registries
        .survival()
        .get_drink(contents.fluid())
        .copied()
        .ok_or(DrinkError::NotDrinkable)?;
    let consumption_temperature = definition.consumption_temperature();
    if !consumption_temperature.contains(contents.temperature()) {
        return Err(DrinkError::TemperatureOutsideConsumptionRange {
            store,
            temperature: contents.temperature(),
            minimum: consumption_temperature.minimum(),
            maximum: consumption_temperature.maximum(),
        });
    }
    Ok(ResolvedDrinkSource {
        fluid: contents.fluid(),
        temperature: contents.temperature(),
        definition,
    })
}

fn validate_drink_duration(
    direct_consumption: DirectConsumptionDefinition,
    volume: Volume,
) -> Result<TickSpan, DrinkError> {
    if volume.is_zero() {
        return Err(DrinkError::ZeroVolume);
    }
    let minimum = direct_consumption.minimum_drink_volume();
    if volume < minimum {
        return Err(DrinkError::DrinkVolumeBelowIntakeMinimum { volume, minimum });
    }
    let maximum = direct_consumption.maximum_drink_volume();
    if volume > maximum {
        return Err(DrinkError::DrinkVolumeExceedsIntakeLimit { volume, maximum });
    }
    Ok(direct_consumption
        .drink_duration(volume)
        .unwrap_or_else(|| unreachable!("validated drink volume is inside authored bounds")))
}

fn map_fluid_egress_error(error: FluidEgressError) -> DrinkError {
    match error {
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
    }
}

pub(super) fn pending_drink_hydration_offer(
    registries: &Registries,
    fluid: FluidDefinitionId,
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
    expected_logistics_revision: u64,
    egress: ValidatedFluidEgress,
    pending: PendingDrinking,
    fluid: FluidDefinitionId,
    next_consumed_volume: AggregateVolume,
    outcome: DrinkOutcome,
}

pub fn validate_drink(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    volume: Volume,
) -> Result<ValidatedDrink, DrinkError> {
    let attention = validate_player_attention(state).map_err(map_player_attention_error)?;
    let source = resolve_drink_source(registries, state, store)?;
    validate_player_fluid_store_access(state, store).map_err(DrinkError::Access)?;
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player attention requires survival state"));
    if player.hydration() > physiology.maximum_hydration() {
        return Err(DrinkError::HydrationOverflow);
    }
    let duration = validate_drink_duration(physiology.direct_consumption(), volume)?;
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
        validate_fluid_egress(registries, state, store, volume).map_err(map_fluid_egress_error)?;
    let hydration_gain = source.definition.hydration_offer(egress.volume());
    if hydration_gain.is_zero() {
        return Err(DrinkError::NoHydrationGain { volume });
    }
    let egress_volume = egress.volume();
    let (expected_survival_revision, next_survival_revision) =
        direct_consumption_survival_revisions(state, duration)
            .ok_or(DrinkError::SurvivalRevisionExhausted)?;
    let consumed_before = state.survival().consumed_fluid_volume(source.fluid);
    let next_consumed_volume = consumed_before
        .checked_add(AggregateVolume::from_volume(egress_volume))
        .ok_or(DrinkError::ConsumedFluidOverflow)?;
    Ok(ValidatedDrink {
        attention,
        expected_survival_revision,
        next_survival_revision,
        expected_logistics_revision: state.logistics().revision(),
        egress,
        pending: PendingDrinking::new(
            source.fluid,
            egress_volume,
            consumed_before,
            source.temperature,
            state.tick(),
            completes_at,
        ),
        fluid: source.fluid,
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
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(DrinkCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
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
