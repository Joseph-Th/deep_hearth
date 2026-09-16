//! Validates production-job references, physical traces, output custody, and runtime resources.

use crate::core::state::AppState;
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::StateValidationError;
use super::reservations::ExpectedReservations;

mod matter;
mod resources;
mod subsystems;

use matter::{validate_job_consumed_inputs, validate_job_outputs};
use resources::{
    validate_job_consumed_energy, validate_job_equipment, validate_job_process_and_source,
    validate_job_released_energy, validate_job_resource_topology,
};
use subsystems::validate_job_subsystem_contracts;

pub(super) fn validate_production_references(
    registries: &Registries,
    state: &AppState,
) -> Result<ExpectedReservations, StateValidationError> {
    let mut expected_reservations = ExpectedReservations::default();
    for job in state.systems.production.jobs() {
        validate_production_job(registries, state, job, &mut expected_reservations)?;
    }
    let inventory_revision = state.systems.inventory.revision();
    if !state
        .systems
        .production
        .has_scheduled_revision_capacity_from(inventory_revision)
    {
        return Err(
            StateValidationError::ProductionInventoryRevisionCapacityExhausted {
                revision: inventory_revision,
                completion_buckets: state.systems.production.scheduled_completion_bucket_count(),
            },
        );
    }
    let equipment_revision = state.systems.equipment.revision();
    if !state
        .systems
        .production
        .has_scheduled_equipment_revision_capacity_from(equipment_revision)
    {
        return Err(
            StateValidationError::ProductionEquipmentRevisionCapacityExhausted {
                revision: equipment_revision,
                completion_buckets: state
                    .systems
                    .production
                    .scheduled_equipment_revision_bucket_count(),
            },
        );
    }
    let energy_revision = state.systems.energy.revision();
    if !state
        .systems
        .production
        .has_scheduled_released_energy_revision_capacity_from(energy_revision)
    {
        return Err(
            StateValidationError::ProductionEnergyRevisionCapacityExhausted {
                revision: energy_revision,
                completion_buckets: state
                    .systems
                    .production
                    .scheduled_released_energy_revision_bucket_count(),
            },
        );
    }
    let structure_revision = state.systems.structures.revision();
    if !state
        .systems
        .production
        .has_scheduled_supported_output_revision_capacity_from(
            structure_revision,
            &state.systems.inventory,
        )
    {
        return Err(
            StateValidationError::ProductionStructureRevisionCapacityExhausted {
                revision: structure_revision,
                completion_buckets: state
                    .systems
                    .production
                    .scheduled_supported_output_revision_bucket_count(&state.systems.inventory),
            },
        );
    }
    Ok(expected_reservations)
}

fn validate_production_job(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    validate_job_process_and_source(registries, state, job)?;
    validate_job_consumed_energy(registries, state, job)?;
    validate_job_released_energy(registries, state, job)?;
    validate_job_equipment(registries, state, job)?;
    validate_job_resource_topology(registries, job)?;
    validate_job_subsystem_contracts(registries, job)?;
    validate_job_consumed_inputs(registries, job)?;
    validate_job_outputs(registries, state, job, expected_reservations)
}
