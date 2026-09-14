//! Read-only survival assessment and diet-supported vitality-recovery arithmetic.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::state::PlayerSurvivalRecord;
use super::{NUTRITION_PARTS_PER_MILLION, NutritionReserves, PhysiologyDefinition, Vitality};

/// Qualitative energy state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HungerState {
    Fed,
    Hungry,
    Starving,
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

/// Returns the current survival projection when a player has been admitted.
#[must_use]
pub fn assess_survival(registries: &Registries, state: &AppState) -> Option<SurvivalAssessment> {
    state
        .survival()
        .player()
        .copied()
        .map(|player| assess_record(registries, player))
}

pub(super) fn assess_record(
    registries: &Registries,
    player: PlayerSurvivalRecord,
) -> SurvivalAssessment {
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

fn diet_recovery_numerator(physiology: PhysiologyDefinition, nutrition: NutritionReserves) -> u64 {
    u64::from(physiology.nutrition().vitality_recovery_ppm_per_tick())
        * u64::from(nutrition.quality_ppm())
}

fn diet_supported_vitality_recovery_ppm_per_tick(
    physiology: PhysiologyDefinition,
    nutrition: NutritionReserves,
) -> u32 {
    let scale = u64::from(NUTRITION_PARTS_PER_MILLION);
    let recovery = (diet_recovery_numerator(physiology, nutrition) + scale / 2) / scale;
    u32::try_from(recovery)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery always fits u32"))
}

pub(super) fn accumulate_diet_supported_vitality_recovery(
    physiology: PhysiologyDefinition,
    nutrition: NutritionReserves,
    remainder: u32,
) -> (u32, u32) {
    debug_assert!(remainder < NUTRITION_PARTS_PER_MILLION);
    let scale = u64::from(NUTRITION_PARTS_PER_MILLION);
    let numerator = diet_recovery_numerator(physiology, nutrition) + u64::from(remainder);
    let recovery = u32::try_from(numerator / scale)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery always fits u32"));
    let next_remainder = u32::try_from(numerator % scale)
        .unwrap_or_else(|_| unreachable!("normalized vitality recovery remainder always fits u32"));
    (recovery, next_remainder)
}
