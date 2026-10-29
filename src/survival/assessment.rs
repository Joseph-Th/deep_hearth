//! Read-only survival assessment and diet-supported vitality-recovery arithmetic.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::logistics::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
use crate::registry::Registries;

use super::state::PlayerSurvivalRecord;
use super::{
    NUTRITION_PARTS_PER_MILLION, NutritionReserves, PhysiologyDefinition, ThermalExposure, Vitality,
};

/// Conditional warmth runway at the player's current position, assuming no local heat.
///
/// This is an actionable forecast, not a promise about weather, travel or future fuel. The
/// two cover alternatives show the physical value of securing a roof; only `current()`
/// describes the player's actual terrain and active structures. Missing world authority
/// cannot be mistaken for open sky or for shelter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerUnheatedComfortRunway {
    overhead_cover: PlayerOverheadCoverAssessment,
    current: Option<TickSpan>,
    if_open: TickSpan,
    if_covered: TickSpan,
}

/// Time until a living player's first hunger/thirst warning while resting without intake.
/// Movement, labor, meals, illness or other future changes invalidate these conditional
/// estimates; they are not a guarantee of uninterrupted safe travel. No forecast is
/// offered while already-admitted intake is scheduled to change the reserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerRestingProvisionRunway {
    hungry_in: Option<TickSpan>,
    thirsty_in: Option<TickSpan>,
}

impl PlayerRestingProvisionRunway {
    /// `None` means no finite duration is representable at this resting rate.
    #[must_use]
    pub const fn hungry_in(self) -> Option<TickSpan> {
        self.hungry_in
    }

    #[must_use]
    pub const fn thirsty_in(self) -> Option<TickSpan> {
        self.thirsty_in
    }
}

fn warning_runway(remaining: u128, warning: u128, loss: u128) -> Option<TickSpan> {
    // A zero threshold still has a real zero-reserve emergency state.
    let comfortable_floor = warning.max(1);
    if remaining < comfortable_floor {
        return Some(TickSpan::ZERO);
    }
    if loss == 0 {
        return None;
    }
    let to_spend = remaining - comfortable_floor + 1;
    u64::try_from(to_spend.div_ceil(loss))
        .ok()
        .map(TickSpan::new)
}

/// A player-visible resting baseline for deciding whether to provision before work or travel.
/// Uses the same current reserves and basal costs as survival execution, without assuming that
/// later locomotion or manual work is free.
#[must_use]
pub fn assess_player_resting_provision_runway(
    registries: &Registries,
    state: &AppState,
) -> Option<PlayerRestingProvisionRunway> {
    let player = state.survival().player()?;
    if player.vitality() == Vitality::ZERO
        || state.survival().pending_direct_consumption().is_some()
    {
        return None;
    }
    let physiology = registries.survival().physiology();
    Some(PlayerRestingProvisionRunway {
        hungry_in: warning_runway(
            player.metabolic_energy().nanojoules(),
            physiology.hungry_below().nanojoules(),
            physiology.basal_energy_cost_per_tick().nanojoules(),
        ),
        thirsty_in: warning_runway(
            u128::from(player.hydration().microliters()),
            u128::from(physiology.thirsty_below().microliters()),
            u128::from(physiology.hydration_loss_per_tick().microliters()),
        ),
    })
}

impl PlayerUnheatedComfortRunway {
    #[must_use]
    pub const fn overhead_cover(self) -> PlayerOverheadCoverAssessment {
        self.overhead_cover
    }

    /// Ticks until the first chilly/cold state if today's exposure and zero heat persist.
    /// Unknown terrain/roof truth produces `None`, not an invented estimate.
    #[must_use]
    pub const fn current(self) -> Option<TickSpan> {
        self.current
    }

    #[must_use]
    pub const fn if_open(self) -> TickSpan {
        self.if_open
    }

    #[must_use]
    pub const fn if_covered(self) -> TickSpan {
        self.if_covered
    }
}

/// Combines real current cover, live survival reserve and authored warmth loss.
/// The projection is deliberately conditional on staying in place without a heat source.
#[must_use]
pub fn assess_player_unheated_comfort_runway(
    registries: &Registries,
    state: &AppState,
) -> Option<PlayerUnheatedComfortRunway> {
    let player = state.survival().player()?;
    if player.vitality() == Vitality::ZERO {
        return None;
    }
    let cover = assess_player_overhead_cover(registries, state)?;
    let comfort = registries.survival().physiology().thermal_comfort();
    let if_open = comfort
        .ticks_until_unheated_discomfort(player.thermal_comfort_ppm(), ThermalExposure::Open);
    let if_covered = comfort.ticks_until_unheated_discomfort(
        player.thermal_comfort_ppm(),
        ThermalExposure::OverheadCover,
    );
    let current = match cover {
        PlayerOverheadCoverAssessment::Open => Some(if_open),
        PlayerOverheadCoverAssessment::Covered => Some(if_covered),
        PlayerOverheadCoverAssessment::WorldUnavailable => None,
    };
    Some(PlayerUnheatedComfortRunway {
        overhead_cover: cover,
        current,
        if_open,
        if_covered,
    })
}

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

/// Qualitative short-horizon warmth state derived from authored physiology thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThermalComfortState {
    Comfortable,
    Chilly,
    Cold,
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
    thermal_comfort_ppm: u32,
    thermal_comfort_state: ThermalComfortState,
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

    /// Returns a coarse whole-ppm per-tick vitality-recovery signal supported by recent dietary
    /// balance, rounded to the nearest ppm for presentation.
    ///
    /// The survival owner still accumulates sub-ppm recovery exactly; this read surface avoids
    /// implying that tiny fractional differences are automatically meaningful player decisions.
    /// Recovery still requires the player to remain above the authored hunger and thirst warning
    /// thresholds and at or above the thermal-comfort warning threshold.
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

    #[must_use]
    pub const fn thermal_comfort_ppm(self) -> u32 {
        self.thermal_comfort_ppm
    }

    #[must_use]
    pub const fn thermal_comfort_state(self) -> ThermalComfortState {
        self.thermal_comfort_state
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
    let thermal_comfort_state = if player.thermal_comfort_ppm() == 0 {
        ThermalComfortState::Cold
    } else if player.thermal_comfort_ppm() < physiology.thermal_comfort().warning_below_ppm() {
        ThermalComfortState::Chilly
    } else {
        ThermalComfortState::Comfortable
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
        thermal_comfort_ppm: player.thermal_comfort_ppm(),
        thermal_comfort_state,
    }
}

#[cfg(test)]
#[path = "assessment_tests.rs"]
mod tests;

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
