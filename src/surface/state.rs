//! Persistent finite loose surface-resource ownership.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use serde::{Deserialize, Serialize, Serializer};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::{ControlTick, SimulationTick};
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, MaterialLotProfile, MaterialLotProvenance,
    MaterialStorageHistory,
};
use crate::material::{CommodityKey, MaterialComposition, ParticleSizeRange};
use crate::spatial::{VoxelBounds, VoxelCoord};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceResourceId(u32);

impl SurfaceResourceId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "surface resource id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceResourceLifecycle {
    Available,
    Depleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceResourceKind {
    Opportunity,
    Drop,
}

/// Exact physical/provenance parcel retained while finite loose matter is in surface custody.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SurfaceMaterialParcel {
    initial_mass: Mass,
    profile: MaterialLotProfile,
    provenance: MaterialLotProvenance,
    ambient_age_parts_at_created_at: u128,
}

impl SurfaceMaterialParcel {
    pub(crate) fn existing(
        mass: Mass,
        profile: MaterialLotProfile,
        provenance: MaterialLotProvenance,
        ambient_age_parts_at_created_at: u128,
    ) -> Self {
        Self {
            initial_mass: mass,
            profile,
            provenance,
            ambient_age_parts_at_created_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResourceRecord {
    id: SurfaceResourceId,
    kind: SurfaceResourceKind,
    position: VoxelCoord,
    material: SurfaceMaterialParcel,
    remaining_mass: Mass,
    lifecycle: SurfaceResourceLifecycle,
    created_at: SimulationTick,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    automatic_pickup_delay_started_at: Option<ControlTick>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    depleted_at: Option<SimulationTick>,
}

impl SurfaceResourceRecord {
    pub(super) fn created(
        id: SurfaceResourceId,
        kind: SurfaceResourceKind,
        position: VoxelCoord,
        material: SurfaceMaterialParcel,
        created_at: SimulationTick,
        automatic_pickup_delay_started_at: Option<ControlTick>,
    ) -> Self {
        let mass = material.initial_mass;
        Self {
            id,
            kind,
            position,
            material,
            remaining_mass: mass,
            lifecycle: SurfaceResourceLifecycle::Available,
            created_at,
            automatic_pickup_delay_started_at,
            depleted_at: None,
        }
    }

    #[must_use]
    pub const fn id(&self) -> SurfaceResourceId {
        self.id
    }
    #[must_use]
    pub const fn kind(&self) -> SurfaceResourceKind {
        self.kind
    }
    #[must_use]
    pub const fn position(&self) -> VoxelCoord {
        self.position
    }
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.material.profile.commodity()
    }
    #[must_use]
    pub const fn initial_mass(&self) -> Mass {
        self.material.initial_mass
    }
    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }
    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.material.profile.temperature()
    }
    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        self.material.profile.composition()
    }
    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.material.profile.particle_size()
    }
    #[must_use]
    pub const fn lifecycle(&self) -> SurfaceResourceLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub(crate) const fn created_at(&self) -> SimulationTick {
        self.created_at
    }

    #[must_use]
    pub(crate) const fn automatic_pickup_delay_started_at(&self) -> Option<ControlTick> {
        self.automatic_pickup_delay_started_at
    }

    pub(crate) const fn material_profile(&self) -> &MaterialLotProfile {
        &self.material.profile
    }

    pub(crate) const fn material_provenance(&self) -> MaterialLotProvenance {
        self.material.provenance
    }

    pub(crate) fn ambient_storage_history_at(
        &self,
        at: SimulationTick,
    ) -> Option<MaterialStorageHistory> {
        MaterialStorageHistory::with_ambient_age_parts(
            self.material.ambient_age_parts_at_created_at,
            self.created_at,
        )
        .rebase(at, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
    }

    pub(crate) fn ambient_storage_age_parts_at(&self, at: SimulationTick) -> Option<u128> {
        self.ambient_storage_history_at(at)?
            .project(at, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
    }

    pub(crate) const fn ambient_age_parts_at_created_at(&self) -> u128 {
        self.material.ambient_age_parts_at_created_at
    }
}

#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "test-gameplay"), derive(Clone, PartialEq, Eq))]
#[serde(deny_unknown_fields)]
pub struct SurfaceResourceState {
    revision: u64,
    next_resource_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    resources: BTreeMap<SurfaceResourceId, SurfaceResourceRecord>,
    /// Rebuilt from persisted resource keys after decoding; never independent custody.
    #[serde(skip)]
    available_by_position: BTreeMap<VoxelCoord, BTreeSet<SurfaceResourceId>>,
}

/// Keeps ordinary diagnostics from becoming a global world-resource oracle.
impl Debug for SurfaceResourceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurfaceResourceState")
            .finish_non_exhaustive()
    }
}

