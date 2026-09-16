//! Cross-owner persistence validation for mining job semantics and resource references.

use std::collections::BTreeMap;

use crate::core::quantity::{Mass, Pressure, Temperature};
use crate::core::state::AppState;
use crate::equipment::{EquipmentDefinition, EquipmentOccupancy, equipment_occupancy};
use crate::geology::GeologicalDepositId;
use crate::inventory::{StockpileRecord, validate_stockpile_storage};
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;

use super::physics::{MiningPhysicsError, resolve_mining_physics};
use super::{MiningJobId, MiningJobRecord, MiningMethodDefinition};

mod error;

pub use error::MiningJobValidationError;

fn map_physics_error(job: MiningJobId, error: MiningPhysicsError) -> MiningJobValidationError {
    match error {
        MiningPhysicsError::MissingCapability { capability } => {
            MiningJobValidationError::MissingCapability { job, capability }
        }
        MiningPhysicsError::CapabilityKindMismatch {
            capability,
            expected,
            found,
        } => MiningJobValidationError::CapabilityKindMismatch {
            job,
            capability,
            expected,
            found,
        },
        MiningPhysicsError::BatchTooLarge { maximum, requested } => {
            MiningJobValidationError::BatchTooLarge {
                job,
                maximum,
                requested,
            }
        }
        MiningPhysicsError::DepositTooHard { hardness, maximum } => {
            MiningJobValidationError::DepositTooHard {
                job,
                hardness,
                maximum,
            }
        }
        MiningPhysicsError::ZeroThroughput => MiningJobValidationError::ZeroThroughput { job },
        MiningPhysicsError::Duration(error) => MiningJobValidationError::Duration { job, error },
        MiningPhysicsError::ConditionDuration(error) => {
            MiningJobValidationError::ConditionDuration { job, error }
        }
    }
}

struct MiningJobReferences<'state> {
    method: &'state MiningMethodDefinition,
    destination: &'state StockpileRecord,
    equipment_definition: &'state EquipmentDefinition,
    deposit_commodity: CommodityKey,
    deposit_temperature: Temperature,
    deposit_composition: &'state MaterialComposition,
    deposit_remaining_mass: Mass,
    excavation_hardness: Pressure,
}

