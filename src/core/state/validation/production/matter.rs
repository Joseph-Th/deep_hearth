//! Trusted-load validation for production consumed matter, output storage, and reservations.

use crate::core::state::AppState;
use crate::inventory::{StockpileId, validate_stockpile_storage};
use crate::material::{validate_material_particle_size_state, validate_material_phase_state};
use crate::production::{ProductionJobRecord, ProductionOutputStream, sum_lot_spec_mass};
use crate::registry::Registries;

use super::super::StateValidationError;
use super::super::reservations::ExpectedReservations;

pub(super) fn validate_job_consumed_inputs(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    for trace in job.consumed_inputs() {
        let commodity = trace.profile().commodity();
        if !registries.materials().has_commodity(commodity) {
            return Err(StateValidationError::UnknownJobConsumedCommodity {
                job: job.id(),
                commodity,
            });
        }
        validate_material_particle_size_state(
            registries.materials(),
            commodity,
            trace.profile().particle_size_distribution(),
        )
        .map_err(
            |error| StateValidationError::InvalidJobConsumedParticleSizeState {
                job: job.id(),
                error,
            },
        )?;
        validate_material_phase_state(
            registries.materials(),
            commodity,
            trace.profile().composition(),
            trace.profile().temperature(),
        )
        .map_err(|error| StateValidationError::InvalidJobConsumedPhaseState {
            job: job.id(),
            error,
        })?;
    }
    Ok(())
}

pub(super) fn validate_job_outputs(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    for stream in job.output_streams() {
        validate_job_output_stream(registries, state, job, stream, expected_reservations)?;
    }
    Ok(())
}

fn validate_job_output_stream(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    stream: &ProductionOutputStream,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    let destination = stream.destination();
    let Some(destination_record) = state.systems.inventory.get_stockpile(destination) else {
        return Err(StateValidationError::UnknownJobDestination {
            job: job.id(),
            stockpile: destination,
        });
    };
    for output in stream.outputs() {
        validate_job_output(registries, job, destination_record, destination, output)?;
    }
    let output_mass = sum_lot_spec_mass(stream.outputs())
        .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
    expected_reservations.add(destination, output_mass)
}

fn validate_job_output(
    registries: &Registries,
    job: &ProductionJobRecord,
    destination_record: &crate::inventory::StockpileRecord,
    destination: StockpileId,
    output: &crate::material::MaterialLotSpec,
) -> Result<(), StateValidationError> {
    if !registries.materials().has_commodity(output.commodity()) {
        return Err(StateValidationError::UnknownJobOutputCommodity {
            job: job.id(),
            commodity: output.commodity(),
        });
    }
    validate_stockpile_storage(
        registries,
        destination_record,
        destination,
        output.commodity(),
        output.composition(),
        output.temperature(),
        output.particle_size_distribution(),
    )
    .map_err(|error| StateValidationError::JobOutputStorage {
        job: job.id(),
        error,
    })
}
