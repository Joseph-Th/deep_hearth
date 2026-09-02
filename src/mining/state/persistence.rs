//! Crate-owned mining serialization that preserves hidden execution proofs only in save output.

use std::collections::BTreeMap;

use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Serialize, Serializer};

use super::{MiningJobId, MiningJobRecord, MiningState};

struct PersistentMiningJob<'job>(&'job MiningJobRecord);

impl Serialize for PersistentMiningJob<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut record = serializer.serialize_struct("MiningJobRecord", 3)?;
        record.serialize_field("identity", &self.0.identity)?;
        record.serialize_field("resources", &self.0.resources)?;
        record.serialize_field("schedule", &self.0.schedule)?;
        record.end()
    }
}

struct PersistentMiningJobs<'jobs>(&'jobs BTreeMap<MiningJobId, MiningJobRecord>);

impl Serialize for PersistentMiningJobs<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut jobs = serializer.serialize_map(Some(self.0.len()))?;
        for (id, record) in self.0 {
            jobs.serialize_entry(id, &PersistentMiningJob(record))?;
        }
        jobs.end()
    }
}

/// Complete mining persistence representation, kept crate-owned so the public read state cannot be
/// serialized into hidden deposit bindings or reserved output profiles.
pub(crate) fn serialize_mining_state<S>(
    state: &MiningState,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut persisted = serializer.serialize_struct("MiningState", 3)?;
    persisted.serialize_field("revision", &state.revision)?;
    persisted.serialize_field("next_job_id", &state.next_job_id)?;
    persisted.serialize_field("jobs", &PersistentMiningJobs(&state.jobs))?;
    persisted.end()
}
