//! Persistent mining work-in-progress state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
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
    pub(super) deposit_mass_before: Mass,
    pub(super) output: MaterialLotSpec,
    pub(super) equipment_condition_after: Condition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MiningJobPhase {
    Working,
    ReadyToClaim,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MiningJobSchedule {
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) phase: MiningJobPhase,
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
    pub(crate) const fn deposit_mass_before(&self) -> Mass {
        self.resources.deposit_mass_before
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
    pub const fn is_working(&self) -> bool {
        matches!(self.schedule.phase, MiningJobPhase::Working)
    }
    #[must_use]
    pub const fn is_ready_to_claim(&self) -> bool {
        matches!(self.schedule.phase, MiningJobPhase::ReadyToClaim)
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

    pub(crate) fn assert_job_insertable(
        &self,
        record: &MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        assert_eq!(
            record.id().value(),
            self.next_job_id,
            "mining job allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_job_id.checked_add(1),
            Some(next_job_id),
            "mining job allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "mining job allocation must advance the owner revision exactly once"
        );
        assert!(record.is_working());
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
    }

    pub(crate) fn insert_job(
        &mut self,
        record: MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        self.assert_job_insertable(&record, next_job_id, next_revision);
        let id = record.identity.id;
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
        completion_tick: SimulationTick,
    ) -> Vec<MiningJobId> {
        self.assert_due_jobs_ready_available(expected_revision, next_revision, completion_tick);
        let jobs = self
            .due_jobs
            .remove(&completion_tick)
            .unwrap_or_else(|| unreachable!("due mining bucket was prechecked"));
        let mut ready = Vec::with_capacity(jobs.len());
        for id in jobs {
            let record = self
                .jobs
                .get_mut(&id)
                .unwrap_or_else(|| unreachable!("due mining job was prechecked"));
            record.schedule.phase = MiningJobPhase::ReadyToClaim;
            let removed = self.equipment_occupancy.remove(&record.equipment());
            assert_eq!(removed, Some(id));
            ready.push(id);
        }
        self.revision = next_revision;
        ready
    }

    pub(crate) fn assert_due_jobs_ready_available(
        &self,
        expected_revision: u64,
        next_revision: u64,
        completion_tick: SimulationTick,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let jobs = self
            .due_jobs
            .get(&completion_tick)
            .unwrap_or_else(|| panic!("validated due mining bucket disappeared"));
        assert!(!jobs.is_empty(), "due mining bucket cannot be empty");
        for &id in jobs {
            let record = self
                .jobs
                .get(&id)
                .unwrap_or_else(|| panic!("validated mining job disappeared"));
            assert!(record.is_working());
            assert_eq!(record.completes_at(), completion_tick);
            assert_eq!(self.equipment_occupancy.get(&record.equipment()), Some(&id));
        }
    }

    pub(crate) fn assert_ready_job_removable(
        &self,
        id: MiningJobId,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let record = self
            .jobs
            .get(&id)
            .unwrap_or_else(|| panic!("validated mining claim job disappeared"));
        assert!(record.is_ready_to_claim());
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "ready mining job remained in due index"
        );
        assert!(
            self.equipment_occupancy.values().all(|job| *job != id),
            "ready mining job retained equipment occupancy"
        );
    }

    pub(crate) fn remove_ready_job(
        &mut self,
        id: MiningJobId,
        expected_revision: u64,
        next_revision: u64,
    ) -> MiningJobRecord {
        self.assert_ready_job_removable(id, expected_revision, next_revision);
        let record = self
            .jobs
            .remove(&id)
            .unwrap_or_else(|| unreachable!("ready mining claim job was prechecked"));
        self.revision = next_revision;
        record
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
            && self
                .jobs
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_job_id)
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }
}

mod validation;

pub use validation::MiningValidationError;
pub(crate) use validation::validate_loaded_mining;