/// Serializes complete surface truth only through the trusted root persistence boundary.
pub(crate) fn serialize_surface_resource_state<S>(
    state: &SurfaceResourceState,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    #[derive(Serialize)]
    struct PersistentSurfaceResourceState<'state> {
        revision: u64,
        next_resource_id: u32,
        resources: &'state BTreeMap<SurfaceResourceId, SurfaceResourceRecord>,
    }

    PersistentSurfaceResourceState {
        revision: state.revision,
        next_resource_id: state.next_resource_id,
        resources: &state.resources,
    }
    .serialize(serializer)
}

impl SurfaceResourceState {
    /// Privileged in-memory snapshot for the simulation's exceptional atomic
    /// control boundary. This does not expose global surface truth through Clone.
    pub(crate) fn snapshot_for_control_transaction(&self) -> Self {
        Self {
            revision: self.revision,
            next_resource_id: self.next_resource_id,
            resources: self.resources.clone(),
            available_by_position: self.available_by_position.clone(),
        }
    }

    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_resource_id: 1,
            resources: BTreeMap::new(),
            available_by_position: BTreeMap::new(),
        }
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        self.available_by_position.clear();
        for (id, record) in &self.resources {
            if record.lifecycle == SurfaceResourceLifecycle::Available
                && !record.remaining_mass.is_zero()
            {
                self.available_by_position
                    .entry(record.position)
                    .or_default()
                    .insert(*id);
            }
        }
    }

    pub(super) fn has_valid_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<SurfaceResourceId>>::new();
        for (id, record) in &self.resources {
            if record.lifecycle == SurfaceResourceLifecycle::Available
                && !record.remaining_mass.is_zero()
            {
                expected.entry(record.position).or_default().insert(*id);
            }
        }
        expected == self.available_by_position
    }

    #[must_use]
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(crate) fn get(&self, id: SurfaceResourceId) -> Option<&SurfaceResourceRecord> {
        self.resources.get(&id)
    }

    /// Iterates currently gatherable loose resources at the supplied trusted observer voxel.
    pub(crate) fn available_resources_at(
        &self,
        position: Option<VoxelCoord>,
    ) -> impl Iterator<Item = &SurfaceResourceRecord> {
        position
            .into_iter()
            .filter_map(|position| self.available_by_position.get(&position))
            .flat_map(|ids| ids.iter())
            .map(|id| {
                self.resources.get(id).unwrap_or_else(|| {
                    unreachable!("surface location index references missing resource")
                })
            })
    }

    /// Reads live surface custody within physical bounds using the local position index.
    /// Restricting the B-tree range by X avoids a scan over remote and depleted resources;
    /// Y and Z are checked against the full half-open bounds before resolving each identity.
    pub(crate) fn available_resources_in_bounds(
        &self,
        bounds: VoxelBounds,
    ) -> impl Iterator<Item = &SurfaceResourceRecord> {
        let start = VoxelCoord::new(bounds.min().x(), i64::MIN, i64::MIN);
        let end = VoxelCoord::new(bounds.max_exclusive().x(), i64::MIN, i64::MIN);
        self.available_by_position
            .range(start..end)
            .filter(move |(position, _)| bounds.has_voxel(**position))
            .flat_map(|(_, ids)| {
                ids.iter().map(|id| {
                    self.resources.get(id).unwrap_or_else(|| {
                        unreachable!("surface location index references missing resource")
                    })
                })
            })
    }

    /// Live surface matter in a chunk column, even when its inclusive final
    /// voxel is i64::MAX and no half-open VoxelBounds can enclose that edge.
    pub(crate) fn available_resources_in_chunk(
        &self,
        chunk: crate::world::ChunkCoord,
    ) -> impl Iterator<Item = &SurfaceResourceRecord> {
        let x_span = chunk.world_min_x().map(|min| {
            (
                VoxelCoord::new(min, i64::MIN, i64::MIN),
                VoxelCoord::new(
                    min.saturating_add(crate::world::CHUNK_SIDE as i64 - 1),
                    i64::MAX,
                    i64::MAX,
                ),
            )
        });
        x_span
            .into_iter()
            .flat_map(move |(low, high)| self.available_by_position.range(low..=high))
            .filter(move |(position, _)| crate::world::ChunkCoord::containing(**position) == chunk)
            .flat_map(|(_, ids)| {
                ids.iter().map(|id| {
                    self.resources.get(id).unwrap_or_else(|| {
                        unreachable!("surface location index references missing resource")
                    })
                })
            })
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = &SurfaceResourceRecord> {
        self.resources.values()
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_resource_id != 0
            && self
                .resources
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_resource_id)
    }

    pub(crate) const fn next_resource_id(&self) -> u32 {
        self.next_resource_id
    }

    #[must_use]
    pub(crate) fn is_pristine_generation_state(&self) -> bool {
        self.revision == 0 && self.next_resource_id == 1 && self.resources.is_empty()
    }

    pub(crate) fn apply_created_batch(
        &mut self,
        records: Vec<SurfaceResourceRecord>,
        next_resource_id: u32,
        next_revision: u64,
    ) {
        assert_eq!(self.revision.checked_add(1), Some(next_revision));
        assert!(!records.is_empty());
        let mut expected_id = self.next_resource_id;
        for record in records {
            assert_eq!(record.id.value(), expected_id);
            assert!(record.lifecycle == SurfaceResourceLifecycle::Available);
            assert!(!record.remaining_mass.is_zero());
            assert!(
                self.available_by_position
                    .entry(record.position)
                    .or_default()
                    .insert(record.id)
            );
            assert!(self.resources.insert(record.id, record).is_none());
            expected_id = expected_id.checked_add(1).unwrap_or_else(|| {
                unreachable!("validated surface generation reserved the next id")
            });
        }
        assert_eq!(expected_id, next_resource_id);
        self.next_resource_id = next_resource_id;
        self.revision = next_revision;
    }

    /// Moves one entire available loose player drop by a single face-adjacent step.
    ///
    /// Scavenging changes only the location, never the parcel identity, age, provenance,
    /// composition or mass. Player pickup can still consume it at the new location.
    pub(crate) fn apply_scavenged_drop_step(
        &mut self,
        resource: SurfaceResourceId,
        from: VoxelCoord,
        to: VoxelCoord,
        next_revision: u64,
    ) {
        assert!(from.is_face_adjacent_to(to));
        assert_eq!(self.revision.checked_add(1), Some(next_revision));
        let record = self.resources.get_mut(&resource).unwrap_or_else(|| {
            unreachable!("selected scavenged drop exists at this surface revision")
        });
        assert_eq!(record.kind, SurfaceResourceKind::Drop);
        assert_eq!(record.position, from);
        assert_eq!(record.lifecycle, SurfaceResourceLifecycle::Available);
        assert!(!record.remaining_mass.is_zero());
        record.position = to;
        self.remove_available_index(from, resource);
        assert!(
            self.available_by_position
                .entry(to)
                .or_default()
                .insert(resource)
        );
        self.revision = next_revision;
    }

    pub(crate) fn apply_gathering(
        &mut self,
        resource: SurfaceResourceId,
        gathered: Mass,
        gathered_at: SimulationTick,
        next_revision: u64,
    ) {
        assert_eq!(self.revision.checked_add(1), Some(next_revision));
        let record = self.resources.get_mut(&resource).unwrap_or_else(|| {
            panic!("validated surface resource disappeared without revision change")
        });
        assert_eq!(record.lifecycle, SurfaceResourceLifecycle::Available);
        assert!(record.depleted_at.is_none());
        assert!(gathered_at >= record.created_at);
        assert!(!gathered.is_zero());
        let remaining = record
            .remaining_mass
            .checked_sub(gathered)
            .unwrap_or_else(|| panic!("surface gathering exceeded remaining resource mass"));
        record.remaining_mass = remaining;
        if remaining.is_zero() {
            record.lifecycle = SurfaceResourceLifecycle::Depleted;
            record.depleted_at = Some(gathered_at);
            let position = record.position;
            self.remove_available_index(position, resource);
        }
        self.revision = next_revision;
    }

    fn remove_available_index(&mut self, position: VoxelCoord, resource: SurfaceResourceId) {
        let ids = self
            .available_by_position
            .get_mut(&position)
            .unwrap_or_else(|| {
                unreachable!("available surface parcel must appear in its position index")
            });
        assert!(ids.remove(&resource));
        if ids.is_empty() {
            self.available_by_position.remove(&position);
        }
    }
}

pub(super) mod validation;

pub use validation::SurfaceResourceValidationError;
pub(crate) use validation::validate_loaded_surface_resources;
