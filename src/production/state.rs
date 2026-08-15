//! Durable production jobs and due-tick index; sibling execution code owns every mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::energy::{ConsumedEnergyTrace, ReleasedEnergyTrace};
use crate::equipment::EquipmentOperationTrace;
use crate::inventory::{ConsumedMaterialTrace, StockpileId};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionError, MaterialId, MaterialLotSpec};

use super::definitions::ProcessId;
use super::resolution::ProcessOutputStreamId;

/// Durable routing for one physically inseparable resolved output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionOutputStream {
    pub(super) id: ProcessOutputStreamId,
    pub(super) destination: StockpileId,
    pub(super) outputs: Vec<MaterialLotSpec>,
}

impl ProductionOutputStream {
    #[must_use]
    pub const fn id(&self) -> ProcessOutputStreamId {
        self.id
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }
}

/// Persistent monotonically allocated production job identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProductionJobId(u64);

impl ProductionJobId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "production job id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Durable running material transformation with capacity reserved until completion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionJobRecord {
    pub(super) id: ProductionJobId,
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) consumed_mass: Mass,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) consumed_energy: Option<ConsumedEnergyTrace>,
    pub(super) released_energy: Option<ReleasedEnergyTrace>,
    pub(super) equipment_provider: Option<EquipmentOperationTrace>,
    pub(super) equipment_condition_after: Option<Condition>,
    pub(super) output_streams: Vec<ProductionOutputStream>,
}

impl ProductionJobRecord {
    #[must_use]
    pub const fn id(&self) -> ProductionJobId {
        self.id
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.completes_at
    }

    #[must_use]
    pub const fn consumed_mass(&self) -> Mass {
        self.consumed_mass
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    /// Returns the finite energy moved into this in-flight operation at start, if any.
    #[must_use]
    pub const fn consumed_energy(&self) -> Option<ConsumedEnergyTrace> {
        self.consumed_energy
    }

    /// Returns exact energy released from process inputs and awaiting sink commit, if any.
    #[must_use]
    pub const fn released_energy(&self) -> Option<ReleasedEnergyTrace> {
        self.released_energy
    }

    /// Returns the equipment provider exclusively occupied by this operation, if any.
    #[must_use]
    pub const fn equipment_provider(&self) -> Option<EquipmentOperationTrace> {
        self.equipment_provider
    }

    /// Returns the persisted post-operation condition for the occupied equipment provider.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment_condition_after
    }

    /// Returns exact material streams and their committed destinations.
    #[must_use]
    pub fn output_streams(&self) -> &[ProductionOutputStream] {
        &self.output_streams
    }

    /// Returns the sole durable stream for process families that require single-stream output.
    #[must_use]
    pub fn single_output_stream(&self) -> Option<&ProductionOutputStream> {
        let [stream] = self.output_streams.as_slice() else {
            return None;
        };
        Some(stream)
    }

    pub(crate) fn involves_stockpile(&self, stockpile: StockpileId) -> bool {
        self.source == stockpile
            || self
                .output_streams
                .iter()
                .any(|stream| stream.destination == stockpile)
    }
}

/// Runtime owner for active process jobs and the deterministic due-tick index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionState {
    pub(super) revision: u64,
    pub(super) next_job_id: u64,
    pub(super) jobs: BTreeMap<ProductionJobId, ProductionJobRecord>,
    pub(super) due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
}

