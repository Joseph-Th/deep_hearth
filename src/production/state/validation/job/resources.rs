//! Consumed material, energy, and equipment-outcome validation for durable production jobs.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;

use super::super::super::{ProductionJobId, ProductionJobRecord};
use super::super::ProductionValidationError;

fn validate_consumed_input_trace(
    id: ProductionJobId,
    started_at: SimulationTick,
    trace: &ConsumedMaterialTrace,
) -> Result<Mass, ProductionValidationError> {
    if trace.mass().is_zero() {
        return Err(ProductionValidationError::ZeroConsumedInputMass { job: id });
    }
    trace.profile().composition().validate().map_err(|error| {
        ProductionValidationError::InvalidConsumedInputComposition { job: id, error }
    })?;
    let host = trace.profile().commodity().material();
    if trace.profile().composition().parts_per_million(host) == 0 {
        return Err(
            ProductionValidationError::ConsumedInputCompositionMissingHost { job: id, host },
        );
    }
    if trace.provenance().latest_created_at() < trace.provenance().earliest_created_at() {
        return Err(ProductionValidationError::InvalidConsumedInputProvenance { job: id });
    }
    if trace.provenance().latest_created_at() > started_at {
        return Err(ProductionValidationError::ConsumedInputCreatedAfterStart {
            job: id,
            latest_created_at: trace.provenance().latest_created_at(),
            started_at,
        });
    }
    Ok(trace.mass())
}

pub(super) fn validate_consumed_resources(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<Mass, ProductionValidationError> {
    if job.output_streams.is_empty() {
        return Err(ProductionValidationError::NoOutputs { job: id });
    }
    if job.resources.consumed_inputs.is_empty() {
        return Err(ProductionValidationError::NoConsumedInputs { job: id });
    }
    let mut traced_input_mass = Mass::ZERO;
    for trace in &job.resources.consumed_inputs {
        traced_input_mass = traced_input_mass
            .checked_add(validate_consumed_input_trace(
                id,
                job.schedule.started_at,
                trace,
            )?)
            .ok_or(ProductionValidationError::ConsumedInputMassOverflow { job: id })?;
    }
    Ok(traced_input_mass)
}

pub(super) fn validate_energy_traces(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    validate_consumed_energy_trace(id, job)?;
    validate_released_energy_trace(id, job)
}

fn validate_consumed_energy_trace(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    if let Some(trace) = job.resources.consumed_energy {
        if trace.energy().is_zero() {
            return Err(ProductionValidationError::ZeroConsumedEnergy { job: id });
        }
        if trace.source().value() == 0 {
            return Err(ProductionValidationError::InvalidConsumedEnergySource { job: id });
        }
        if trace.definition().value() == 0 {
            return Err(ProductionValidationError::InvalidConsumedEnergyDefinition { job: id });
        }
    }
    Ok(())
}

fn validate_released_energy_trace(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    if let Some(trace) = job.resources.released_energy {
        if trace.energy().is_zero() {
            return Err(ProductionValidationError::ZeroReleasedEnergy { job: id });
        }
        if trace.destination().value() == 0 {
            return Err(ProductionValidationError::InvalidReleasedEnergyDestination { job: id });
        }
        if trace.definition().value() == 0 {
            return Err(ProductionValidationError::InvalidReleasedEnergyDefinition { job: id });
        }
    }
    Ok(())
}

pub(super) fn validate_equipment_outcome(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    if job.equipment.requires_active_support && job.equipment.provider.is_none() {
        return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: id });
    }
    match (job.equipment.provider, job.equipment.condition_after) {
        (Some(provider), Some(after)) => {
            if after > provider.condition() {
                return Err(ProductionValidationError::EquipmentConditionImproved {
                    job: id,
                    before: provider.condition(),
                    after,
                });
            }
        }
        (Some(_), None) => {
            return Err(ProductionValidationError::MissingEquipmentConditionOutcome { job: id });
        }
        (None, Some(_)) => {
            return Err(ProductionValidationError::EquipmentConditionWithoutProvider { job: id });
        }
        (None, None) => {}
    }
    Ok(())
}
