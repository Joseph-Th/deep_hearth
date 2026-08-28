//! Durable production-job record validation, independent of derived owner indexes.

use std::collections::BTreeSet;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::{AMBIENT_PRESERVATION_MULTIPLIER_PPM, ConsumedMaterialTrace};
use crate::material::MaterialLotSpec;

use super::super::{
    ProductionJobId, ProductionJobRecord, ProductionOutputStream, ProductionSuspension,
    ProductionSuspensionReason,
};
use super::ProductionValidationError;

pub(super) fn validate_job_record(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    validate_job_schedule(id, job, current)?;
    validate_job_suspension(id, job)?;
    validate_consumed_resources(id, job)?;
    validate_energy_traces(id, job)?;
    validate_equipment_outcome(id, job)?;
    validate_outputs(id, job)
}

fn validate_job_schedule(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    if id.value() == 0 || job.identity.id.value() == 0 {
        return Err(ProductionValidationError::ZeroJobId);
    }
    if id != job.identity.id {
        return Err(ProductionValidationError::JobIdMismatch {
            key: id,
            record: job.identity.id,
        });
    }
    if job.schedule.started_at > current {
        return Err(ProductionValidationError::JobStartedInFuture {
            job: id,
            started_at: job.schedule.started_at,
            current,
        });
    }
    if job.schedule.completes_at <= job.schedule.started_at {
        return Err(ProductionValidationError::CompletionNotAfterStart { job: id });
    }
    if job.schedule.active_duration.value() == 0 {
        return Err(ProductionValidationError::ZeroActiveDuration { job: id });
    }
    let storage_transition = job.resources.material_storage_history.last_transition_at();
    if storage_transition != job.schedule.started_at {
        return Err(
            ProductionValidationError::StorageHistoryTransitionMismatch {
                job: id,
                transition: storage_transition,
                started_at: job.schedule.started_at,
            },
        );
    }
    job.resources
        .material_storage_history
        .project(current, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .ok_or(ProductionValidationError::StorageHistoryOverflow {
            job: id,
            at: current,
        })?;
    job.resources
        .material_storage_history
        .project(
            job.schedule.completes_at,
            AMBIENT_PRESERVATION_MULTIPLIER_PPM,
        )
        .ok_or(ProductionValidationError::StorageHistoryOverflow {
            job: id,
            at: job.schedule.completes_at,
        })?;
    if job.equipment.requires_active_support && job.equipment.provider.is_none() {
        return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: id });
    }
    Ok(())
}

fn validate_job_suspension(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    let Some(suspension) = job.schedule.suspension else {
        return Ok(());
    };
    validate_suspension_schedule(id, job, suspension)?;
    validate_suspension_reason(id, job, suspension)
}

fn validate_suspension_schedule(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    suspension: ProductionSuspension,
) -> Result<(), ProductionValidationError> {
    if suspension.remaining_active_time().value() == 0 {
        return Err(ProductionValidationError::ZeroSuspensionRemaining { job: id });
    }
    if suspension.suspended_at() < job.schedule.started_at {
        return Err(ProductionValidationError::SuspensionBeforeStart {
            job: id,
            started_at: job.schedule.started_at,
            suspended_at: suspension.suspended_at(),
        });
    }
    if suspension.remaining_active_time().value() > job.schedule.active_duration.value() {
        return Err(
            ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                job: id,
                remaining: suspension.remaining_active_time(),
                active_duration: job.schedule.active_duration,
            },
        );
    }
    let expected_due = suspension
        .suspended_at()
        .checked_add_span(suspension.remaining_active_time())
        .ok_or(ProductionValidationError::SuspensionScheduleOverflow { job: id })?;
    if expected_due != job.schedule.completes_at {
        return Err(ProductionValidationError::SuspensionScheduleMismatch {
            job: id,
            expected_due,
            actual_due: job.schedule.completes_at,
        });
    }
    Ok(())
}

