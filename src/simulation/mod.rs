//! Canonical synchronous simulation pipeline; future subsystem phases are wired here in visible order.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::{AppState, apply_clock_advance, validate_invariants};
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{StockpileId, StockpileStructuralLoadError};
use crate::labor::{
    ManualPowerOutcome, ManualPowerTickError, apply_manual_power_tick, apply_player_work_tick,
    decide_manual_power_tick, decide_player_work_tick, player_work_exertion,
};
use crate::mining::{MiningJobId, MiningTickError, apply_mining_tick, decide_mining_tick};
use crate::production::{
    CompletionApplication, CompletionCommitError, CompletionPlanError, ProcessCompletion,
    ProductionAvailabilityChange, ProductionJobId, apply_completion_plan, decide_due_completions,
};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;
use crate::survival::{
    SurvivalAssessment, SurvivalTickError, apply_survival_tick, assess_survival,
    decide_survival_tick,
};

/// Successful result of one canonical simulation tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    tick: SimulationTick,
    production_availability_changes: Vec<ProductionAvailabilityChange>,
    production_completions: Vec<ProcessCompletion>,
    ready_mining_jobs: Vec<MiningJobId>,
    manual_power: Option<ManualPowerOutcome>,
    survival: Option<SurvivalAssessment>,
}

impl TickOutcome {
    /// Returns the committed authoritative tick.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.tick
    }

    /// Returns production jobs suspended or resumed because provider availability changed during
    /// this tick. The changes are ordered by stable job ID.
    #[must_use]
    pub fn production_availability_changes(&self) -> &[ProductionAvailabilityChange] {
        &self.production_availability_changes
    }

    /// Returns process jobs whose outputs became authoritative during this tick.
    #[must_use]
    pub fn production_completions(&self) -> &[ProcessCompletion] {
        &self.production_completions
    }

    /// Returns mining jobs whose labor phase finished this tick and can now be claimed.
    #[must_use]
    pub fn ready_mining_jobs(&self) -> &[MiningJobId] {
        &self.ready_mining_jobs
    }

    /// Returns direct player-powered energy generation that completed during this tick.
    #[must_use]
    pub const fn manual_power(&self) -> Option<ManualPowerOutcome> {
        self.manual_power
    }

    /// Returns the post-tick player survival projection when survival has been initialized.
    #[must_use]
    pub const fn survival(&self) -> Option<SurvivalAssessment> {
        self.survival
    }
}

/// Failure returned before any mutation when a simulation tick cannot advance.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Energy storage cannot advance its persisted revision for completed energy release.
    EnergyRevisionExhausted,
    /// Player survival cannot advance its persisted revision for this tick.
    SurvivalRevisionExhausted,
    /// Authored basal and work energy costs cannot be represented together.
    SurvivalEnergyCostOverflow,
    /// Authored basal and work hydration losses cannot be represented together.
    SurvivalHydrationCostOverflow,
    /// Exclusive player-work ownership cannot release at this tick.
    PlayerWorkRevisionExhausted,
    /// Mining cannot advance its persisted scheduling revision for this tick.
    MiningRevisionExhausted,
    /// Direct player-powered generation cannot advance its energy owner revision this tick.
    ManualPowerEnergyRevisionExhausted,
    /// Direct player-powered generation cannot advance its equipment owner revision this tick.
    ManualPowerEquipmentRevisionExhausted,
    /// A suspended operation cannot schedule its remaining active time within the world clock.
    ProductionResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    /// Due output mass cannot be aggregated in its destination stockpile.
    DestinationMassOverflow { stockpile: StockpileId },
    /// Due output weight cannot be resolved against its structural support.
    StructuralLoad(StockpileStructuralLoadError),
    /// Inventory changed after completion planning and before commit.
    StaleInventoryRevision { expected: u64, actual: u64 },
    /// Production changed after completion planning and before commit.
    StaleProductionRevision { expected: u64, actual: u64 },
    /// Equipment changed after a wear-bearing completion was planned and before commit.
    StaleEquipmentRevision { expected: u64, actual: u64 },
    /// Energy storage changed after a released-energy completion was planned and before commit.
    StaleEnergyRevision { expected: u64, actual: u64 },
    /// Structure changed after a stored-matter load completion was planned and before commit.
    StaleStructureRevision { expected: u64, actual: u64 },
    /// A validated stored-matter structural consequence could not commit.
    Structure(StructuralCommitError),
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
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival state revision space is exhausted")
            }
            Self::SurvivalEnergyCostOverflow => {
                formatter.write_str("combined survival energy cost overflows authoritative storage")
            }
            Self::SurvivalHydrationCostOverflow => formatter
                .write_str("combined survival hydration loss overflows authoritative storage"),
            Self::PlayerWorkRevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
            Self::MiningRevisionExhausted => {
                formatter.write_str("mining revision space is exhausted")
            }
            Self::ManualPowerEnergyRevisionExhausted => {
                formatter.write_str("manual power energy revision space is exhausted")
            }
            Self::ManualPowerEquipmentRevisionExhausted => {
                formatter.write_str("manual power equipment revision space is exhausted")
            }
            Self::ProductionResumeTickOverflow {
                job,
                current,
                remaining,
            } => write!(
                formatter,
                "production job {} cannot resume {} active ticks from simulation tick {}",
                job.value(),
                remaining.value(),
                current.value()
            ),
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
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted")
            }
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "due production output mass overflows stockpile {}",
                stockpile.value()
            ),
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "due production stored-matter load failed: {error}"
                )
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
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "tick completion plan expected structural revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => {
                write!(
                    formatter,
                    "tick stored-matter structural commit failed: {error}"
                )
            }
        }
    }
}

