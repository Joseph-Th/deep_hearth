//! Revision-bound player-work admission and resource reservation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::{
    ControlTick, SimulationBoundaryError, TickSpan, require_simulation_boundary,
};
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

use super::{
    PlayerAttentionError, PlayerWork, PlayerWorkResourceBudgetError, ValidatedPlayerAttentionHold,
    calculate_player_work_resource_budget, validate_player_attention,
};

mod tick;

pub(crate) use tick::{
    PlayerWorkTickError, apply_player_work_tick, decide_manual_production_player_work_start,
    decide_player_work_control_tick, decide_player_work_tick, player_work_exertion,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerWorkStartError {
    SimulationBoundary(SimulationBoundaryError),
    SurvivalNotInitialized,
    PlayerDead,
    PlayerAirborne,
    PlayerMoving,
    Busy { active: Box<PlayerWork> },
    MetabolicCostOverflow { duration: TickSpan },
    InsufficientMetabolicEnergy { available: Energy, required: Energy },
    HydrationCostOverflow { duration: TickSpan },
    InsufficientHydration { available: Volume, required: Volume },
    RevisionExhausted,
    SurvivalRevisionExhausted { duration: TickSpan },
}

impl Display for PlayerWorkStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimulationBoundary(error) => Display::fmt(error, formatter),
            Self::SurvivalNotInitialized => {
                formatter.write_str("player work requires initialized survival state")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot begin work"),
            Self::PlayerAirborne => {
                formatter.write_str("player must be grounded before beginning slow work")
            }
            Self::PlayerMoving => {
                formatter.write_str("player must stop horizontal motion before beginning slow work")
            }
            Self::Busy { active } => write!(formatter, "player is already occupied by {active:?}"),
            Self::MetabolicCostOverflow { duration } => write!(
                formatter,
                "player-work metabolic cost overflows across {} active ticks",
                duration.value()
            ),
            Self::InsufficientMetabolicEnergy {
                available,
                required,
            } => write!(
                formatter,
                "player work requires {} nJ of metabolic reserve but only {} nJ is available",
                required.nanojoules(),
                available.nanojoules()
            ),
            Self::HydrationCostOverflow { duration } => write!(
                formatter,
                "player-work hydration cost overflows across {} active ticks",
                duration.value()
            ),
            Self::InsufficientHydration {
                available,
                required,
            } => write!(
                formatter,
                "player work requires {} uL of hydration reserve but only {} uL is available",
                required.microliters(),
                available.microliters()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
            Self::SurvivalRevisionExhausted { duration } => write!(
                formatter,
                "player work cannot reserve survival revisions for {} active ticks",
                duration.value()
            ),
        }
    }
}

