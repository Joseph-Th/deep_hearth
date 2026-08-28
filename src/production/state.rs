//! Durable production jobs and synchronized scheduling/resource indexes; child validation audits persistence.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ConsumedEnergyTrace, EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{ConsumedMaterialTrace, MaterialStorageHistory, StockpileId};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::definitions::ProcessId;
use super::resolution::ProcessOutputStreamId;

/// Durable routing for one physically inseparable resolved output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionOutputStream {
    pub(super) id: ProcessOutputStreamId,
    pub(super) destination: StockpileId,
    pub(super) outputs: Vec<MaterialLotSpec>,
}

/// Why an in-flight production job is currently unable to accumulate active process time.
///
/// Suspension never manufactures a failure product. The production job remains the authoritative
/// owner of its consumed matter and energy until its physical requirements become usable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProductionSuspensionReason {
    EquipmentSupportUnavailable { equipment: EquipmentId },
    OutputSupportUnavailable { stockpile: StockpileId },
    PlayerLaborUnavailable,
}

/// When an occupied resource can become available to unrelated work.
///
/// Running jobs have a scheduled wall-clock release. Suspended jobs deliberately do not expose their
/// stale pre-suspension completion tick as a promise: release depends on physical recovery and resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionOccupancyRelease {
    Scheduled(SimulationTick),
    AwaitingResume,
}

impl Display for ProductionOccupancyRelease {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheduled(tick) => write!(formatter, "until tick {}", tick.value()),
            Self::AwaitingResume => {
                formatter.write_str("while its production job is suspended awaiting recovery")
            }
        }
    }
}

/// Durable pause state for one production job whose active-time clock is not currently advancing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionSuspension {
    suspended_at: SimulationTick,
    remaining_active_time: TickSpan,
    reason: ProductionSuspensionReason,
}

impl ProductionSuspension {
    pub(super) const fn new(
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) -> Self {
        Self {
            suspended_at,
            remaining_active_time,
            reason,
        }
    }

    #[must_use]
    pub const fn suspended_at(self) -> SimulationTick {
        self.suspended_at
    }

    #[must_use]
    pub const fn remaining_active_time(self) -> TickSpan {
        self.remaining_active_time
    }

