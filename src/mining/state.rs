//! Owns persistent mining work-in-progress state and its derived runtime indexes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};

use serde::Deserialize;

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;

mod indexes;
mod job;
mod lifecycle;
mod persistence;

pub use job::{MiningJobId, MiningJobRecord};
pub(in crate::mining) use job::{
    MiningJobIdentity, MiningJobPhase, MiningJobResources, MiningJobSchedule,
};
pub(crate) use persistence::serialize_mining_state;

#[derive(Clone, PartialEq, Eq, Deserialize)]
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

impl Debug for MiningState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MiningState")
            .field("revision", &self.revision)
            .field("jobs", &self.jobs)
            .finish_non_exhaustive()
    }
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

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
            && self
                .jobs
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_job_id)
    }
}

mod validation;

pub use validation::MiningValidationError;
pub(crate) use validation::validate_loaded_mining;
