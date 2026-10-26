//! Authoritative per-tick survival evolution and direct-consumption uptake.

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;

use crate::survival::assessment::{SurvivalAssessment, assess_record};
use crate::survival::state::{PlayerSurvivalRecord, player_record_with_thermal_comfort};
use crate::survival::{SurvivalExertion, Vitality};

mod physiology;

use physiology::{LiveTickExposure, resolve_live_player_tick};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurvivalTickError {
    RevisionExhausted,
    EnergyCostOverflow,
    HydrationCostOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurvivalTickPlan {
    expected_revision: u64,
    next_revision: u64,
    after: PlayerSurvivalRecord,
    clear_pending_consumption: bool,
    assessment: SurvivalAssessment,
}

impl SurvivalTickPlan {
    #[must_use]
    pub(crate) fn player_dead_after_tick(&self) -> bool {
        self.after.vitality() == Vitality::ZERO
    }
}

fn build_tick_plan(
    registries: &Registries,
    state: &AppState,
    after: PlayerSurvivalRecord,
    clear_pending_consumption: bool,
) -> Result<SurvivalTickPlan, SurvivalTickError> {
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(SurvivalTickError::RevisionExhausted)?;
    Ok(SurvivalTickPlan {
        expected_revision,
        next_revision,
        after,
        clear_pending_consumption,
        assessment: assess_record(registries, after),
    })
}

#[cfg(test)]
pub(crate) fn decide_survival_tick(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
    next_tick: SimulationTick,
) -> Result<Option<SurvivalTickPlan>, SurvivalTickError> {
    decide_survival_tick_with_local_heat(
        registries,
        state,
        exertion,
        next_tick,
        Energy::ZERO,
        crate::survival::ThermalExposure::Open,
        0,
    )
}

pub(crate) fn decide_survival_tick_with_local_heat(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
    next_tick: SimulationTick,
    local_heat: Energy,
    thermal_exposure: crate::survival::ThermalExposure,
    fall_vitality_loss_ppm: u32,
) -> Result<Option<SurvivalTickPlan>, SurvivalTickError> {
    let Some(before) = state.survival().player().copied() else {
        return Ok(None);
    };
    if before.vitality() == Vitality::ZERO {
        if state.survival().pending_direct_consumption().is_none() {
            return Ok(None);
        }
        // Death discards the in-progress meal or drink with no physiological credit by design.
        // Its matter already crossed the terminal consumption boundary at admission, so there is
        // no refund and no duplication: the intake is wasted, matching death during a meal.
        return build_tick_plan(registries, state, before, true).map(Some);
    }
    let resolved = resolve_live_player_tick(
        registries,
        state,
        before,
        exertion,
        next_tick,
        LiveTickExposure {
            local_heat,
            thermal_exposure,
            fall_vitality_loss_ppm,
        },
    )?;
    build_tick_plan(
        registries,
        state,
        resolved.after,
        resolved.clear_pending_consumption,
    )
    .map(Some)
}

/// Settles exertion that accrued after the most recent slow survival boundary.
///
/// This intentionally applies no basal metabolism, nutrition decay, thermal drift, direct intake,
/// or vitality change. Those remain owned by the 3.6-second survival tick. Control-rate work uses
/// this only for the final fractional exertion tail when it completes between slow boundaries.
pub(crate) fn decide_control_exertion_settlement(
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
    let metabolic_energy = before
        .metabolic_energy()
        .checked_sub(exertion.energy_cost_per_tick())
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: admitted control-rate work exhausted metabolic reserve"
            )
        });
    let hydration = before
        .hydration()
        .checked_sub(exertion.hydration_loss_per_tick())
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: admitted control-rate work exhausted hydration reserve"
            )
        });
    let after = player_record_with_thermal_comfort(
        metabolic_energy,
        hydration,
        before.vitality(),
        before.nutrition(),
        before.vitality_recovery_remainder(),
        before.thermal_comfort_ppm(),
    );
    build_tick_plan(registries, state, after, false).map(Some)
}

/// An immediate, budgeted exertion event. Unlike accumulated scheduled work,
/// an attack rejects exhausted reserves instead of letting the control pipeline
/// panic or silently borrow from future meals. No basal tick is applied here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MeleeExertionError {
    InsufficientMetabolicEnergy,
    InsufficientHydration,
    RevisionExhausted,
}

pub(crate) fn decide_melee_exertion(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
) -> Result<SurvivalTickPlan, MeleeExertionError> {
    let player = state
        .survival()
        .player()
        .filter(|player| player.vitality() != Vitality::ZERO)
        .expect("living admitted melee actor has survival ownership");
    let metabolic_energy = player
        .metabolic_energy()
        .checked_sub(exertion.energy_cost_per_tick())
        .ok_or(MeleeExertionError::InsufficientMetabolicEnergy)?;
    let hydration = player
        .hydration()
        .checked_sub(exertion.hydration_loss_per_tick())
        .ok_or(MeleeExertionError::InsufficientHydration)?;
    let after = player_record_with_thermal_comfort(
        metabolic_energy,
        hydration,
        player.vitality(),
        player.nutrition(),
        player.vitality_recovery_remainder(),
        player.thermal_comfort_ppm(),
    );
    build_tick_plan(registries, state, after, false)
        .map_err(|_| MeleeExertionError::RevisionExhausted)
}

pub(crate) fn apply_survival_tick(
    state: &mut AppState,
    plan: Option<SurvivalTickPlan>,
) -> Option<SurvivalAssessment> {
    let plan = plan?;
    state.survival_state_mut().apply_player_tick(
        plan.expected_revision,
        plan.next_revision,
        plan.after,
        plan.clear_pending_consumption,
    );
    Some(plan.assessment)
}
