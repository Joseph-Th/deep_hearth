//! Persistent-state validation for production; this child audits private owner data without exposing mutation.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionError, MaterialId, MaterialLotSpec};

use super::super::resolution::ProcessOutputStreamId;
use super::{ProductionJobId, ProductionState, ProductionSuspensionReason};

/// Persistent-state validation failure for production records or their due index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionValidationError {
    ZeroNextJobId,
    ZeroJobId,
    NextIdNotAfterExisting {
        next: u64,
        highest: ProductionJobId,
    },
    ConsumedInputCreatedAfterStart {
        job: ProductionJobId,
        latest_created_at: SimulationTick,
        started_at: SimulationTick,
    },
    JobIdMismatch {
        key: ProductionJobId,
        record: ProductionJobId,
    },
    CompletionNotAfterStart {
        job: ProductionJobId,
    },
    ZeroActiveDuration {
        job: ProductionJobId,
    },
    RequiredSupportWithoutEquipment {
        job: ProductionJobId,
    },
    SuspensionWithoutRequiredSupport {
        job: ProductionJobId,
    },
    ZeroSuspensionRemaining {
        job: ProductionJobId,
    },
    SuspensionBeforeStart {
        job: ProductionJobId,
        started_at: SimulationTick,
        suspended_at: SimulationTick,
    },
    SuspensionRemainingExceedsActiveDuration {
        job: ProductionJobId,
        remaining: TickSpan,
        active_duration: TickSpan,
    },
    SuspensionScheduleOverflow {
        job: ProductionJobId,
    },
    SuspensionScheduleMismatch {
        job: ProductionJobId,
        expected_due: SimulationTick,
        actual_due: SimulationTick,
    },
    SuspensionEquipmentMismatch {
        job: ProductionJobId,
        expected: EquipmentId,
        reason: EquipmentId,
    },
    NoOutputs {
        job: ProductionJobId,
    },
    ZeroOutputStreamId {
        job: ProductionJobId,
    },
    DuplicateOutputStreamId {
        job: ProductionJobId,
        stream: ProcessOutputStreamId,
    },
    NonCanonicalOutputStreamOrder {
        job: ProductionJobId,
    },
    EmptyOutputStream {
        job: ProductionJobId,
    },
    NoConsumedInputs {
        job: ProductionJobId,
    },
    ZeroConsumedInputMass {
        job: ProductionJobId,
    },
    InvalidConsumedInputComposition {
        job: ProductionJobId,
        error: CompositionError,
    },
    ConsumedInputCompositionMissingHost {
        job: ProductionJobId,
        host: MaterialId,
    },
    InvalidConsumedInputProvenance {
        job: ProductionJobId,
    },
    ConsumedInputMassOverflow {
        job: ProductionJobId,
    },
    ConsumedInputMassMismatch {
        job: ProductionJobId,
        traced: Mass,
        consumed: Mass,
    },
    ZeroConsumedEnergy {
        job: ProductionJobId,
    },
    InvalidConsumedEnergySource {
        job: ProductionJobId,
    },
    InvalidConsumedEnergyDefinition {
        job: ProductionJobId,
    },
    ZeroReleasedEnergy {
        job: ProductionJobId,
    },
    InvalidReleasedEnergyDestination {
        job: ProductionJobId,
    },
    InvalidReleasedEnergyDefinition {
        job: ProductionJobId,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionWithoutProvider {
        job: ProductionJobId,
    },
    EquipmentConditionImproved {
        job: ProductionJobId,
        before: Condition,
        after: Condition,
    },
    ZeroOutputMass {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    InvalidOutputComposition {
        job: ProductionJobId,
        commodity: CommodityKey,
        error: CompositionError,
    },
    OutputCompositionMissingHost {
        job: ProductionJobId,
        host: MaterialId,
    },
    DuplicateOutputSpecification {
        job: ProductionJobId,
    },
    NonCanonicalOutputOrder {
        job: ProductionJobId,
        stream: ProcessOutputStreamId,
    },
    OutputMassOverflow {
        job: ProductionJobId,
    },
    OutputMassMismatch {
        job: ProductionJobId,
        output: Mass,
        consumed: Mass,
    },
    MissingDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    UnexpectedDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    SuspendedJobInDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    EmptyDueIndex {
        due: SimulationTick,
    },
    EnergyOccupancyIndexMismatch {
        store: EnergyStoreId,
        indexed: Option<ProductionJobId>,
        expected: Option<ProductionJobId>,
    },
    EnergyDoubleBooked {
        store: EnergyStoreId,
    },
    EquipmentOccupancyIndexMismatch {
        equipment: EquipmentId,
        indexed: Option<ProductionJobId>,
        expected: Option<ProductionJobId>,
    },
    EquipmentDoubleBooked {
        equipment: EquipmentId,
    },
    StockpileOccupancyIndexMismatch {
        stockpile: StockpileId,
    },
}

impl Display for ProductionValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextJobId => formatter.write_str("next production job id must not be zero"),
            Self::ZeroJobId => formatter.write_str("production job id must not be zero"),
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next production job id {next} is not after existing id {}",
                highest.value()
            ),
            Self::JobIdMismatch { key, record } => write!(
                formatter,
                "production map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::CompletionNotAfterStart { job } => write!(
                formatter,
                "production job {} does not complete after its start tick",
                job.value()
            ),
            Self::ZeroActiveDuration { job } => write!(
                formatter,
                "production job {} has zero required active duration",
                job.value()
            ),
            Self::RequiredSupportWithoutEquipment { job } => write!(
                formatter,
                "production job {} requires active equipment support but has no equipment provider",
                job.value()
            ),
            Self::SuspensionWithoutRequiredSupport { job } => write!(
                formatter,
                "production job {} is suspended for equipment support without an active-support requirement",
                job.value()
            ),
            Self::ZeroSuspensionRemaining { job } => write!(
                formatter,
                "production job {} suspension retains zero active time",
                job.value()
            ),
            Self::SuspensionBeforeStart {
                job,
                started_at,
                suspended_at,
            } => write!(
                formatter,
                "production job {} claims suspension at tick {} before its start tick {}",
                job.value(),
                suspended_at.value(),
                started_at.value()
            ),
            Self::SuspensionRemainingExceedsActiveDuration {
                job,
                remaining,
                active_duration,
            } => write!(
                formatter,
                "production job {} suspension retains {} active ticks but the operation requires only {} active ticks total",
                job.value(),
                remaining.value(),
                active_duration.value()
            ),
            Self::SuspensionScheduleOverflow { job } => write!(
                formatter,
                "production job {} suspension schedule exceeds simulation tick range",
                job.value()
            ),
            Self::SuspensionScheduleMismatch {
                job,
                expected_due,
                actual_due,
            } => write!(
                formatter,
                "production job {} suspended schedule implies due tick {} but stores due tick {}",
                job.value(),
                expected_due.value(),
                actual_due.value()
            ),
            Self::SuspensionEquipmentMismatch {
                job,
                expected,
                reason,
            } => write!(
                formatter,
                "production job {} suspension references equipment {} but provider is {}",
                job.value(),
                reason.value(),
                expected.value()
            ),
            Self::NoOutputs { job } => write!(
                formatter,
                "production job {} owns no in-process output matter",
                job.value()
            ),
            Self::ZeroOutputStreamId { job } => write!(
                formatter,
                "production job {} contains a zero output stream id",
                job.value()
            ),
            Self::DuplicateOutputStreamId { job, stream } => write!(
                formatter,
                "production job {} contains duplicate output stream id {}",
                job.value(),
                stream.value()
            ),
            Self::NonCanonicalOutputStreamOrder { job } => write!(
                formatter,
                "production job {} output streams are not in canonical stream-id order",
                job.value()
            ),
            Self::EmptyOutputStream { job } => write!(
                formatter,
                "production job {} contains an empty output stream",
                job.value()
            ),
            Self::NoConsumedInputs { job } => write!(
                formatter,
                "production job {} has no consumed input traces",
                job.value()
            ),
            Self::ZeroConsumedInputMass { job } => write!(
                formatter,
                "production job {} contains a zero-mass consumed input trace",
                job.value()
            ),
            Self::InvalidConsumedInputComposition { job, error } => write!(
                formatter,
                "production job {} contains invalid consumed input composition: {error}",
                job.value()
            ),
            Self::ConsumedInputCompositionMissingHost { job, host } => write!(
                formatter,
                "production job {} consumed input composition omits host material {}",
                job.value(),
                host.value()
            ),
            Self::InvalidConsumedInputProvenance { job } => write!(
                formatter,
                "production job {} contains an invalid consumed input provenance range",
                job.value()
            ),
            Self::ConsumedInputMassOverflow { job } => write!(
                formatter,
                "production job {} consumed input trace mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::ConsumedInputMassMismatch {
                job,
                traced,
                consumed,
            } => write!(
                formatter,
                "production job {} traces {} mg of consumed input but records {} mg consumed",
                job.value(),
                traced.milligrams(),
                consumed.milligrams()
            ),
            Self::ConsumedInputCreatedAfterStart {
                job,
                latest_created_at,
                started_at,
            } => write!(
                formatter,
                "production job {} consumed input provenance reaches tick {} after job start tick {}",
                job.value(),
                latest_created_at.value(),
                started_at.value()
            ),
            Self::ZeroConsumedEnergy { job } => write!(
                formatter,
                "production job {} traces a zero-energy operation input",
                job.value()
            ),
            Self::InvalidConsumedEnergySource { job } => write!(
                formatter,
                "production job {} traces invalid zero energy-store identity",
                job.value()
            ),
            Self::InvalidConsumedEnergyDefinition { job } => write!(
                formatter,
                "production job {} traces invalid zero energy-store definition identity",
                job.value()
            ),
            Self::ZeroReleasedEnergy { job } => write!(
                formatter,
                "production job {} traces zero released energy",
                job.value()
            ),
            Self::InvalidReleasedEnergyDestination { job } => write!(
                formatter,
                "production job {} traces invalid zero released-energy destination identity",
                job.value()
            ),
            Self::InvalidReleasedEnergyDefinition { job } => write!(
                formatter,
                "production job {} traces invalid zero released-energy definition identity",
                job.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "production job {} occupies equipment without a post-operation condition outcome",
                job.value()
            ),
            Self::EquipmentConditionWithoutProvider { job } => write!(
                formatter,
                "production job {} stores an equipment condition outcome without a provider",
                job.value()
            ),
            Self::EquipmentConditionImproved { job, before, after } => write!(
                formatter,
                "production job {} improves equipment condition from {} ppm to {} ppm",
                job.value(),
                before.parts_per_million(),
                after.parts_per_million()
            ),
            Self::ZeroOutputMass { job, commodity } => write!(
                formatter,
                "production job {} promises zero mass for material {} form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InvalidOutputComposition {
                job,
                commodity,
                error,
            } => write!(
                formatter,
                "production job {} output material {} form {} has invalid composition: {error}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputCompositionMissingHost { job, host } => write!(
                formatter,
                "production job {} output composition omits host material {}",
                job.value(),
                host.value()
            ),
            Self::DuplicateOutputSpecification { job } => write!(
                formatter,
                "production job {} contains duplicate resolved output lot specifications",
                job.value()
            ),
            Self::NonCanonicalOutputOrder { job, stream } => write!(
                formatter,
                "production job {} output stream {} lot specifications are not in canonical order",
                job.value(),
                stream.value()
            ),
            Self::OutputMassMismatch {
                job,
                output,
                consumed,
            } => write!(
                formatter,
                "production job {} owns {} mg output but records {} mg consumed",
                job.value(),
                output.milligrams(),
                consumed.milligrams()
            ),
            Self::OutputMassOverflow { job } => write!(
                formatter,
                "production job {} output mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::MissingDueIndex { job, due } => write!(
                formatter,
                "production job {} is missing from due index tick {}",
                job.value(),
                due.value()
            ),
            Self::UnexpectedDueIndex { job, due } => write!(
                formatter,
                "due index tick {} references inconsistent production job {}",
                due.value(),
                job.value()
            ),
            Self::SuspendedJobInDueIndex { job, due } => write!(
                formatter,
                "suspended production job {} remains indexed for completion at tick {}",
                job.value(),
                due.value()
            ),
            Self::EmptyDueIndex { due } => write!(
                formatter,
                "production due index contains an empty bucket at tick {}",
                due.value()
            ),
            Self::EnergyOccupancyIndexMismatch {
                store,
                indexed,
                expected,
            } => write!(
                formatter,
                "energy occupancy index for store {} records job {:?} but active jobs require {:?}",
                store.value(),
                indexed.map(ProductionJobId::value),
                expected.map(ProductionJobId::value)
            ),
            Self::EnergyDoubleBooked { store } => write!(
                formatter,
                "multiple production jobs exclusively reserve energy store {}",
                store.value()
            ),
            Self::EquipmentOccupancyIndexMismatch {
                equipment,
                indexed,
                expected,
            } => write!(
                formatter,
                "equipment occupancy index for equipment {} records job {:?} but active jobs require {:?}",
                equipment.value(),
                indexed.map(ProductionJobId::value),
                expected.map(ProductionJobId::value)
            ),
            Self::EquipmentDoubleBooked { equipment } => write!(
                formatter,
                "multiple production jobs exclusively occupy equipment {}",
                equipment.value()
            ),
            Self::StockpileOccupancyIndexMismatch { stockpile } => write!(
                formatter,
                "stockpile occupancy index for stockpile {} disagrees with active production jobs",
                stockpile.value()
            ),
        }
    }
}

