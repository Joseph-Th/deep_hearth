//! Persistent mining work-in-progress state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentOperationTrace};
use crate::geology::GeologicalDepositId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::MiningMethodId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MiningJobId(u64);

impl MiningJobId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "mining job id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MiningJobIdentity {
    pub(super) id: MiningJobId,
    pub(super) method: MiningMethodId,
    pub(super) deposit: GeologicalDepositId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MiningJobResources {
    pub(super) destination: StockpileId,
    pub(super) equipment_trace: EquipmentOperationTrace,
    pub(super) output: MaterialLotSpec,
    pub(super) equipment_condition_after: Condition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MiningJobSchedule {
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) ready_at: Option<SimulationTick>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiningJobRecord {
    identity: MiningJobIdentity,
    resources: MiningJobResources,
    schedule: MiningJobSchedule,
}

impl MiningJobRecord {
    pub(super) const fn new(
        identity: MiningJobIdentity,
        resources: MiningJobResources,
        schedule: MiningJobSchedule,
    ) -> Self {
        Self {
            identity,
            resources,
            schedule,
        }
    }

    #[must_use]
    pub const fn id(&self) -> MiningJobId {
        self.identity.id
    }
    #[must_use]
    pub const fn method(&self) -> MiningMethodId {
        self.identity.method
    }
    #[must_use]
    pub(crate) const fn deposit(&self) -> GeologicalDepositId {
        self.identity.deposit
    }
    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.resources.destination
    }
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.resources.equipment_trace.equipment()
    }
    #[must_use]
    pub const fn equipment_definition(&self) -> EquipmentDefinitionId {
        self.resources.equipment_trace.definition()
    }
    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.schedule.started_at
    }
    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.schedule.completes_at
    }
    #[must_use]
    pub(crate) const fn output(&self) -> &MaterialLotSpec {
        &self.resources.output
    }
    #[must_use]
    pub const fn equipment_condition_before(&self) -> Condition {
        self.resources.equipment_trace.condition()
    }
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Condition {
        self.resources.equipment_condition_after
    }
    #[must_use]
    pub const fn ready_at(&self) -> Option<SimulationTick> {
        self.schedule.ready_at
    }
    #[must_use]
    pub const fn is_working(&self) -> bool {
        self.schedule.ready_at.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MiningState {
    revision: u64,
    next_job_id: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    jobs: BTreeMap<MiningJobId, MiningJobRecord>,
    #[serde(skip)]
    due_jobs: BTreeMap<SimulationTick, BTreeSet<MiningJobId>>,
    #[serde(skip)]
    equipment_occupancy: BTreeMap<EquipmentId, MiningJobId>,
}

impl MiningState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            due_jobs: BTreeMap::new(),
            equipment_occupancy: BTreeMap::new(),
        }
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub(crate) const fn next_job_id(&self) -> u64 {
        self.next_job_id
    }
    #[must_use]
    pub fn get_job(&self, id: MiningJobId) -> Option<&MiningJobRecord> {
        self.jobs.get(&id)
    }
    pub fn jobs(&self) -> impl Iterator<Item = &MiningJobRecord> {
        self.jobs.values()
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut due_jobs = BTreeMap::<SimulationTick, BTreeSet<MiningJobId>>::new();
        let mut equipment_occupancy = BTreeMap::<EquipmentId, MiningJobId>::new();
        for job in self.jobs.values().filter(|job| job.is_working()) {
            due_jobs
                .entry(job.completes_at())
                .or_default()
                .insert(job.id());
            equipment_occupancy
                .entry(job.equipment())
                .or_insert(job.id());
        }
        self.due_jobs = due_jobs;
        self.equipment_occupancy = equipment_occupancy;
    }

    pub(crate) fn get_equipment_occupant(&self, equipment: EquipmentId) -> Option<MiningJobId> {
        self.equipment_occupancy.get(&equipment).copied()
    }

    pub(crate) fn jobs_due_at(&self, tick: SimulationTick) -> Option<&BTreeSet<MiningJobId>> {
        self.due_jobs.get(&tick)
    }

    pub(crate) fn insert_job(
        &mut self,
        record: MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        assert!(record.schedule.ready_at.is_none());
        assert!(!self.equipment_occupancy.contains_key(&record.equipment()));
        let id = record.identity.id;
        assert!(
            !self.jobs.contains_key(&id),
            "validated mining job ID must be unique"
        );
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "runtime invariant broken: mining due index already contains job {}",
            id.value()
        );
        let inserted = self
            .due_jobs
            .entry(record.schedule.completes_at)
            .or_default()
            .insert(id);
        assert!(
            inserted,
            "prechecked mining due index rejected job {}",
            id.value()
        );
        let previous = self.equipment_occupancy.insert(record.equipment(), id);
        assert!(
            previous.is_none(),
            "prechecked mining equipment occupancy replaced an existing job"
        );
        let previous = self.jobs.insert(id, record);
        assert!(previous.is_none(), "prechecked mining job ID was replaced");
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(crate) fn mark_due_jobs_ready(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        ready_at: SimulationTick,
    ) -> Vec<MiningJobId> {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let jobs = self
            .due_jobs
            .remove(&ready_at)
            .unwrap_or_else(|| panic!("validated due mining bucket disappeared"));
        let mut ready = Vec::with_capacity(jobs.len());
        for id in jobs {
            let record = self
                .jobs
                .get_mut(&id)
                .unwrap_or_else(|| panic!("validated mining job disappeared"));
            assert!(record.schedule.ready_at.is_none());
            record.schedule.ready_at = Some(ready_at);
            let removed = self.equipment_occupancy.remove(&record.equipment());
            assert_eq!(removed, Some(id));
            ready.push(id);
        }
        self.revision = next_revision;
        ready
    }

    pub(crate) fn remove_ready_job(
        &mut self,
        id: MiningJobId,
        expected_revision: u64,
        next_revision: u64,
    ) -> MiningJobRecord {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let record = self
            .jobs
            .remove(&id)
            .unwrap_or_else(|| panic!("validated mining claim job disappeared"));
        assert!(record.schedule.ready_at.is_some());
        self.revision = next_revision;
        record
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }
}

mod validation;

pub use validation::MiningValidationError;
pub(crate) use validation::validate_loaded_mining;
