//! Authoritative per-tick survival evolution and direct-consumption uptake.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;

use crate::survival::assessment::{SurvivalAssessment, assess_record};
use crate::survival::state::PlayerSurvivalRecord;
use crate::survival::{SurvivalExertion, Vitality};

mod physiology;

use physiology::resolve_live_player_tick;

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

pub(crate) fn decide_survival_tick(
    registries: &Registries,
    state: &AppState,
    exertion: SurvivalExertion,
    next_tick: SimulationTick,
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
    let resolved = resolve_live_player_tick(registries, state, before, exertion, next_tick)?;
    build_tick_plan(
        registries,
        state,
        resolved.after,
        resolved.clear_pending_consumption,
    )
    .map(Some)
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
