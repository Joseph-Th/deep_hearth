//! Persistent mining work-in-progress state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
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
pub struct MiningJobRecord {
    pub(super) id: MiningJobId,
    pub(super) method: MiningMethodId,
    pub(super) deposit: GeologicalDepositId,
    pub(super) destination: StockpileId,
    pub(super) equipment: EquipmentId,
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) output: MaterialLotSpec,
    pub(super) equipment_condition_before: Condition,
    pub(super) equipment_condition_after: Condition,
    pub(super) ready_at: Option<SimulationTick>,
}

impl MiningJobRecord {
    #[must_use]
    pub const fn id(&self) -> MiningJobId {
        self.id
    }
    #[must_use]
    pub const fn method(&self) -> MiningMethodId {
        self.method
    }
    #[must_use]
    pub const fn deposit(&self) -> GeologicalDepositId {
        self.deposit
    }
    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
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
    pub const fn output(&self) -> &MaterialLotSpec {
        &self.output
    }
    #[must_use]
    pub const fn equipment_condition_before(&self) -> Condition {
        self.equipment_condition_before
    }
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Condition {
        self.equipment_condition_after
    }
    #[must_use]
    pub const fn ready_at(&self) -> Option<SimulationTick> {
        self.ready_at
    }
    #[must_use]
    pub const fn is_working(&self) -> bool {
        self.ready_at.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiningState {
    revision: u64,
    next_job_id: u64,
    jobs: BTreeMap<MiningJobId, MiningJobRecord>,
    due_jobs: BTreeMap<SimulationTick, BTreeSet<MiningJobId>>,
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

    pub(crate) fn jobs_due_at(&self, tick: SimulationTick) -> BTreeSet<MiningJobId> {
        self.due_jobs.get(&tick).cloned().unwrap_or_default()
    }

    pub(crate) fn get_equipment_occupant(&self, equipment: EquipmentId) -> Option<MiningJobId> {
        self.equipment_occupancy.get(&equipment).copied()
    }

    pub(crate) fn insert_job(
        &mut self,
        record: MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        assert!(record.ready_at.is_none());
        assert!(!self.equipment_occupancy.contains_key(&record.equipment));
        let id = record.id;
        self.due_jobs
            .entry(record.completes_at)
            .or_default()
            .insert(id);
        self.equipment_occupancy.insert(record.equipment, id);
        assert!(self.jobs.insert(id, record).is_none());
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(crate) fn mark_ready_batch(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        ready_at: SimulationTick,
        jobs: &BTreeSet<MiningJobId>,
    ) {
        assert_eq!(self.revision, expected_revision);
        if jobs.is_empty() {
            return;
        }
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let indexed = self
            .due_jobs
            .remove(&ready_at)
            .unwrap_or_else(|| panic!("validated due mining bucket disappeared"));
        assert_eq!(&indexed, jobs);
        for id in jobs {
            let record = self
                .jobs
                .get_mut(id)
                .unwrap_or_else(|| panic!("validated mining job disappeared"));
            assert!(record.ready_at.is_none());
            record.ready_at = Some(ready_at);
            let removed = self.equipment_occupancy.remove(&record.equipment);
            assert_eq!(removed, Some(*id));
        }
        self.revision = next_revision;
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
        assert!(record.ready_at.is_some());
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