impl Error for ProductionValidationError {}

pub(crate) fn validate_loaded_production(
    state: &ProductionState,
) -> Result<(), ProductionValidationError> {
    if state.next_job_id == 0 {
        return Err(ProductionValidationError::ZeroNextJobId);
    }
    if let Some(highest) = state.jobs.keys().next_back().copied()
        && state.next_job_id <= highest.value()
    {
        return Err(ProductionValidationError::NextIdNotAfterExisting {
            next: state.next_job_id,
            highest,
        });
    }

    for (id, job) in &state.jobs {
        if id.value() == 0 || job.identity.id.value() == 0 {
            return Err(ProductionValidationError::ZeroJobId);
        }
        if *id != job.identity.id {
            return Err(ProductionValidationError::JobIdMismatch {
                key: *id,
                record: job.identity.id,
            });
        }
        if job.schedule.completes_at <= job.schedule.started_at {
            return Err(ProductionValidationError::CompletionNotAfterStart { job: *id });
        }
        if job.schedule.active_duration.value() == 0 {
            return Err(ProductionValidationError::ZeroActiveDuration { job: *id });
        }
        if job.equipment.requires_active_support && job.equipment.provider.is_none() {
            return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: *id });
        }
        if let Some(suspension) = job.schedule.suspension {
            if !job.equipment.requires_active_support {
                return Err(
                    ProductionValidationError::SuspensionWithoutRequiredSupport { job: *id },
                );
            }
            if suspension.remaining_active_time().value() == 0 {
                return Err(ProductionValidationError::ZeroSuspensionRemaining { job: *id });
            }
            if suspension.suspended_at() < job.schedule.started_at {
                return Err(ProductionValidationError::SuspensionBeforeStart {
                    job: *id,
                    started_at: job.schedule.started_at,
                    suspended_at: suspension.suspended_at(),
                });
            }
            if suspension.remaining_active_time().value() > job.schedule.active_duration.value() {
                return Err(
                    ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                        job: *id,
                        remaining: suspension.remaining_active_time(),
                        active_duration: job.schedule.active_duration,
                    },
                );
            }
            let expected_due = suspension
                .suspended_at()
                .checked_add_span(suspension.remaining_active_time())
                .ok_or(ProductionValidationError::SuspensionScheduleOverflow { job: *id })?;
            if expected_due != job.schedule.completes_at {
                return Err(ProductionValidationError::SuspensionScheduleMismatch {
                    job: *id,
                    expected_due,
                    actual_due: job.schedule.completes_at,
                });
            }
            match suspension.reason() {
                ProductionSuspensionReason::EquipmentSupportUnavailable { equipment } => {
                    let expected = match job.equipment.provider {
                        Some(provider) => provider.equipment(),
                        None => {
                            return Err(
                                ProductionValidationError::RequiredSupportWithoutEquipment {
                                    job: *id,
                                },
                            );
                        }
                    };
                    if equipment != expected {
                        return Err(ProductionValidationError::SuspensionEquipmentMismatch {
                            job: *id,
                            expected,
                            reason: equipment,
                        });
                    }
                }
            }
        }
        if job.output_streams.is_empty() {
            return Err(ProductionValidationError::NoOutputs { job: *id });
        }
        if job.resources.consumed_inputs.is_empty() {
            return Err(ProductionValidationError::NoConsumedInputs { job: *id });
        }
        let mut traced_input_mass = Mass::ZERO;
        for trace in &job.resources.consumed_inputs {
            if trace.mass().is_zero() {
                return Err(ProductionValidationError::ZeroConsumedInputMass { job: *id });
            }
            trace.profile().composition().validate().map_err(|error| {
                ProductionValidationError::InvalidConsumedInputComposition { job: *id, error }
            })?;
            let host = trace.profile().commodity().material();
            if trace.profile().composition().parts_per_million(host) == 0 {
                return Err(
                    ProductionValidationError::ConsumedInputCompositionMissingHost {
                        job: *id,
                        host,
                    },
                );
            }
            if trace.provenance().latest_created_at() < trace.provenance().earliest_created_at() {
                return Err(ProductionValidationError::InvalidConsumedInputProvenance { job: *id });
            }
            if trace.provenance().latest_created_at() > job.schedule.started_at {
                return Err(ProductionValidationError::ConsumedInputCreatedAfterStart {
                    job: *id,
                    latest_created_at: trace.provenance().latest_created_at(),
                    started_at: job.schedule.started_at,
                });
            }
            traced_input_mass = traced_input_mass
                .checked_add(trace.mass())
                .ok_or(ProductionValidationError::ConsumedInputMassOverflow { job: *id })?;
        }
        if traced_input_mass != job.resources.consumed_mass {
            return Err(ProductionValidationError::ConsumedInputMassMismatch {
                job: *id,
                traced: traced_input_mass,
                consumed: job.resources.consumed_mass,
            });
        }
        if let Some(trace) = job.resources.consumed_energy {
            if trace.energy().is_zero() {
                return Err(ProductionValidationError::ZeroConsumedEnergy { job: *id });
            }
            if trace.source().value() == 0 {
                return Err(ProductionValidationError::InvalidConsumedEnergySource { job: *id });
            }
            if trace.definition().value() == 0 {
                return Err(ProductionValidationError::InvalidConsumedEnergyDefinition {
                    job: *id,
                });
            }
        }
        if let Some(trace) = job.resources.released_energy {
            if trace.energy().is_zero() {
                return Err(ProductionValidationError::ZeroReleasedEnergy { job: *id });
            }
            if trace.destination().value() == 0 {
                return Err(
                    ProductionValidationError::InvalidReleasedEnergyDestination { job: *id },
                );
            }
            if trace.definition().value() == 0 {
                return Err(ProductionValidationError::InvalidReleasedEnergyDefinition {
                    job: *id,
                });
            }
        }
        match (job.equipment.provider, job.equipment.condition_after) {
            (Some(provider), Some(after)) => {
                if after > provider.condition() {
                    return Err(ProductionValidationError::EquipmentConditionImproved {
                        job: *id,
                        before: provider.condition(),
                        after,
                    });
                }
            }
            (Some(_), None) => {
                return Err(
                    ProductionValidationError::MissingEquipmentConditionOutcome { job: *id },
                );
            }
            (None, Some(_)) => {
                return Err(
                    ProductionValidationError::EquipmentConditionWithoutProvider { job: *id },
                );
            }
            (None, None) => {}
        }
        let mut output_mass = Mass::ZERO;
        let mut output_stream_ids = BTreeSet::new();
        let mut previous_stream_id = None;
        for stream in &job.output_streams {
            if stream.id.value() == 0 {
                return Err(ProductionValidationError::ZeroOutputStreamId { job: *id });
            }
            if !output_stream_ids.insert(stream.id) {
                return Err(ProductionValidationError::DuplicateOutputStreamId {
                    job: *id,
                    stream: stream.id,
                });
            }
            if previous_stream_id.is_some_and(|previous| previous > stream.id) {
                return Err(ProductionValidationError::NonCanonicalOutputStreamOrder { job: *id });
            }
            previous_stream_id = Some(stream.id);
            if stream.outputs.is_empty() {
                return Err(ProductionValidationError::EmptyOutputStream { job: *id });
            }
            let mut seen_outputs = BTreeSet::new();
            let mut previous_output = None;
            for output in &stream.outputs {
                if output.mass().is_zero() {
                    return Err(ProductionValidationError::ZeroOutputMass {
                        job: *id,
                        commodity: output.commodity(),
                    });
                }
                output.composition().validate().map_err(|error| {
                    ProductionValidationError::InvalidOutputComposition {
                        job: *id,
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
                        job: *id,
                        host: output.commodity().material(),
                    });
                }
                if !seen_outputs.insert(output.clone()) {
                    return Err(ProductionValidationError::DuplicateOutputSpecification {
                        job: *id,
                    });
                }
                if previous_output.is_some_and(|previous: &MaterialLotSpec| previous > output) {
                    return Err(ProductionValidationError::NonCanonicalOutputOrder {
                        job: *id,
                        stream: stream.id,
                    });
                }
                previous_output = Some(output);
                output_mass = output_mass
                    .checked_add(output.mass())
                    .ok_or(ProductionValidationError::OutputMassOverflow { job: *id })?;
            }
        }
        if output_mass != job.resources.consumed_mass {
            return Err(ProductionValidationError::OutputMassMismatch {
                job: *id,
                output: output_mass,
                consumed: job.resources.consumed_mass,
            });
        }
        let is_indexed = state
            .due_jobs
            .get(&job.schedule.completes_at)
            .is_some_and(|ids| ids.contains(id));
        if job.schedule.suspension.is_some() && is_indexed {
            return Err(ProductionValidationError::SuspendedJobInDueIndex {
                job: *id,
                due: job.schedule.completes_at,
            });
        }
        if job.schedule.suspension.is_none() && !is_indexed {
            return Err(ProductionValidationError::MissingDueIndex {
                job: *id,
                due: job.schedule.completes_at,
            });
        }
    }

    for (due, ids) in &state.due_jobs {
        if ids.is_empty() {
            return Err(ProductionValidationError::EmptyDueIndex { due: *due });
        }
        for id in ids {
            let Some(job) = state.jobs.get(id) else {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            };
            if job.schedule.suspension.is_some() {
                return Err(ProductionValidationError::SuspendedJobInDueIndex {
                    job: *id,
                    due: *due,
                });
            }
            if job.schedule.completes_at != *due {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            }
        }
    }
    if let Some((store, indexed, expected)) = state
        .energy_occupancy_mismatch()
        .map_err(|store| ProductionValidationError::EnergyDoubleBooked { store })?
    {
        return Err(ProductionValidationError::EnergyOccupancyIndexMismatch {
            store,
            indexed,
            expected,
        });
    }
    if let Some((equipment, indexed, expected)) = state
        .equipment_occupancy_mismatch()
        .map_err(|equipment| ProductionValidationError::EquipmentDoubleBooked { equipment })?
    {
        return Err(ProductionValidationError::EquipmentOccupancyIndexMismatch {
            equipment,
            indexed,
            expected,
        });
    }
    if let Some(stockpile) = state.stockpile_occupancy_mismatch() {
        return Err(ProductionValidationError::StockpileOccupancyIndexMismatch { stockpile });
    }
    Ok(())
}
