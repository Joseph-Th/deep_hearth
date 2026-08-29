//! Player admission, tick metabolism, and read-only survival assessment.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::state::{PlayerSurvivalRecord, player_record};
use super::{NUTRITION_PARTS_PER_MILLION, NutritionReserves, Vitality};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurvivalTickError {
    RevisionExhausted,
    EnergyCostOverflow,
    HydrationCostOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurvivalTickPlan {
    expected_revision: u64,
    next_revision: u64,
    after: PlayerSurvivalRecord,
    assessment: SurvivalAssessment,
}

pub(crate) fn decide_survival_tick(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
) -> Result<Option<SurvivalTickPlan>, SurvivalTickError> {
    let Some(before) = state.survival().player().copied() else {
        return Ok(None);
    };
    if before.vitality() == Vitality::ZERO {
        return Ok(None);
    }
    let physiology = registries.survival().physiology();
    let energy_cost = physiology
        .basal_energy_cost_per_tick()
        .checked_add(exertion.energy_cost_per_tick())
        .ok_or(SurvivalTickError::EnergyCostOverflow)?;
    let hydration_loss = physiology
        .hydration_loss_per_tick()
        .checked_add(exertion.hydration_loss_per_tick())
        .ok_or(SurvivalTickError::HydrationCostOverflow)?;
    let energy_deficit = energy_cost > before.metabolic_energy();
    let hydration_deficit = hydration_loss > before.hydration();
    let energy_after = before
        .metabolic_energy()
        .checked_sub(energy_cost)
        .unwrap_or(Energy::ZERO);
    let hydration_after = before
        .hydration()
        .checked_sub(hydration_loss)
        .unwrap_or(Volume::ZERO);
    let nutrition_after = before
        .nutrition()
        .decay(physiology.nutrition().decay_ppm_per_tick());
    let mut vitality_loss = 0_u32;
    if energy_deficit {
        vitality_loss =
            vitality_loss.saturating_add(physiology.starvation_vitality_loss_ppm_per_tick());
    }
    if hydration_deficit {
        vitality_loss =
            vitality_loss.saturating_add(physiology.dehydration_vitality_loss_ppm_per_tick());
    }
    let mut vitality_recovery_remainder = before.vitality_recovery_remainder();
    let vitality_after_ppm = if vitality_loss > 0 {
        before
            .vitality()
            .parts_per_million()
            .saturating_sub(vitality_loss)
    } else if energy_after >= physiology.hungry_below()
        && hydration_after >= physiology.thirsty_below()
        && before.vitality() < Vitality::MAXIMUM
    {
        // Current reserves support the current tick. Nutrition decays into the next persisted state
        // after supplying this tick's recovery, matching the assessment visible before the tick.
        let (recovery, next_remainder) = accumulate_diet_supported_vitality_recovery(
            physiology,
            before.nutrition(),
            vitality_recovery_remainder,
        );
        vitality_recovery_remainder = next_remainder;
        let recovered = before
            .vitality()
            .parts_per_million()
            .saturating_add(recovery)
            .min(Vitality::MAXIMUM.parts_per_million());
        if recovered == Vitality::MAXIMUM.parts_per_million() {
            vitality_recovery_remainder = 0;
        }
        recovered
    } else {
        if before.vitality() == Vitality::MAXIMUM {
            vitality_recovery_remainder = 0;
        }
        before.vitality().parts_per_million()
    };
    let vitality_after = Vitality::from_parts_per_million_unchecked(vitality_after_ppm);
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(SurvivalTickError::RevisionExhausted)?;
    let after = player_record(
        energy_after,
        hydration_after,
        vitality_after,
        nutrition_after,
        vitality_recovery_remainder,
    );
    Ok(Some(SurvivalTickPlan {
        expected_revision,
        next_revision,
        after,
        assessment: assess_record(registries, after),
    }))
}

pub(crate) fn apply_survival_tick(
    state: &mut AppState,
    plan: Option<SurvivalTickPlan>,
) -> Option<SurvivalAssessment> {
    let plan = plan?;
    state
        .survival_state_mut()
        .apply_player(plan.expected_revision, plan.next_revision, plan.after);
    Some(plan.assessment)
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
