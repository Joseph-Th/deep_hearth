//! Canonical synchronous simulation pipeline; future subsystem phases are wired here in visible order.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::{AppState, apply_clock_advance, validate_invariants};
use crate::core::time::SimulationTick;
use crate::production::{
    CompletionCommitError, CompletionPlanError, ProcessCompletion, apply_completion_plan,
    decide_due_completions,
};
use crate::registry::Registries;

/// Successful result of one canonical simulation tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    tick: SimulationTick,
    production_completions: Vec<ProcessCompletion>,
}

impl TickOutcome {
    /// Returns the committed authoritative tick.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    /// Returns process jobs whose outputs became authoritative during this tick.
    #[must_use]
    pub fn production_completions(&self) -> &[ProcessCompletion] {
        &self.production_completions
    }
}

/// Failure returned before any mutation when a simulation tick cannot advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickError {
    /// The authoritative tick counter has reached its representable maximum.
    ClockExhausted { current: SimulationTick },
    /// Due output lots cannot be allocated without exhausting persistent lot identity space.
    MaterialLotIdExhausted,
    /// Inventory cannot advance its persisted revision for this tick's consequences.
    InventoryRevisionExhausted,
    /// Production cannot advance its persisted revision for this tick's consequences.
    ProductionRevisionExhausted,
    /// Equipment cannot advance its persisted revision for completed-operation wear.
    EquipmentRevisionExhausted,
    /// Inventory changed after completion planning and before commit.
    StaleInventoryRevision { expected: u64, actual: u64 },
    /// Production changed after completion planning and before commit.
    StaleProductionRevision { expected: u64, actual: u64 },
    /// Equipment changed after a wear-bearing completion was planned and before commit.
    StaleEquipmentRevision { expected: u64, actual: u64 },
}

impl Display for TickError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClockExhausted { current } => {
                write!(
                    formatter,
                    "simulation clock exhausted at tick {}",
                    current.value()
                )
            }
            Self::MaterialLotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::ProductionRevisionExhausted => {
                formatter.write_str("production revision space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected production revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected equipment revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for TickError {}

/// Advances the full authoritative simulation by exactly one base tick.
///
/// All future subsystem phases belong in this visible sequence. The registry parameter is already
/// part of the canonical signature even though the initial clock-only foundation does not yet need
/// a static definition during advancement.
pub fn advance_tick(
    registries: &Registries,
    state: &mut AppState,
) -> Result<TickOutcome, TickError> {
    let current = state.tick();
    let Some(next_value) = current.value().checked_add(1) else {
        return Err(TickError::ClockExhausted { current });
    };
    let next_tick = SimulationTick::new(next_value);

    // Decide against the pre-tick snapshot; due jobs are indexed by exact authoritative tick.
    let completion_plan =
        decide_due_completions(state, next_tick).map_err(|error| match error {
            CompletionPlanError::MaterialLotIds => TickError::MaterialLotIdExhausted,
            CompletionPlanError::InventoryRevision => TickError::InventoryRevisionExhausted,
            CompletionPlanError::ProductionRevision => TickError::ProductionRevisionExhausted,
            CompletionPlanError::EquipmentRevision => TickError::EquipmentRevisionExhausted,
        })?;
    let production_completions =
        apply_completion_plan(state, completion_plan).map_err(|error| match error {
            CompletionCommitError::InventoryStale { expected, actual } => {
                TickError::StaleInventoryRevision { expected, actual }
            }
            CompletionCommitError::ProductionRevisionChanged { expected, actual } => {
                TickError::StaleProductionRevision { expected, actual }
            }
            CompletionCommitError::EquipmentRevisionConflict { expected, actual } => {
                TickError::StaleEquipmentRevision { expected, actual }
            }
        })?;
    apply_clock_advance(state, next_tick);

    validate_invariants(registries, state);
    Ok(TickOutcome {
        tick: next_tick,
        production_completions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::build_registries;
    use crate::core::state::make_test_state_at_tick;
    use crate::core::time::WorldSeed;

    #[test]
    fn canonical_tick_advances_exactly_once() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(7));

        let result = advance_tick(&registries, &mut state);
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => panic!("tick unexpectedly failed: {error}"),
        };

        assert_eq!(outcome.tick(), SimulationTick::new(1));
        assert_eq!(state.tick(), SimulationTick::new(1));
    }

    #[test]
    fn clock_exhaustion_leaves_state_unchanged() {
        let registries = build_registries();
        let mut state = make_test_state_at_tick(WorldSeed::new(9), SimulationTick::new(u64::MAX));
        let before = state.clone();

        let result = advance_tick(&registries, &mut state);

        assert_eq!(
            result,
            Err(TickError::ClockExhausted {
                current: SimulationTick::new(u64::MAX),
            })
        );
        assert_eq!(state, before);
    }
}