fn validate_suspension_reason(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    suspension: ProductionSuspension,
) -> Result<(), ProductionValidationError> {
    match suspension.reason() {
        ProductionSuspensionReason::EquipmentSupportUnavailable { equipment } => {
            if !job.equipment.requires_active_support {
                return Err(
                    ProductionValidationError::SuspensionEquipmentSupportNotRequired { job: id },
                );
            }
            let Some(provider) = job.equipment.provider else {
                return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: id });
            };
            let expected = provider.equipment();
            if equipment != expected {
                return Err(ProductionValidationError::SuspensionEquipmentMismatch {
                    job: id,
                    expected,
                    reason: equipment,
                });
            }
        }
        ProductionSuspensionReason::OutputSupportUnavailable { stockpile } => {
            if !job
                .output_streams
                .iter()
                .any(|stream| stream.destination == stockpile)
            {
                return Err(ProductionValidationError::SuspensionOutputMismatch {
                    job: id,
                    stockpile,
                });
            }
        }
        ProductionSuspensionReason::PlayerLaborUnavailable => {}
    }
    Ok(())
}

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

fn validate_consumed_resources(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
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
    if traced_input_mass != job.resources.consumed_mass {
        return Err(ProductionValidationError::ConsumedInputMassMismatch {
            job: id,
            traced: traced_input_mass,
            consumed: job.resources.consumed_mass,
        });
    }
    Ok(())
}

fn validate_energy_traces(
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

fn validate_equipment_outcome(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
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

fn validate_output_spec(
    id: ProductionJobId,
    output: &MaterialLotSpec,
) -> Result<(), ProductionValidationError> {
    if output.mass().is_zero() {
        return Err(ProductionValidationError::ZeroOutputMass {
            job: id,
            commodity: output.commodity(),
        });
    }
    output.composition().validate().map_err(|error| {
        ProductionValidationError::InvalidOutputComposition {
            job: id,
            commodity: output.commodity(),
            error,
        }
    })?;
    if output
        .composition()
        .parts_per_million(output.commodity().material())
        == 0
    {
        return Err(ProductionValidationError::OutputCompositionMissingHost {
            job: id,
            host: output.commodity().material(),
        });
    }
    Ok(())
}

fn validate_output_stream(
    id: ProductionJobId,
    stream: &ProductionOutputStream,
) -> Result<Mass, ProductionValidationError> {
    if stream.outputs.is_empty() {
        return Err(ProductionValidationError::EmptyOutputStream { job: id });
    }
    let mut stream_mass = Mass::ZERO;
    let mut seen_outputs = BTreeSet::new();
    let mut previous_output = None;
    for output in &stream.outputs {
        validate_output_spec(id, output)?;
        if !seen_outputs.insert(output.clone()) {
            return Err(ProductionValidationError::DuplicateOutputSpecification { job: id });
        }
        if previous_output.is_some_and(|previous: &MaterialLotSpec| previous > output) {
            return Err(ProductionValidationError::NonCanonicalOutputOrder {
                job: id,
                stream: stream.id,
            });
        }
        previous_output = Some(output);
        stream_mass = stream_mass
            .checked_add(output.mass())
            .ok_or(ProductionValidationError::OutputMassOverflow { job: id })?;
    }
    Ok(stream_mass)
}

fn validate_outputs(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    let mut output_mass = Mass::ZERO;
    let mut output_stream_ids = BTreeSet::new();
    let mut previous_stream_id = None;
    for stream in &job.output_streams {
        if stream.id.value() == 0 {
            return Err(ProductionValidationError::ZeroOutputStreamId { job: id });
        }
        if !output_stream_ids.insert(stream.id) {
            return Err(ProductionValidationError::DuplicateOutputStreamId {
                job: id,
                stream: stream.id,
            });
        }
        if previous_stream_id.is_some_and(|previous| previous > stream.id) {
            return Err(ProductionValidationError::NonCanonicalOutputStreamOrder { job: id });
        }
        previous_stream_id = Some(stream.id);
        output_mass = output_mass
            .checked_add(validate_output_stream(id, stream)?)
            .ok_or(ProductionValidationError::OutputMassOverflow { job: id })?;
    }
    if output_mass != job.resources.consumed_mass {
        return Err(ProductionValidationError::OutputMassMismatch {
            job: id,
            output: output_mass,
            consumed: job.resources.consumed_mass,
        });
    }
    Ok(())
}
