//! Defines persistent structural member identity, geometry, lifecycle, loads, and embodiment.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{Area, Force, Length, Mass};
use crate::core::time::SimulationTick;
use crate::inventory::{ConsumedMaterialTrace, checked_consumed_material_mass};
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::definitions::StructuralProfileId;
use super::super::geometry::StructuralGeometryError;

/// Persistent identifier for one structural member record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructuralElementId(pub(super) u32);

impl StructuralElementId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "structural element id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Structural lifecycle separates construction configuration from load-bearing participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLifecycle {
    Planned,
    Active,
    Failed,
}

/// Physical origin of an externally resolved load contribution.
///
/// Each owning system updates only its own contribution so unrelated causes cannot overwrite one
/// another. Structural analysis consumes the sum and does not invent these source values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLoadKind {
    SelfWeight,
    Permanent,
    StoredMatter,
    Equipment,
    Fluid,
    Snow,
    Wind,
    Occupancy,
}

/// Immutable physical geometry of one structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StructuralElementGeometry {
    pub(in crate::structural) bounds: VoxelBounds,
    pub(in crate::structural) length: Length,
    pub(in crate::structural) cross_section: Area,
}

impl StructuralElementGeometry {
    /// Builds validated prismatic member geometry before it can enter authoritative state.
    pub fn new(
        bounds: VoxelBounds,
        length: Length,
        cross_section: Area,
    ) -> Result<Self, StructuralGeometryError> {
        let geometry = Self {
            bounds,
            length,
            cross_section,
        };
        geometry.validate()?;
        Ok(geometry)
    }

    /// Rechecks geometry after a serialization or internal trust boundary.
    pub fn validate(self) -> Result<(), StructuralGeometryError> {
        if self.cross_section.is_zero() {
            return Err(StructuralGeometryError::ZeroCrossSection);
        }
        if self.length.is_zero() {
            return Err(StructuralGeometryError::ZeroLength);
        }
        Ok(())
    }

    #[must_use]
    pub const fn bounds(self) -> VoxelBounds {
        self.bounds
    }

    #[must_use]
    pub const fn length(self) -> Length {
        self.length
    }

    #[must_use]
    pub const fn cross_section(self) -> Area {
        self.cross_section
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuralElementGeometryRepresentation {
    bounds: VoxelBounds,
    length: Length,
    cross_section: Area,
}

impl<'de> Deserialize<'de> for StructuralElementGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = StructuralElementGeometryRepresentation::deserialize(deserializer)?;
        Self::new(
            representation.bounds,
            representation.length,
            representation.cross_section,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Immutable authored/runtime specification of one structural member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::structural) struct StructuralElementConfiguration {
    pub(in crate::structural) profile: StructuralProfileId,
    pub(in crate::structural) material: MaterialId,
    pub(in crate::structural) geometry: StructuralElementGeometry,
    pub(in crate::structural) is_grounded: bool,
}

/// Persistent physical and lifecycle state for one structural member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralElementRecord {
    pub(in crate::structural) id: StructuralElementId,
    pub(in crate::structural) configuration: StructuralElementConfiguration,
    pub(in crate::structural) embodied_material: Vec<ConsumedMaterialTrace>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    pub(in crate::structural) loads: BTreeMap<StructuralLoadKind, Force>,
    pub(in crate::structural) lifecycle: StructuralLifecycle,
    pub(in crate::structural) is_cracked: bool,
    pub(in crate::structural) created_at: SimulationTick,
}

impl StructuralElementRecord {
    #[must_use]
    pub const fn id(&self) -> StructuralElementId {
        self.id
    }

    #[must_use]
    pub const fn profile(&self) -> StructuralProfileId {
        self.configuration.profile
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.configuration.material
    }

    #[must_use]
    pub const fn bounds(&self) -> VoxelBounds {
        self.configuration.geometry.bounds
    }

    #[must_use]
    pub const fn cross_section(&self) -> Area {
        self.configuration.geometry.cross_section
    }

    #[must_use]
    pub const fn length(&self) -> Length {
        self.configuration.geometry.length
    }

    #[must_use]
    pub const fn geometry(&self) -> StructuralElementGeometry {
        self.configuration.geometry
    }

    #[must_use]
    pub const fn is_grounded(&self) -> bool {
        self.configuration.is_grounded
    }

    /// Exact matter currently owned by this structural member.
    #[must_use]
    pub fn embodied_mass(&self) -> Mass {
        checked_consumed_material_mass(&self.embodied_material).unwrap_or_else(|| {
            panic!(
                "validated structural element {} embodied trace mass overflowed",
                self.id.value()
            )
        })
    }

    /// Physical/provenance traces transferred into this member at construction.
    #[must_use]
    pub fn embodied_material(&self) -> &[ConsumedMaterialTrace] {
        &self.embodied_material
    }

    #[must_use]
    pub fn load(&self, kind: StructuralLoadKind) -> Force {
        self.loads.get(&kind).copied().unwrap_or(Force::ZERO)
    }

    pub fn loads(&self) -> impl Iterator<Item = (StructuralLoadKind, Force)> + '_ {
        self.loads.iter().map(|(kind, force)| (*kind, *force))
    }

    #[must_use]
    pub const fn lifecycle(&self) -> StructuralLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn is_cracked(&self) -> bool {
        self.is_cracked
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}