impl Error for TickError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::Structure(error) => Some(error),
            Self::ClockExhausted { current: _current } => None,
            Self::ProductionResumeTickOverflow {
                job: _job,
                current: _current,
                remaining: _remaining,
            } => None,
            Self::DestinationMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleProductionRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEnergyRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleStructureRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::MaterialLotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::ProductionRevisionExhausted
            | Self::EquipmentRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::SurvivalEnergyCostOverflow
            | Self::SurvivalHydrationCostOverflow
            | Self::PlayerWorkRevisionExhausted
            | Self::MiningRevisionExhausted
            | Self::ManualPowerEnergyRevisionExhausted
            | Self::ManualPowerEquipmentRevisionExhausted => None,
        }
    }
}

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
        decide_due_completions(registries, state, next_tick).map_err(|error| match error {
            CompletionPlanError::MaterialLotIds => TickError::MaterialLotIdExhausted,
            CompletionPlanError::InventoryRevision => TickError::InventoryRevisionExhausted,
            CompletionPlanError::ProductionRevision => TickError::ProductionRevisionExhausted,
            CompletionPlanError::EquipmentRevision => TickError::EquipmentRevisionExhausted,
            CompletionPlanError::EnergyRevision => TickError::EnergyRevisionExhausted,
            CompletionPlanError::ResumeTickOverflow {
                job,
                current,
                remaining,
            } => TickError::ProductionResumeTickOverflow {
                job,
                current,
                remaining,
            },
            CompletionPlanError::DestinationMassOverflow { stockpile } => {
                TickError::DestinationMassOverflow { stockpile }
            }
            CompletionPlanError::StructuralLoad(error) => TickError::StructuralLoad(error),
        })?;
    let player_work_plan = decide_player_work_tick(state, next_tick)
        .map_err(|_error| TickError::PlayerWorkRevisionExhausted)?;
    let manual_power_plan =
        decide_manual_power_tick(state, next_tick).map_err(|error| match error {
            ManualPowerTickError::EnergyRevisionExhausted => {
                TickError::ManualPowerEnergyRevisionExhausted
            }
            ManualPowerTickError::EquipmentRevisionExhausted => {
                TickError::ManualPowerEquipmentRevisionExhausted
            }
        })?;
    let mining_plan = decide_mining_tick(state, next_tick).map_err(|error| match error {
        MiningTickError::MiningRevisionExhausted => TickError::MiningRevisionExhausted,
        MiningTickError::EquipmentRevisionExhausted => TickError::EquipmentRevisionExhausted,
    })?;
    let exertion = player_work_exertion(registries, state);
    let survival_plan =
        decide_survival_tick(registries, state, exertion).map_err(|error| match error {
            SurvivalTickError::RevisionExhausted => TickError::SurvivalRevisionExhausted,
            SurvivalTickError::EnergyCostOverflow => TickError::SurvivalEnergyCostOverflow,
            SurvivalTickError::HydrationCostOverflow => TickError::SurvivalHydrationCostOverflow,
        })?;
    let CompletionApplication {
        completions: production_completions,
        availability_changes: production_availability_changes,
    } = apply_completion_plan(state, completion_plan).map_err(|error| match error {
        CompletionCommitError::InventoryStale { expected, actual } => {
            TickError::StaleInventoryRevision { expected, actual }
        }
        CompletionCommitError::ProductionRevisionChanged { expected, actual } => {
            TickError::StaleProductionRevision { expected, actual }
        }
        CompletionCommitError::EquipmentRevisionConflict { expected, actual } => {
            TickError::StaleEquipmentRevision { expected, actual }
        }
        CompletionCommitError::EnergyRevisionConflict { expected, actual } => {
            TickError::StaleEnergyRevision { expected, actual }
        }
        CompletionCommitError::StructureRevisionConflict { expected, actual } => {
            TickError::StaleStructureRevision { expected, actual }
        }
        CompletionCommitError::Structure(error) => TickError::Structure(error),
    })?;
    let ready_mining_jobs = apply_mining_tick(state, mining_plan);
    let manual_power = apply_manual_power_tick(state, manual_power_plan);
    apply_player_work_tick(state, player_work_plan);
    let survival =
        apply_survival_tick(state, survival_plan).or_else(|| assess_survival(registries, state));
    apply_clock_advance(state, next_tick);

    validate_invariants(registries, state);
    Ok(TickOutcome {
        tick: next_tick,
        production_availability_changes,
        production_completions,
        ready_mining_jobs,
        manual_power,
        survival,
    })
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-foundation")
))]
mod tests {
    use super::*;
    use crate::content::build_registries;
    use crate::core::state::make_test_state_at_tick;
    use crate::core::time::WorldSeed;
    use crate::survival::{Vitality, initialize_player_survival, player_record};

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

    #[test]
    fn dead_player_remains_visible_in_tick_survival_outcome_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_1001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("dead-player survival initialization failed: {error}"));
        let player =
            state.survival().player().copied().unwrap_or_else(|| {
                panic!("initialized dead-player fixture is missing survival state")
            });
        let expected_revision = state.survival().revision();
        state.survival_state_mut().apply_player(
            expected_revision,
            expected_revision + 1,
            player_record(
                player.metabolic_energy(),
                player.hydration(),
                Vitality::ZERO,
                player.nutrition(),
            ),
        );
        let frozen_revision = state.survival().revision();
        let frozen_player = state.survival().player().copied();

        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("dead-player tick failed: {error}"));

        assert_eq!(
            outcome.survival().map(SurvivalAssessment::vitality),
            Some(Vitality::ZERO)
        );
        assert_eq!(state.survival().revision(), frozen_revision);
        assert_eq!(state.survival().player().copied(), frozen_player);
    }
}
