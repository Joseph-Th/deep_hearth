//! Persistent finite loose surface-resource ownership.

use std::collections::BTreeMap;
use std::fmt::Debug;

use serde::{Deserialize, Serialize, Serializer};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec};
use crate::spatial::VoxelCoord;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResourceRecord {
    id: SurfaceResourceId,
    kind: SurfaceResourceKind,
    position: VoxelCoord,
    commodity: CommodityKey,
    initial_mass: Mass,
    remaining_mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    lifecycle: SurfaceResourceLifecycle,
    created_at: SimulationTick,
    depleted_at: Option<SimulationTick>,
}

impl SurfaceResourceRecord {
    pub(super) fn created(
        id: SurfaceResourceId,
        kind: SurfaceResourceKind,
        position: VoxelCoord,
        material: MaterialLotSpec,
        created_at: SimulationTick,
    ) -> Self {
        let mass = material.mass();
        Self {
            id,
            kind,
            position,
            commodity: material.commodity(),
            initial_mass: mass,
            remaining_mass: mass,
            temperature: material.temperature(),
            composition: material.composition().clone(),
            lifecycle: SurfaceResourceLifecycle::Available,
            created_at,
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
        self.commodity
    }
    #[must_use]
    pub const fn initial_mass(&self) -> Mass {
        self.initial_mass
    }
    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }
    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }
    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
    #[must_use]
    pub const fn lifecycle(&self) -> SurfaceResourceLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub(crate) const fn created_at(&self) -> SimulationTick {
        self.created_at
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
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_resource_id: 1,
            resources: BTreeMap::new(),
        }
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
        self.resources.values().filter(move |resource| {
            Some(resource.position) == position
                && resource.lifecycle == SurfaceResourceLifecycle::Available
                && !resource.remaining_mass.is_zero()
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
            assert!(self.resources.insert(record.id, record).is_none());
            expected_id = expected_id.checked_add(1).unwrap_or_else(|| {
                unreachable!("validated surface generation reserved the next id")
            });
        }
        assert_eq!(expected_id, next_resource_id);
        self.next_resource_id = next_resource_id;
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
        }
        self.revision = next_revision;
    }
}

pub(super) mod validation;

pub use validation::SurfaceResourceValidationError;
pub(crate) use validation::validate_loaded_surface_resources;