fn resolve_mining_job_references<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    job: &MiningJobRecord,
) -> Result<MiningJobReferences<'state>, MiningJobValidationError> {
    let method = registries
        .mining()
        .get_method(job.method())
        .ok_or(MiningJobValidationError::UnknownMethod { job: job.id() })?;
    let deposit = state
        .geology()
        .get_deposit(job.deposit())
        .ok_or(MiningJobValidationError::UnknownDeposit { job: job.id() })?;
    let destination = state
        .inventory()
        .get_stockpile(job.destination())
        .ok_or(MiningJobValidationError::UnknownDestination { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(job.equipment_definition())
        .ok_or(MiningJobValidationError::UnknownEquipmentDefinition {
            job: job.id(),
            definition: job.equipment_definition(),
        })?;
    Ok(MiningJobReferences {
        method,
        destination,
        equipment_definition,
        deposit_commodity: deposit.commodity(),
        deposit_temperature: deposit.temperature(),
        deposit_composition: deposit.composition(),
        deposit_remaining_mass: deposit.remaining_mass(),
        excavation_hardness: deposit.excavation_hardness(),
    })
}

fn validate_working_mining_equipment(
    state: &AppState,
    job: &MiningJobRecord,
    references: &MiningJobReferences<'_>,
) -> Result<(), MiningJobValidationError> {
    if !job.is_working() {
        return Ok(());
    }
    let equipment = state
        .equipment()
        .get_equipment(job.equipment())
        .ok_or(MiningJobValidationError::WorkingEquipmentMissing { job: job.id() })?;
    if equipment.definition() != job.equipment_definition() {
        return Err(
            MiningJobValidationError::WorkingEquipmentDefinitionMismatch {
                job: job.id(),
                expected: job.equipment_definition(),
                actual: equipment.definition(),
            },
        );
    }
    validate_mining_equipment_portability(job.id(), references.equipment_definition)?;
    if equipment.supported_by().is_some() {
        return Err(MiningJobValidationError::WorkingEquipmentMounted { job: job.id() });
    }
    if equipment.condition() != job.equipment_condition_before() {
        return Err(MiningJobValidationError::EquipmentConditionMismatch { job: job.id() });
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;

fn validate_mining_equipment_portability(
    job: MiningJobId,
    definition: &EquipmentDefinition,
) -> Result<(), MiningJobValidationError> {
    if definition.requires_structural_support() {
        return Err(MiningJobValidationError::WorkingEquipmentRequiresStructuralSupport { job });
    }
    Ok(())
}

fn validate_mining_source_ownership(
    job: &MiningJobRecord,
    references: &MiningJobReferences<'_>,
) -> Result<(), MiningJobValidationError> {
    if job.requested_mass().is_zero() {
        return Err(MiningJobValidationError::ZeroRequestedMass { job: job.id() });
    }
    let expected_output = Mass::from_milligrams(
        job.requested_mass()
            .milligrams()
            .min(job.deposit_mass_before().milligrams()),
    );
    if job.output().mass() != expected_output {
        return Err(MiningJobValidationError::OutputMassMismatch {
            job: job.id(),
            requested: job.requested_mass(),
            available: job.deposit_mass_before(),
            output: job.output().mass(),
        });
    }
    let remaining_after = job
        .deposit_mass_before()
        .checked_sub(job.output().mass())
        .ok_or(MiningJobValidationError::OutputExceedsDepositTrace {
            job: job.id(),
            traced: job.deposit_mass_before(),
            output: job.output().mass(),
        })?;
    if job.is_working() {
        if references.deposit_remaining_mass != job.deposit_mass_before() {
            return Err(MiningJobValidationError::WorkingDepositMassMismatch {
                job: job.id(),
                expected: job.deposit_mass_before(),
                actual: references.deposit_remaining_mass,
            });
        }
    } else if references.deposit_remaining_mass > remaining_after {
        return Err(
            MiningJobValidationError::ReadyDepositMassAbovePostExtraction {
                job: job.id(),
                maximum: remaining_after,
                actual: references.deposit_remaining_mass,
            },
        );
    }
    Ok(())
}

fn validate_mining_output(
    registries: &Registries,
    job: &MiningJobRecord,
    references: &MiningJobReferences<'_>,
) -> Result<(), MiningJobValidationError> {
    let output = job.output();
    if output.commodity() != references.deposit_commodity
        || output.temperature() != references.deposit_temperature
        || output.composition() != references.deposit_composition
        || output.particle_size_distribution().is_some()
    {
        return Err(MiningJobValidationError::OutputProfileMismatch { job: job.id() });
    }
    if validate_stockpile_storage(
        registries,
        references.destination,
        job.destination(),
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .is_err()
    {
        return Err(MiningJobValidationError::OutputStorageInvalid { job: job.id() });
    }
    Ok(())
}

fn validate_mining_equipment_exclusivity(
    state: &AppState,
    job: &MiningJobRecord,
) -> Result<(), MiningJobValidationError> {
    if !job.is_working() {
        return Ok(());
    }
    match equipment_occupancy(state, job.equipment()) {
        Some(EquipmentOccupancy::Production { .. }) => {
            return Err(MiningJobValidationError::EquipmentAlsoUsedByProduction { job: job.id() });
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            return Err(MiningJobValidationError::EquipmentAlsoUsedByManualPower { job: job.id() });
        }
        Some(
            EquipmentOccupancy::Mining { .. }
            | EquipmentOccupancy::Prospecting { .. }
            | EquipmentOccupancy::Maintenance { .. },
        )
        | None => {}
    }
    Ok(())
}

fn validate_mining_job_physics(
    registries: &Registries,
    job: &MiningJobRecord,
    references: &MiningJobReferences<'_>,
) -> Result<(), MiningJobValidationError> {
    let physics = resolve_mining_physics(
        registries.core().physical_tick_duration(),
        references.method,
        references.equipment_definition,
        job.equipment_condition_before(),
        references.excavation_hardness,
        job.requested_mass(),
    )
    .map_err(|error| map_physics_error(job.id(), error))?;
    let stored_duration = job
        .completes_at()
        .checked_duration_since(job.started_at())
        .filter(|duration| !duration.is_zero())
        .ok_or(MiningJobValidationError::InvalidSchedule { job: job.id() })?;
    if stored_duration != physics.duration() {
        return Err(MiningJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: physics.duration(),
        });
    }
    if job.equipment_condition_after() != physics.condition_after() {
        return Err(MiningJobValidationError::ConditionOutcomeMismatch {
            job: job.id(),
            stored: job.equipment_condition_after(),
            required: physics.condition_after(),
        });
    }
    Ok(())
}

fn validate_working_completion_revision_capacity(
    state: &AppState,
    job: &MiningJobRecord,
) -> Result<(), MiningJobValidationError> {
    if !job.is_working() {
        return Ok(());
    }
    if state.mining().revision().checked_add(1).is_none() {
        return Err(MiningJobValidationError::WorkingMiningRevisionExhausted { job: job.id() });
    }
    if state.geology().revision().checked_add(1).is_none() {
        return Err(MiningJobValidationError::WorkingGeologyRevisionExhausted { job: job.id() });
    }
    if job.equipment_condition_after() != job.equipment_condition_before()
        && state.equipment().revision().checked_add(1).is_none()
    {
        return Err(MiningJobValidationError::WorkingEquipmentRevisionExhausted { job: job.id() });
    }
    Ok(())
}

fn validate_loaded_mining_job(
    registries: &Registries,
    state: &AppState,
    job: &MiningJobRecord,
) -> Result<(), MiningJobValidationError> {
    let references = resolve_mining_job_references(registries, state, job)?;
    validate_working_completion_revision_capacity(state, job)?;
    validate_working_mining_equipment(state, job, &references)?;
    validate_mining_source_ownership(job, &references)?;
    validate_mining_output(registries, job, &references)?;
    validate_mining_equipment_exclusivity(state, job)?;
    validate_mining_job_physics(registries, job, &references)
}

pub(crate) fn validate_loaded_mining_jobs(
    registries: &Registries,
    state: &AppState,
) -> Result<(), MiningJobValidationError> {
    for job in state.mining().jobs() {
        validate_loaded_mining_job(registries, state, job)?;
    }
    validate_retained_work_intervals(state)?;
    validate_retained_deposit_history(state)
}

fn validate_retained_work_intervals(state: &AppState) -> Result<(), MiningJobValidationError> {
    let mut jobs = state.mining().jobs().collect::<Vec<_>>();
    jobs.sort_by_key(|job| (job.started_at(), job.id()));
    for pair in jobs.windows(2) {
        let earlier = pair[0];
        let later = pair[1];
        if later.started_at() < earlier.completes_at() {
            return Err(MiningJobValidationError::OverlappingRetainedWork {
                earlier: earlier.id(),
                later: later.id(),
                earlier_completes: earlier.completes_at(),
                later_starts: later.started_at(),
            });
        }
    }
    Ok(())
}

fn validate_retained_deposit_history(state: &AppState) -> Result<(), MiningJobValidationError> {
    let mut by_deposit = BTreeMap::<GeologicalDepositId, Vec<&MiningJobRecord>>::new();
    for job in state.mining().jobs() {
        by_deposit.entry(job.deposit()).or_default().push(job);
    }
    for jobs in by_deposit.values_mut() {
        jobs.sort_by_key(|job| (job.completes_at(), job.id()));
        for pair in jobs.windows(2) {
            let earlier = pair[0];
            let later = pair[1];
            let maximum_later_mass = earlier
                .deposit_mass_before()
                .checked_sub(earlier.output().mass())
                .unwrap_or_else(|| {
                    unreachable!(
                        "individual mining source validation ran before history validation"
                    )
                });
            if later.deposit_mass_before() > maximum_later_mass {
                return Err(MiningJobValidationError::DepositHistoryMassIncrease {
                    earlier: earlier.id(),
                    later: later.id(),
                    maximum_later_mass,
                    later_mass: later.deposit_mass_before(),
                });
            }
        }
    }
    Ok(())
}