    #[must_use]
    pub const fn reason(self) -> ProductionSuspensionReason {
        self.reason
    }
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

type ProductionOccupancyMismatch<Resource> =
    (Resource, Option<ProductionJobId>, Option<ProductionJobId>);

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
#[serde(deny_unknown_fields)]
pub struct ProductionJobRecord {
    pub(super) identity: ProductionJobIdentity,
    pub(super) schedule: ProductionJobSchedule,
    pub(super) resources: ProductionJobResources,
    pub(super) equipment: ProductionJobEquipment,
    pub(super) output_streams: Vec<ProductionOutputStream>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobIdentity {
    pub(super) id: ProductionJobId,
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobSchedule {
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) active_duration: TickSpan,
    pub(super) suspension: Option<ProductionSuspension>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobResources {
    pub(super) consumed_mass: Mass,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) material_storage_history: MaterialStorageHistory,
    pub(super) consumed_energy: Option<ConsumedEnergyTrace>,
    pub(super) released_energy: Option<ReleasedEnergyTrace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobEquipment {
    pub(super) provider: Option<EquipmentOperationTrace>,
    pub(super) requires_active_support: bool,
    pub(super) condition_after: Option<Condition>,
}

impl ProductionJobRecord {
    #[must_use]
    pub const fn id(&self) -> ProductionJobId {
        self.identity.id
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.identity.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.identity.source
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.schedule.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.schedule.completes_at
    }

    /// Returns the authored/resolved amount of active process time required by this operation.
    /// Wall-clock suspension never changes this physics contract.
    #[must_use]
    pub const fn active_duration(&self) -> TickSpan {
        self.schedule.active_duration
    }

    /// Returns the current suspension state, if this job is retaining work-in-process while paused.
    #[must_use]
    pub const fn suspension(&self) -> Option<ProductionSuspension> {
        self.schedule.suspension
    }

    #[must_use]
    pub const fn is_suspended(&self) -> bool {
        self.schedule.suspension.is_some()
    }

    /// Returns the externally meaningful release horizon for resources exclusively owned by this
    /// job. A suspended operation has no scheduled release until it resumes.
    #[must_use]
    pub const fn occupancy_release(&self) -> ProductionOccupancyRelease {
        if self.schedule.suspension.is_some() {
            ProductionOccupancyRelease::AwaitingResume
        } else {
            ProductionOccupancyRelease::Scheduled(self.schedule.completes_at)
        }
    }

    #[must_use]
    pub const fn consumed_mass(&self) -> Mass {
        self.resources.consumed_mass
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.resources.consumed_inputs
    }

    /// Returns inherited perishability exposure for the in-flight matter, rebased to job start.
    #[must_use]
    pub(crate) const fn material_storage_history(&self) -> MaterialStorageHistory {
        self.resources.material_storage_history
    }

    /// Returns the finite energy moved into this in-flight operation at start, if any.
    #[must_use]
    pub const fn consumed_energy(&self) -> Option<ConsumedEnergyTrace> {
        self.resources.consumed_energy
    }

    /// Returns exact energy released from process inputs and awaiting sink commit, if any.
    #[must_use]
    pub const fn released_energy(&self) -> Option<ReleasedEnergyTrace> {
        self.resources.released_energy
    }

    /// Returns the equipment provider exclusively occupied by this operation, if any.
    #[must_use]
    pub const fn equipment_provider(&self) -> Option<EquipmentOperationTrace> {
        self.equipment.provider
    }

    /// Whether this operation was authorized only while its equipment had an active structural
    /// support. Unsupported/free-standing providers do not acquire this requirement implicitly.
    #[must_use]
    pub const fn has_required_active_support(&self) -> bool {
        self.equipment.requires_active_support
    }

    /// Returns the persisted post-operation condition for the occupied equipment provider.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment.condition_after
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

    fn index_projection(&self) -> ProductionJobIndexProjection {
        ProductionJobIndexProjection {
            due_tick: (!self.is_suspended()).then_some(self.completes_at()),
            energy_stores: self
                .resources
                .consumed_energy
                .map(|trace| trace.source())
                .into_iter()
                .chain(
                    self.resources
                        .released_energy
                        .map(|trace| trace.destination()),
                )
                .collect(),
            equipment: self.equipment.provider.map(|provider| provider.equipment()),
            output_stockpiles: self
                .output_streams
                .iter()
                .map(|stream| stream.destination)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProductionJobIndexProjection {
    due_tick: Option<SimulationTick>,
    energy_stores: Vec<EnergyStoreId>,
    equipment: Option<EquipmentId>,
    output_stockpiles: BTreeSet<StockpileId>,
}

/// Runtime owner for active process jobs and deterministic scheduling/resource indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionState {
    revision: u64,
    next_job_id: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    jobs: BTreeMap<ProductionJobId, ProductionJobRecord>,
    #[serde(skip)]
    due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
    #[serde(skip)]
    energy_occupancy: BTreeMap<EnergyStoreId, ProductionJobId>,
    #[serde(skip)]
    equipment_occupancy: BTreeMap<EquipmentId, ProductionJobId>,
    #[serde(skip)]
    output_stockpile_occupancy: BTreeMap<StockpileId, BTreeSet<ProductionJobId>>,
}

impl ProductionState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            due_jobs: BTreeMap::new(),
            energy_occupancy: BTreeMap::new(),
            equipment_occupancy: BTreeMap::new(),
            output_stockpile_occupancy: BTreeMap::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) const fn next_job_id(&self) -> u64 {
        self.next_job_id
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut due_jobs = BTreeMap::<SimulationTick, BTreeSet<ProductionJobId>>::new();
        let mut energy_occupancy = BTreeMap::<EnergyStoreId, ProductionJobId>::new();
        let mut equipment_occupancy = BTreeMap::<EquipmentId, ProductionJobId>::new();
        let mut output_stockpile_occupancy =
            BTreeMap::<StockpileId, BTreeSet<ProductionJobId>>::new();
        for job in self.jobs.values() {
            let projection = job.index_projection();
            if let Some(due_tick) = projection.due_tick {
                due_jobs.entry(due_tick).or_default().insert(job.id());
            }
            for store in projection.energy_stores {
                energy_occupancy.entry(store).or_insert(job.id());
            }
            if let Some(equipment) = projection.equipment {
                equipment_occupancy.entry(equipment).or_insert(job.id());
            }
            for stockpile in projection.output_stockpiles {
                output_stockpile_occupancy
                    .entry(stockpile)
                    .or_default()
                    .insert(job.id());
            }
        }
        self.due_jobs = due_jobs;
        self.energy_occupancy = energy_occupancy;
        self.equipment_occupancy = equipment_occupancy;
        self.output_stockpile_occupancy = output_stockpile_occupancy;
    }

    fn expected_energy_occupancy(
        &self,
    ) -> Result<BTreeMap<EnergyStoreId, ProductionJobId>, EnergyStoreId> {
        let mut occupied = BTreeMap::new();
        for job in self.jobs.values() {
            for store in job.index_projection().energy_stores {
                if occupied.insert(store, job.identity.id).is_some() {
                    return Err(store);
                }
            }
        }
        Ok(occupied)
    }

    fn energy_occupancy_mismatch(
        &self,
    ) -> Result<Option<ProductionOccupancyMismatch<EnergyStoreId>>, EnergyStoreId> {
        let expected = self.expected_energy_occupancy()?;
        let stores = self
            .energy_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for store in stores {
            let indexed = self.energy_occupancy.get(&store).copied();
            let expected = expected.get(&store).copied();
            if indexed != expected {
                return Ok(Some((store, indexed, expected)));
            }
        }
        Ok(None)
    }

    fn expected_equipment_occupancy(
        &self,
    ) -> Result<BTreeMap<EquipmentId, ProductionJobId>, EquipmentId> {
        let mut occupied = BTreeMap::new();
        for job in self.jobs.values() {
            if let Some(equipment) = job.index_projection().equipment
                && occupied.insert(equipment, job.identity.id).is_some()
            {
                return Err(equipment);
            }
        }
        Ok(occupied)
    }

    fn equipment_occupancy_mismatch(
        &self,
    ) -> Result<Option<ProductionOccupancyMismatch<EquipmentId>>, EquipmentId> {
        let expected = self.expected_equipment_occupancy()?;
        let equipment_ids = self
            .equipment_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for equipment in equipment_ids {
            let indexed = self.equipment_occupancy.get(&equipment).copied();
            let expected = expected.get(&equipment).copied();
            if indexed != expected {
                return Ok(Some((equipment, indexed, expected)));
            }
        }
        Ok(None)
    }

    fn expected_output_stockpile_occupancy(
        &self,
    ) -> BTreeMap<StockpileId, BTreeSet<ProductionJobId>> {
        let mut occupied = BTreeMap::<StockpileId, BTreeSet<ProductionJobId>>::new();
        for job in self.jobs.values() {
            for stockpile in job.index_projection().output_stockpiles {
                occupied
                    .entry(stockpile)
                    .or_default()
                    .insert(job.identity.id);
            }
        }
        occupied
    }

    fn output_stockpile_occupancy_mismatch(&self) -> Option<StockpileId> {
        let expected = self.expected_output_stockpile_occupancy();
        let stockpiles = self
            .output_stockpile_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        stockpiles.into_iter().find(|stockpile| {
            self.output_stockpile_occupancy.get(stockpile) != expected.get(stockpile)
        })
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    pub(super) fn jobs_due_at(&self, tick: SimulationTick) -> BTreeSet<ProductionJobId> {
        self.due_jobs.get(&tick).cloned().unwrap_or_default()
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

    /// Returns the active production job that exclusively reserves one finite energy store.
    #[must_use]
    pub(crate) fn get_energy_occupant(&self, store: EnergyStoreId) -> Option<ProductionJobId> {
        self.energy_occupancy.get(&store).copied()
    }

    /// Returns the active production job that exclusively occupies one equipment instance.
    #[must_use]
    pub(crate) fn get_equipment_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<&ProductionJobRecord> {
        self.equipment_occupancy
            .get(&equipment)
            .and_then(|job| self.jobs.get(job))
    }

    /// Returns the lowest-ID running production job with in-flight output reserved for a stockpile.
    /// Suspended jobs do not block support relocation because their active-time clock is stopped.
    #[must_use]
    pub(crate) fn get_running_output_stockpile_occupant(
        &self,
        stockpile: StockpileId,
    ) -> Option<&ProductionJobRecord> {
        self.output_stockpile_occupancy
            .get(&stockpile)
            .and_then(|jobs| {
                jobs.iter()
                    .find_map(|job| self.jobs.get(job).filter(|record| !record.is_suspended()))
            })
    }

    fn assert_job_indexes_available(
        &self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        assert_eq!(
            projection.energy_stores.len(),
            projection
                .energy_stores
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            "validated production job cannot reserve one energy store more than once"
        );
        for store in &projection.energy_stores {
            assert!(
                !self.energy_occupancy.contains_key(store),
                "validated production job cannot replace an existing energy-store reservation"
            );
        }
        if let Some(equipment) = projection.equipment {
            assert!(
                !self.equipment_occupancy.contains_key(&equipment),
                "validated production job cannot replace an existing equipment reservation"
            );
        }
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
        for stockpile in &projection.output_stockpiles {
            assert!(
                !self
                    .output_stockpile_occupancy
                    .get(stockpile)
                    .is_some_and(|occupants| occupants.contains(&id)),
                "runtime invariant broken: production output-stockpile occupancy already contains job {}",
                id.value()
            );
        }
    }

    fn insert_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
        assert!(
            self.due_jobs.entry(due).or_default().insert(id),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
    }

    fn remove_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
        let remove_bucket = {
            let due_jobs = self.due_jobs.get_mut(&due).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: production due index is missing job {}",
                    id.value()
                )
            });
            assert!(
                due_jobs.remove(&id),
                "runtime invariant broken: production due index is missing job {}",
                id.value()
            );
            due_jobs.is_empty()
        };
        if remove_bucket {
            self.due_jobs.remove(&due);
        }
    }

