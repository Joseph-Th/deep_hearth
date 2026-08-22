//! Canonical synchronous simulation tick pipeline with active subsystem phases wired in visible order.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::{AppState, apply_clock_advance, validate_invariants};
use crate::core::time::{SimulationTick, TickSpan};
use crate::geology::{
    FieldProspectingOutcome, FieldProspectingTickError, ProspectingCommitError,
    apply_field_prospecting_tick, decide_field_prospecting_tick,
};
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
    field_prospecting: Option<FieldProspectingOutcome>,
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

    /// Returns the geological observation acquired by field inspection on this tick, if any.
    #[must_use]
    pub const fn field_prospecting(&self) -> Option<FieldProspectingOutcome> {
        self.field_prospecting
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
    /// Field prospecting cannot allocate another persistent observation identity.
    GeologicalObservationIdExhausted,
    /// Field prospecting cannot advance acquired geological knowledge.
    GeologicalKnowledgeRevisionExhausted,
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
    /// Geological knowledge changed after a due field observation was planned.
    StaleGeologicalKnowledgeRevision { expected: u64, actual: u64 },
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
            Self::GeologicalObservationIdExhausted => {
                formatter.write_str("geological observation identifier space is exhausted")
            }
            Self::GeologicalKnowledgeRevisionExhausted => {
                formatter.write_str("geological knowledge revision space is exhausted")
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
            Self::StaleGeologicalKnowledgeRevision { expected, actual } => write!(
                formatter,
                "field prospecting expected geological knowledge revision {expected} but current revision is {actual}"
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
            }
            | Self::StaleGeologicalKnowledgeRevision {
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
            | Self::ManualPowerEquipmentRevisionExhausted
            | Self::GeologicalObservationIdExhausted
            | Self::GeologicalKnowledgeRevisionExhausted => None,
        }
    }
}

fn has_revision_capacity(current: u64, steps: u64) -> bool {
    current.checked_add(steps).is_some()
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
    let field_prospecting_plan = decide_field_prospecting_tick(registries, state, next_tick)
        .map_err(|error| match error {
            FieldProspectingTickError::ObservationIdExhausted => {
                TickError::GeologicalObservationIdExhausted
            }
            FieldProspectingTickError::KnowledgeRevisionExhausted => {
                TickError::GeologicalKnowledgeRevisionExhausted
            }
        })?;
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
    let equipment_revision_steps = completion_plan
        .equipment_revision_steps()
        .checked_add(
            mining_plan
                .as_ref()
                .map_or(0, |plan| plan.equipment_revision_steps()),
        )
        .and_then(|steps| {
            steps.checked_add(
                manual_power_plan
                    .as_ref()
                    .map_or(0, |plan| plan.equipment_revision_steps()),
            )
        })
        .unwrap_or_else(|| panic!("fixed per-tick equipment revision budget overflowed"));
    if !has_revision_capacity(state.equipment().revision(), equipment_revision_steps) {
        return Err(TickError::EquipmentRevisionExhausted);
    }
    let energy_revision_steps = completion_plan
        .energy_revision_steps()
        .checked_add(
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.energy_revision_steps()),
        )
        .unwrap_or_else(|| panic!("fixed per-tick energy revision budget overflowed"));
    if !has_revision_capacity(state.energy().revision(), energy_revision_steps) {
        return Err(TickError::EnergyRevisionExhausted);
    }
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
    let field_prospecting = apply_field_prospecting_tick(state, field_prospecting_plan).map_err(
        |error| match error {
            ProspectingCommitError::StaleKnowledgeRevision { expected, actual } => {
                TickError::StaleGeologicalKnowledgeRevision { expected, actual }
            }
        },
    )?;
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
        field_prospecting,
        survival,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
