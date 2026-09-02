//! Canonical synchronous simulation tick pipeline with active subsystem phases wired in visible order.

mod error;

pub use error::TickError;

use crate::core::state::{AppState, apply_clock_advance, validate_invariants};
use crate::core::time::SimulationTick;
use crate::energy::{apply_passive_energy_dissipation, decide_passive_energy_dissipation};
use crate::equipment::{
    EquipmentMaintenanceOutcome, apply_equipment_maintenance_tick,
    decide_equipment_maintenance_tick,
};
use crate::geology::{
    FieldProspectingOutcome, FieldProspectingTickError, apply_field_prospecting_tick,
    decide_field_prospecting_tick,
};
use crate::inventory::{
    StorageEnclosureDismantlingOutcome, StorageEnclosureDismantlingTickError,
    apply_storage_enclosure_dismantling_tick, decide_storage_enclosure_dismantling_tick,
};
use crate::labor::{
    ManualPowerOutcome, ManualPowerTickError, apply_manual_power_tick, apply_player_work_tick,
    decide_manual_power_tick, decide_player_work_tick, player_work_exertion,
};
use crate::mining::{MiningJobId, MiningTickError, apply_mining_tick, decide_mining_tick};
use crate::production::{
    CompletionApplication, CompletionCommitError, CompletionPlanError, ProcessCompletion,
    ProductionAvailabilityChange, apply_completion_plan, decide_due_completions,
};
use crate::registry::Registries;
use crate::survival::{
    SurvivalAssessment, SurvivalTickError, apply_survival_tick, assess_survival,
    decide_survival_tick,
};

/// Successful result of one canonical simulation tick.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickOutcome {
    tick: SimulationTick,
    production_availability_changes: Vec<ProductionAvailabilityChange>,
    production_completions: Vec<ProcessCompletion>,
    ready_mining_jobs: Vec<MiningJobId>,
    manual_power: Option<ManualPowerOutcome>,
    equipment_maintenance: Option<EquipmentMaintenanceOutcome>,
    storage_enclosure_dismantling: Option<StorageEnclosureDismantlingOutcome>,
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

    /// Returns equipment condition recovery completed by direct maintenance this tick.
    #[must_use]
    pub const fn equipment_maintenance(&self) -> Option<EquipmentMaintenanceOutcome> {
        self.equipment_maintenance
    }

    /// Returns enclosure matter recovered by direct dismantling completed during this tick.
    #[must_use]
    pub fn storage_enclosure_dismantling(&self) -> Option<&StorageEnclosureDismantlingOutcome> {
        self.storage_enclosure_dismantling.as_ref()
    }

    /// Returns the geological observation acquired by field prospecting on this tick, if any.
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

fn has_revision_capacity(current: u64, steps: u64) -> bool {
    current.checked_add(steps).is_some()
}

/// Advances the full authoritative simulation by exactly one base tick.
///
/// Every authoritative subsystem phase is sequenced here so cross-owner decisions are made against
/// one pre-tick snapshot and then applied in a deterministic order.
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
            CompletionPlanError::PlayerWorkRevision => TickError::PlayerWorkRevisionExhausted,
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
            CompletionPlanError::StorageAgeOverflow { job } => {
                TickError::ProductionStorageAgeOverflow { job }
            }
            CompletionPlanError::StructuralLoad(error) => TickError::StructuralLoad(error),
        })?;
    let projected_inventory = completion_plan.project_inventory_after_deposits(state.inventory());
    let storage_enclosure_dismantling_plan = decide_storage_enclosure_dismantling_tick(
        registries,
        state,
        &projected_inventory,
        next_tick,
    )
    .map_err(|error| match error {
        StorageEnclosureDismantlingTickError::MaterialLotIds => TickError::MaterialLotIdExhausted,
        StorageEnclosureDismantlingTickError::InventoryRevision => {
            TickError::InventoryRevisionExhausted
        }
    })?;
    let equipment_maintenance_plan = decide_equipment_maintenance_tick(state, next_tick);
    let player_work_plan = decide_player_work_tick(
        registries,
        state,
        next_tick,
        completion_plan.availability_changes(),
    )
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
        MiningTickError::Geology => TickError::GeologyRevisionExhausted,
        MiningTickError::Mining => TickError::MiningRevisionExhausted,
        MiningTickError::Equipment => TickError::EquipmentRevisionExhausted,
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
        .and_then(|steps| {
            steps.checked_add(
                equipment_maintenance_plan
                    .as_ref()
                    .map_or(0, |plan| plan.equipment_revision_steps()),
            )
        })
        .unwrap_or_else(|| panic!("fixed per-tick equipment revision budget overflowed"));
    if !has_revision_capacity(state.equipment().revision(), equipment_revision_steps) {
        return Err(TickError::EquipmentRevisionExhausted);
    }
    let scheduled_energy_revision_steps = completion_plan
        .energy_revision_steps()
        .checked_add(
            manual_power_plan
                .as_ref()
                .map_or(0, |plan| plan.energy_revision_steps()),
        )
        .unwrap_or_else(|| panic!("fixed per-tick energy revision budget overflowed"));
    let passive_energy_plan = decide_passive_energy_dissipation(registries, state);
    let energy_revision_steps = scheduled_energy_revision_steps
        .checked_add(passive_energy_plan.energy_revision_steps())
        .unwrap_or_else(|| panic!("fixed per-tick passive energy revision budget overflowed"));
    if !has_revision_capacity(state.energy().revision(), energy_revision_steps) {
        return Err(TickError::EnergyRevisionExhausted);
    }
    let exertion = player_work_exertion(registries, state, completion_plan.availability_changes());
    let survival_plan = decide_survival_tick(registries, state, exertion, next_tick).map_err(
        |error| match error {
            SurvivalTickError::RevisionExhausted => TickError::SurvivalRevisionExhausted,
            SurvivalTickError::EnergyCostOverflow => TickError::SurvivalEnergyCostOverflow,
            SurvivalTickError::HydrationCostOverflow => TickError::SurvivalHydrationCostOverflow,
        },
    )?;
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
        CompletionCommitError::PlayerWorkRevisionConflict { expected, actual } => {
            TickError::StalePlayerWorkRevision { expected, actual }
        }
        CompletionCommitError::SurvivalRevisionConflict { expected, actual } => {
            TickError::StaleSurvivalRevision { expected, actual }
        }
        CompletionCommitError::Structure(error) => TickError::Structure(error),
    })?;
    let ready_mining_jobs = apply_mining_tick(state, mining_plan);
    let manual_power = apply_manual_power_tick(state, manual_power_plan);
    let equipment_maintenance = apply_equipment_maintenance_tick(state, equipment_maintenance_plan);
    apply_passive_energy_dissipation(state, passive_energy_plan);
    let field_prospecting = apply_field_prospecting_tick(state, field_prospecting_plan);
    let storage_enclosure_dismantling =
        apply_storage_enclosure_dismantling_tick(state, storage_enclosure_dismantling_plan);
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
        equipment_maintenance,
        storage_enclosure_dismantling,
        field_prospecting,
        survival,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