    fn insert_job_indexes(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        if let Some(due_tick) = projection.due_tick {
            self.insert_due_job(id, due_tick);
        }
        for store in projection.energy_stores.iter().copied() {
            assert!(
                self.energy_occupancy.insert(store, id).is_none(),
                "runtime invariant broken: production energy occupancy replaced an existing job"
            );
        }
        if let Some(equipment) = projection.equipment {
            assert!(
                self.equipment_occupancy.insert(equipment, id).is_none(),
                "runtime invariant broken: production equipment occupancy replaced an existing job"
            );
        }
        for stockpile in &projection.output_stockpiles {
            assert!(
                self.output_stockpile_occupancy
                    .entry(*stockpile)
                    .or_default()
                    .insert(id),
                "runtime invariant broken: production output-stockpile occupancy already contains job {}",
                id.value()
            );
        }
    }

    fn remove_job_indexes(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        if let Some(due_tick) = projection.due_tick {
            self.remove_due_job(id, due_tick);
        }
        for store in &projection.energy_stores {
            assert_eq!(
                self.energy_occupancy.remove(store),
                Some(id),
                "runtime invariant broken: energy occupancy index disagrees with production job {}",
                id.value()
            );
        }
        if let Some(equipment) = projection.equipment {
            assert_eq!(
                self.equipment_occupancy.remove(&equipment),
                Some(id),
                "runtime invariant broken: equipment occupancy index disagrees with production job {}",
                id.value()
            );
        }
        for stockpile in &projection.output_stockpiles {
            let remove_bucket = {
                let occupants = self
                    .output_stockpile_occupancy
                    .get_mut(stockpile)
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: output-stockpile occupancy index missing production job {}",
                            id.value()
                        )
                    });
                assert!(
                    occupants.remove(&id),
                    "runtime invariant broken: output-stockpile occupancy index disagrees with production job {}",
                    id.value()
                );
                occupants.is_empty()
            };
            if remove_bucket {
                self.output_stockpile_occupancy.remove(stockpile);
            }
        }
    }

    pub(super) fn insert_job(
        &mut self,
        job: ProductionJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        let id = job.identity.id;
        let projection = job.index_projection();
        assert!(
            !self.jobs.contains_key(&id),
            "validated production job ID must be unique"
        );
        self.assert_job_indexes_available(id, &projection);
        let replaced = self.jobs.insert(id, job);
        assert!(
            replaced.is_none(),
            "prechecked production job ID was replaced"
        );
        self.insert_job_indexes(id, &projection);
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(super) fn suspend_job(
        &mut self,
        id: ProductionJobId,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) {
        let due = match self.jobs.get(&id) {
            Some(record) => record.schedule.completes_at,
            None => panic!(
                "runtime invariant broken: production job {} disappeared before suspension",
                id.value()
            ),
        };
        self.remove_due_job(id, due);
        let record = match self.jobs.get_mut(&id) {
            Some(record) => record,
            None => unreachable!("production job existence was checked before due-index mutation"),
        };
        assert!(
            record.schedule.suspension.is_none(),
            "runtime invariant broken: already-suspended job received another suspension"
        );
        record.schedule.suspension = Some(ProductionSuspension::new(
            suspended_at,
            remaining_active_time,
            reason,
        ));
    }

    pub(super) fn resume_job(&mut self, id: ProductionJobId, scheduled_completion: SimulationTick) {
        let record = match self.jobs.get_mut(&id) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: production job {} disappeared before resume",
                id.value()
            ),
        };
        assert!(
            record.schedule.suspension.is_some(),
            "runtime invariant broken: running job received a resume transition"
        );
        record.schedule.completes_at = scheduled_completion;
        record.schedule.suspension = None;
        self.insert_due_job(id, scheduled_completion);
    }

    pub(super) fn change_suspension_reason(
        &mut self,
        id: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    ) {
        let record = self.jobs.get_mut(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} disappeared before suspension reason change",
                id.value()
            )
        });
        let suspension = record.schedule.suspension.as_mut().unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: running job {} received a suspension reason change",
                id.value()
            )
        });
        assert_eq!(
            suspension.reason, previous,
            "runtime invariant broken: production suspension reason changed after planning"
        );
        assert_ne!(previous, reason);
        suspension.reason = reason;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "production revision must advance exactly once per canonical mutation batch"
        );
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
        let projection = job.index_projection();
        self.remove_job_indexes(id, &projection);
        job
    }
}

mod validation;

pub use validation::ProductionValidationError;
pub(crate) use validation::validate_loaded_production;
