//! Player admission, tick metabolism, and read-only survival assessment.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::state::{PlayerSurvivalRecord, player_record};
use super::{NUTRITION_PARTS_PER_MILLION, NutritionReserves, Vitality};

mod tick;

pub(crate) use tick::{SurvivalTickError, apply_survival_tick, decide_survival_tick};

/// Qualitative energy state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HungerState {
    Fed,
    Hungry,
    Starving,
}

/// Additional per-tick physiological cost of the player's current physical work.
///
/// Basal metabolism remains authored by `PhysiologyDefinition`; work owners contribute only the
/// incremental cost above rest so simulation can combine them without creating a second metabolism
/// path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurvivalExertion {
    energy_cost_per_tick: Energy,
    hydration_loss_per_tick: Volume,
}

impl SurvivalExertion {
    pub const REST: Self = Self {
        energy_cost_per_tick: Energy::ZERO,
        hydration_loss_per_tick: Volume::ZERO,
    };

    #[must_use]
    pub const fn new(energy_cost_per_tick: Energy, hydration_loss_per_tick: Volume) -> Self {
        Self {
            energy_cost_per_tick,
            hydration_loss_per_tick,
        }
    }

    #[must_use]
    pub const fn energy_cost_per_tick(self) -> Energy {
        self.energy_cost_per_tick
    }

    #[must_use]
    pub const fn hydration_loss_per_tick(self) -> Volume {
        self.hydration_loss_per_tick
    }

    /// Rejects a resting profile where an authored action represents active physical player work.
    pub(crate) const fn assert_active_player_work(self) {
        assert!(
            !self.energy_cost_per_tick.is_zero(),
            "active player work exertion must consume metabolic energy"
        );
    }
}

/// Qualitative hydration state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HydrationState {
    Hydrated,
    Thirsty,
    Dehydrated,
}

/// Read-only survival projection suitable for UI and gameplay policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurvivalAssessment {
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    nutrition: NutritionReserves,
    diet_quality_ppm: u32,
    diet_supported_vitality_recovery_ppm_per_tick: u32,
    hunger: HungerState,
    hydration_state: HydrationState,
}

impl SurvivalAssessment {
    #[must_use]
    pub const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub const fn hydration(self) -> Volume {
        self.hydration
    }

    #[must_use]
    pub const fn vitality(self) -> Vitality {
        self.vitality
    }

    #[must_use]
    pub const fn nutrition(self) -> NutritionReserves {
        self.nutrition
    }

    #[must_use]
    pub const fn diet_quality_ppm(self) -> u32 {
        self.diet_quality_ppm
    }

    /// Returns the current whole-ppm per-tick vitality recovery supported by recent dietary balance,
    /// rounded to the nearest ppm for presentation.
    ///
    /// Recovery still requires the player to remain above the authored hunger and thirst warning
    /// thresholds. Exposing the rate here makes the practical consequence of diet quality available
    /// to UI and gameplay policy without duplicating survival-owner arithmetic.
    #[must_use]
    pub const fn diet_supported_vitality_recovery_ppm_per_tick(self) -> u32 {
        self.diet_supported_vitality_recovery_ppm_per_tick
    }

    #[must_use]
    pub const fn hunger(self) -> HungerState {
        self.hunger
    }

    #[must_use]
    pub const fn hydration_state(self) -> HydrationState {
        self.hydration_state
    }
}

/// Failure while admitting the local player into survival simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializeSurvivalError {
    AlreadyInitialized,
    RevisionExhausted,
}

impl Display for InitializeSurvivalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => {
                formatter.write_str("player survival is already initialized")
            }
            Self::RevisionExhausted => formatter.write_str("survival revision space is exhausted"),
        }
    }
}

impl Error for InitializeSurvivalError {}

/// Starts the local player's survival state at authored full reserves.
pub fn initialize_player_survival(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    let physiology = registries.survival().physiology();
    state.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );
    Ok(())
}

/// Initializes the gameplay harness player exactly at the authored hydration warning boundary.
///
/// This is fixture-only starting-state construction. Runtime depletion still occurs exclusively
/// through authoritative simulation ticks after scenario setup returns.
#[cfg(feature = "test-gameplay")]
pub(crate) fn initialize_player_survival_at_hydration_warning_boundary_for_fixture(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    let physiology = registries.survival().physiology();
    state.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.thirsty_below(),
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );
    Ok(())
}

/// Initializes the gameplay harness player exactly at the authored hunger warning boundary.
///
/// This is fixture-only starting-state construction. Runtime depletion still occurs exclusively
/// through authoritative simulation ticks after scenario setup returns.
#[cfg(feature = "test-gameplay")]
pub(crate) fn initialize_player_survival_at_hunger_warning_boundary_for_fixture(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    let physiology = registries.survival().physiology();
    state.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            physiology.hungry_below(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );
    Ok(())
}

/// Returns the current survival projection when a player has been admitted.
#[must_use]
pub fn assess_survival(registries: &Registries, state: &AppState) -> Option<SurvivalAssessment> {
    state
        .survival()
        .player()
        .copied()
        .map(|player| assess_record(registries, player))
}

fn assess_record(registries: &Registries, player: PlayerSurvivalRecord) -> SurvivalAssessment {
    let physiology = registries.survival().physiology();
    let hunger = if player.metabolic_energy().is_zero() {
        HungerState::Starving
    } else if player.metabolic_energy() < physiology.hungry_below() {
        HungerState::Hungry
    } else {
        HungerState::Fed
    };
    let hydration_state = if player.hydration().is_zero() {
        HydrationState::Dehydrated
    } else if player.hydration() < physiology.thirsty_below() {
        HydrationState::Thirsty
    } else {
        HydrationState::Hydrated
    };
    SurvivalAssessment {
        metabolic_energy: player.metabolic_energy(),
        hydration: player.hydration(),
        vitality: player.vitality(),
        nutrition: player.nutrition(),
        diet_quality_ppm: player.nutrition().quality_ppm(),
        diet_supported_vitality_recovery_ppm_per_tick:
            diet_supported_vitality_recovery_ppm_per_tick(physiology, player.nutrition()),
        hunger,
        hydration_state,
    }
}

fn diet_supported_vitality_recovery_ppm_per_tick(
    physiology: super::PhysiologyDefinition,
    nutrition: NutritionReserves,
) -> u32 {
    let scale = u64::from(NUTRITION_PARTS_PER_MILLION);
    let numerator = u64::from(physiology.nutrition().vitality_recovery_ppm_per_tick())
        * u64::from(nutrition.quality_ppm());
    let recovery = (numerator + scale / 2) / scale;
    u32::try_from(recovery)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery always fits u32"))
}

fn accumulate_diet_supported_vitality_recovery(
    physiology: super::PhysiologyDefinition,
    nutrition: NutritionReserves,
    remainder: u32,
) -> (u32, u32) {
    debug_assert!(remainder < NUTRITION_PARTS_PER_MILLION);
    let scale = u64::from(NUTRITION_PARTS_PER_MILLION);
    let numerator = u64::from(physiology.nutrition().vitality_recovery_ppm_per_tick())
        * u64::from(nutrition.quality_ppm())
        + u64::from(remainder);
    let recovery = u32::try_from(numerator / scale)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery always fits u32"));
    let next_remainder = u32::try_from(numerator % scale)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery remainder always fits u32"));
    (recovery, next_remainder)
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
