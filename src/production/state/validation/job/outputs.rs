//! Output-stream canonicalization and mass-conservation validation for durable production jobs.

use std::collections::BTreeSet;

use crate::core::quantity::Mass;
use crate::material::MaterialLotSpec;

use super::super::super::{ProductionJobId, ProductionJobRecord, ProductionOutputStream};
use super::super::ProductionValidationError;

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

pub(super) fn validate_outputs(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    consumed_mass: Mass,
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
    if output_mass != consumed_mass {
        return Err(ProductionValidationError::OutputMassMismatch {
            job: id,
            output: output_mass,
            consumed: consumed_mass,
        });
    }
    Ok(())
}
