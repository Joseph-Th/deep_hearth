//! Persistent player-owned geological knowledge and deterministic lookup indexes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::material::MaterialId;

use super::{GeologicalObservationId, GeologicalObservationRecord};

/// Player-accessible geological knowledge, separate from exact authoritative deposit truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalKnowledgeState {
    pub(super) revision: u64,
    pub(super) next_observation_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    pub(super) observations: BTreeMap<GeologicalObservationId, GeologicalObservationRecord>,
    #[serde(skip)]
    pub(super) observations_by_material: BTreeMap<MaterialId, BTreeSet<GeologicalObservationId>>,
}

impl GeologicalKnowledgeState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_observation_id: 1,
            observations: BTreeMap::new(),
            observations_by_material: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(in crate::geology) const fn next_observation_id(&self) -> u32 {
        self.next_observation_id
    }

    #[must_use]
    pub fn get_observation(
        &self,
        id: GeologicalObservationId,
    ) -> Option<&GeologicalObservationRecord> {
        self.observations.get(&id)
    }

    pub fn observations(&self) -> impl Iterator<Item = &GeologicalObservationRecord> {
        self.observations.values()
    }

    pub(super) fn observation_ids_for_material(
        &self,
        material: MaterialId,
    ) -> impl Iterator<Item = GeologicalObservationId> + '_ {
        self.observations_by_material
            .get(&material)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    /// Iterates materials for which at least one observation has been acquired.
    pub(super) fn known_materials(&self) -> impl Iterator<Item = MaterialId> + '_ {
        self.observations_by_material.keys().copied()
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut observations_by_material =
            BTreeMap::<MaterialId, BTreeSet<GeologicalObservationId>>::new();
        for observation in self.observations.values() {
            for finding in &observation.findings {
                observations_by_material
                    .entry(finding.material())
                    .or_default()
                    .insert(observation.id);
            }
        }
        self.observations_by_material = observations_by_material;
    }

    pub(in crate::geology) fn insert_observation(
        &mut self,
        record: GeologicalObservationRecord,
        next_observation_id: u32,
        next_revision: u64,
    ) {
        let id = record.id;
        assert_eq!(
            id.value(),
            self.next_observation_id,
            "geological observation allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_observation_id.checked_add(1),
            Some(next_observation_id),
            "geological observation allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "geological observation allocation must advance the owner revision exactly once"
        );
        assert!(
            !self.observations.contains_key(&id),
            "validated geological observation ID must be unique"
        );
        for finding in &record.findings {
            self.observations_by_material
                .entry(finding.material())
                .or_default()
                .insert(id);
        }
        let replaced = self.observations.insert(id, record);
        assert!(replaced.is_none(), "observation uniqueness was prechecked");
        self.next_observation_id = next_observation_id;
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_observation_id != 0
            && self
                .observations
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_observation_id)
    }
}