impl ProductionState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            due_jobs: BTreeMap::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
    }

    pub(crate) fn has_valid_equipment_condition_outcomes(&self) -> bool {
        self.jobs.values().all(|job| {
            match (job.equipment_provider, job.equipment_condition_after) {
                (Some(provider), Some(after)) => after <= provider.condition(),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
        })
    }

    pub(crate) fn has_unique_energy_reservations(&self) -> bool {
        let mut occupied = BTreeSet::new();
        for job in self.jobs.values() {
            if let Some(trace) = job.consumed_energy
                && !occupied.insert(trace.source())
            {
                return false;
            }
            if let Some(trace) = job.released_energy
                && !occupied.insert(trace.destination())
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    /// Returns one active process job by stable runtime ID.
    #[must_use]
    pub fn get_job(&self, id: ProductionJobId) -> Option<&ProductionJobRecord> {
        self.jobs.get(&id)
    }

    /// Iterates active jobs deterministically by stable runtime ID.
    pub fn jobs(&self) -> impl Iterator<Item = &ProductionJobRecord> {
        self.jobs.values()
    }

    pub(super) fn insert_job(
        &mut self,
        job: ProductionJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        let id = job.id;
        let completes_at = job.completes_at;
        let replaced = self.jobs.insert(id, job);
        assert!(
            replaced.is_none(),
            "validated production job ID must be unique"
        );
        let inserted = self.due_jobs.entry(completes_at).or_default().insert(id);
        assert!(
            inserted,
            "production due index must not contain duplicate job IDs"
        );
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(super) fn remove_job(&mut self, id: ProductionJobId) -> ProductionJobRecord {
        let job = match self.jobs.remove(&id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: missing production job {}",
                id.value()
            ),
        };
        let due_set = match self.due_jobs.get_mut(&job.completes_at) {
            Some(due_set) => due_set,
            None => panic!(
                "runtime invariant broken: missing due index for production job {}",
                id.value()
            ),
        };
        assert!(
            due_set.remove(&id),
            "runtime invariant broken: due index missing production job {}",
            id.value()
        );
        if due_set.is_empty() {
            self.due_jobs.remove(&job.completes_at);
        }
        job
    }
}

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
        if id.value() == 0 || job.id.value() == 0 {
            return Err(ProductionValidationError::ZeroJobId);
        }
        if *id != job.id {
            return Err(ProductionValidationError::JobIdMismatch {
                key: *id,
                record: job.id,
            });
        }
        if job.completes_at <= job.started_at {
            return Err(ProductionValidationError::CompletionNotAfterStart { job: *id });
        }
        if job.output_streams.is_empty() {
            return Err(ProductionValidationError::NoOutputs { job: *id });
        }
        if job.consumed_inputs.is_empty() {
            return Err(ProductionValidationError::NoConsumedInputs { job: *id });
        }
        let mut traced_input_mass = Mass::ZERO;
        for trace in &job.consumed_inputs {
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
            if trace.provenance().latest_created_at() > job.started_at {
                return Err(ProductionValidationError::ConsumedInputCreatedAfterStart {
                    job: *id,
                    latest_created_at: trace.provenance().latest_created_at(),
                    started_at: job.started_at,
                });
            }
            traced_input_mass = traced_input_mass
                .checked_add(trace.mass())
                .ok_or(ProductionValidationError::ConsumedInputMassOverflow { job: *id })?;
        }
        if traced_input_mass != job.consumed_mass {
            return Err(ProductionValidationError::ConsumedInputMassMismatch {
                job: *id,
                traced: traced_input_mass,
                consumed: job.consumed_mass,
            });
        }
        if let Some(trace) = job.consumed_energy {
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
        if let Some(trace) = job.released_energy {
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
        match (job.equipment_provider, job.equipment_condition_after) {
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
        if output_mass != job.consumed_mass {
            return Err(ProductionValidationError::OutputMassMismatch {
                job: *id,
                output: output_mass,
                consumed: job.consumed_mass,
            });
        }
        let is_indexed = state
            .due_jobs
            .get(&job.completes_at)
            .is_some_and(|ids| ids.contains(id));
        if !is_indexed {
            return Err(ProductionValidationError::MissingDueIndex {
                job: *id,
                due: job.completes_at,
            });
        }
    }

    for (due, ids) in &state.due_jobs {
        for id in ids {
            let Some(job) = state.jobs.get(id) else {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            };
            if job.completes_at != *due {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            }
        }
    }
    Ok(())
}