impl Error for PlayerWorkStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SimulationBoundary(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::PlayerAirborne
            | Self::PlayerMoving
            | Self::Busy { .. }
            | Self::MetabolicCostOverflow { .. }
            | Self::InsufficientMetabolicEnergy { .. }
            | Self::HydrationCostOverflow { .. }
            | Self::InsufficientHydration { .. }
            | Self::RevisionExhausted
            | Self::SurvivalRevisionExhausted { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkCommitError {
    StaleControlTick {
        expected: ControlTick,
        actual: ControlTick,
    },
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
}

impl Display for PlayerWorkCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleControlTick { expected, actual } => write!(
                formatter,
                "player-work admission expected control tick {} but current control tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "player-work revision changed from {expected} to {actual} after validation"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "player-work admission expected survival revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerWorkCommitError {}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerWorkStart {
    attention: ValidatedPlayerAttentionHold,
    expected_control_tick: ControlTick,
    expected_survival_revision: u64,
    resource_budget: super::PlayerWorkResourceBudget,
}

impl ValidatedPlayerWorkStart {
    pub(crate) const fn resource_budget(&self) -> super::PlayerWorkResourceBudget {
        self.resource_budget
    }

    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), PlayerWorkCommitError> {
        let actual_control_tick = state.control_tick();
        if state.tick() == self.expected_control_tick.simulation_tick()
            && actual_control_tick != self.expected_control_tick
        {
            return Err(PlayerWorkCommitError::StaleControlTick {
                expected: self.expected_control_tick,
                actual: actual_control_tick,
            });
        }
        self.attention.precheck(state).map_err(|conflict| {
            PlayerWorkCommitError::StaleRevision {
                expected: conflict.expected(),
                actual: conflict.actual(),
            }
        })?;
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(PlayerWorkCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        self.attention.apply(state);
    }
}

pub(crate) fn validate_player_work_start(
    registries: &Registries,
    state: &AppState,
    work: PlayerWork,
    duration: TickSpan,
    exertion: SurvivalExertion,
) -> Result<ValidatedPlayerWorkStart, PlayerWorkStartError> {
    require_simulation_boundary(state.control_tick())
        .map_err(PlayerWorkStartError::SimulationBoundary)?;
    validate_player_work_start_inner(
        registries,
        state,
        work,
        duration,
        exertion,
        duration.value(),
    )
}

pub(crate) fn validate_player_work_start_control_rate(
    registries: &Registries,
    state: &AppState,
    work: PlayerWork,
    duration: TickSpan,
    exertion: SurvivalExertion,
) -> Result<ValidatedPlayerWorkStart, PlayerWorkStartError> {
    let survival_revision_demand = duration
        .value()
        .checked_add(u64::from(!state.control_tick().is_simulation_boundary()))
        .ok_or(PlayerWorkStartError::SurvivalRevisionExhausted { duration })?;
    validate_player_work_start_inner(
        registries,
        state,
        work,
        duration,
        exertion,
        survival_revision_demand,
    )
}

fn validate_player_work_start_inner(
    registries: &Registries,
    state: &AppState,
    work: PlayerWork,
    duration: TickSpan,
    exertion: SurvivalExertion,
    survival_revision_demand: u64,
) -> Result<ValidatedPlayerWorkStart, PlayerWorkStartError> {
    if crate::logistics::player_body_uses_generated_world(
        state,
        registries.survival().physiology().locomotion(),
    ) {
        if !crate::logistics::player_body_is_grounded(registries, state) {
            return Err(PlayerWorkStartError::PlayerAirborne);
        }
        if state.logistics().player().is_some_and(|player| {
            player.body().horizontal_velocity() != crate::spatial::HorizontalVelocity::ZERO
        }) {
            return Err(PlayerWorkStartError::PlayerMoving);
        }
    }
    let attention = validate_player_attention(state).map_err(|error| match error {
        PlayerAttentionError::SurvivalNotInitialized => {
            PlayerWorkStartError::SurvivalNotInitialized
        }
        PlayerAttentionError::PlayerDead => PlayerWorkStartError::PlayerDead,
        PlayerAttentionError::Busy { active } => PlayerWorkStartError::Busy { active },
    })?;
    let Some(player) = state.survival().player() else {
        return Err(PlayerWorkStartError::SurvivalNotInitialized);
    };
    let budget = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        duration,
    )
    .map_err(|error| match error {
        PlayerWorkResourceBudgetError::EnergyOverflow => {
            PlayerWorkStartError::MetabolicCostOverflow { duration }
        }
        PlayerWorkResourceBudgetError::HydrationOverflow => {
            PlayerWorkStartError::HydrationCostOverflow { duration }
        }
    })?;
    if player.metabolic_energy() < budget.metabolic_energy() {
        return Err(PlayerWorkStartError::InsufficientMetabolicEnergy {
            available: player.metabolic_energy(),
            required: budget.metabolic_energy(),
        });
    }
    if player.hydration() < budget.hydration() {
        return Err(PlayerWorkStartError::InsufficientHydration {
            available: player.hydration(),
            required: budget.hydration(),
        });
    }
    if !state
        .survival()
        .can_advance_revision_by(survival_revision_demand)
    {
        return Err(PlayerWorkStartError::SurvivalRevisionExhausted { duration });
    }
    let attention = attention
        .hold(work)
        .ok_or(PlayerWorkStartError::RevisionExhausted)?;
    Ok(ValidatedPlayerWorkStart {
        attention,
        expected_control_tick: state.control_tick(),
        expected_survival_revision: state.survival().revision(),
        resource_budget: budget,
    })
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
