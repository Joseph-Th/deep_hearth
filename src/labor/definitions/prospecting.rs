//! Authored bounded geological observation methods.

use serde::{Deserialize, Serialize};

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::quantity::{Mass, Pressure};
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::geology::GeologicalEvidenceKind;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
use crate::spatial::VoxelBounds;
use crate::survival::SurvivalExertion;

/// Stable authored identity for one direct geological observation method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProspectingMethodId(u32);

impl ProspectingMethodId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "prospecting method id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Spatial evidence granularity produced when one prospecting action completes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProspectingSpatialResolution {
    /// One observation summarizes the complete actor-selected region.
    #[default]
    AggregateRegion,
    /// One observation is recorded for each explicitly covered voxel.
    PerVoxel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProspectingRegionError {
    VolumeOverflow,
    TooLarge { actual: u128, maximum: u128 },
}

/// Physical instrument requirements and per-instrument wear for one prospecting method.
///
/// The accepted definitions are explicit authored tool identities rather than a generic tier score.
/// This keeps prospecting precision owned by the method while equipment remains responsible for
/// persistent condition, maintenance, assembly, and upgrade identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProspectingEquipmentProfile {
    primary: EquipmentDefinitionId,
    primary_condition_wear_ppm_per_active_tick: u32,
    alternative: Option<EquipmentDefinitionId>,
    alternative_condition_wear_ppm_per_active_tick: Option<u32>,
}

impl ProspectingEquipmentProfile {
    #[must_use]
    pub fn new(primary: EquipmentDefinitionId, condition_wear_ppm_per_active_tick: u32) -> Self {
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            primary,
            primary_condition_wear_ppm_per_active_tick: condition_wear_ppm_per_active_tick,
            alternative: None,
            alternative_condition_wear_ppm_per_active_tick: None,
        }
    }

    #[must_use]
    pub fn with_alternative(
        mut self,
        alternative: EquipmentDefinitionId,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert_ne!(
            alternative, self.primary,
            "prospecting equipment alternatives must identify distinct definitions"
        );
        assert!(
            self.alternative.is_none(),
            "prospecting equipment profile cannot define more than one alternative"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        self.alternative = Some(alternative);
        self.alternative_condition_wear_ppm_per_active_tick =
            Some(condition_wear_ppm_per_active_tick);
        self
    }

    #[must_use]
    pub fn condition_wear_ppm_per_active_tick(
        self,
        definition: EquipmentDefinitionId,
    ) -> Option<u32> {
        if definition == self.primary {
            return Some(self.primary_condition_wear_ppm_per_active_tick);
        }
        if self.alternative == Some(definition) {
            return self.alternative_condition_wear_ppm_per_active_tick;
        }
        None
    }

    #[must_use]
    pub fn accepts(self, definition: EquipmentDefinitionId) -> bool {
        self.condition_wear_ppm_per_active_tick(definition)
            .is_some()
    }

    #[must_use]
    pub const fn primary(self) -> EquipmentDefinitionId {
        self.primary
    }

    #[must_use]
    pub const fn alternative(self) -> Option<EquipmentDefinitionId> {
        self.alternative
    }
}

/// Authored rule for one bounded player-performed geological observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProspectingDefinition {
    id: ProspectingMethodId,
    evidence: GeologicalEvidenceKind,
    spatial_resolution: ProspectingSpatialResolution,
    duration: TickSpan,
    maximum_region_voxels: u128,
    abundance_uncertainty_ppm: u32,
    excavation_hardness_resolution: Option<Pressure>,
    resource_mass_resolution: Option<Mass>,
    exertion: SurvivalExertion,
    equipment: Option<ProspectingEquipmentProfile>,
}

impl ProspectingDefinition {
    #[must_use]
    pub fn new(
        id: ProspectingMethodId,
        evidence: GeologicalEvidenceKind,
        duration: TickSpan,
        maximum_region_voxels: u128,
        abundance_uncertainty_ppm: u32,
        exertion: SurvivalExertion,
    ) -> Self {
        assert!(
            duration.value() != 0,
            "prospecting duration must be nonzero"
        );
        assert!(
            maximum_region_voxels != 0,
            "prospecting maximum region must contain at least one voxel"
        );
        assert!(
            abundance_uncertainty_ppm <= NORMALIZED_PARTS_PER_MILLION,
            "prospecting abundance uncertainty must not exceed one million ppm"
        );
        exertion.assert_active_player_work();
        Self {
            id,
            evidence,
            spatial_resolution: ProspectingSpatialResolution::AggregateRegion,
            duration,
            maximum_region_voxels,
            abundance_uncertainty_ppm,
            excavation_hardness_resolution: None,
            resource_mass_resolution: None,
            exertion,
            equipment: None,
        }
    }

    #[must_use]
    pub fn new_with_equipment(
        id: ProspectingMethodId,
        evidence: GeologicalEvidenceKind,
        duration: TickSpan,
        maximum_region_voxels: u128,
        abundance_uncertainty_ppm: u32,
        exertion: SurvivalExertion,
        equipment: ProspectingEquipmentProfile,
    ) -> Self {
        let mut definition = Self::new(
            id,
            evidence,
            duration,
            maximum_region_voxels,
            abundance_uncertainty_ppm,
            exertion,
        );
        definition.equipment = Some(equipment);
        definition
    }

    /// Changes only the spatial granularity of acquired evidence for this authored method.
    #[must_use]
    pub fn with_spatial_resolution(
        mut self,
        spatial_resolution: ProspectingSpatialResolution,
    ) -> Self {
        if spatial_resolution == ProspectingSpatialResolution::PerVoxel {
            assert!(
                self.maximum_region_voxels <= u128::from(u32::MAX),
                "per-voxel prospecting observation count must fit the geological observation identity space"
            );
        }
        self.spatial_resolution = spatial_resolution;
        self
    }

    /// Adds a coarse extractable-body mass estimate to fully localized physical observations.
    #[must_use]
    pub fn with_resource_mass_resolution(mut self, resolution: Mass) -> Self {
        assert!(
            self.resource_mass_resolution.is_none(),
            "prospecting method {} cannot define resource-mass resolution more than once",
            self.id.value()
        );
        assert!(
            self.equipment.is_some(),
            "resource-mass prospecting requires a physical instrument"
        );
        assert!(
            self.evidence.supports_resource_mass(),
            "prospecting method {} cannot attach resource mass to {:?} evidence",
            self.id.value(),
            self.evidence
        );
        assert!(
            !resolution.is_zero(),
            "resource-mass prospecting resolution must be nonzero"
        );
        self.resource_mass_resolution = Some(resolution);
        self
    }

    /// Adds a physical excavation-resistance measurement to observations from this method.
    ///
    /// The resolution is the width of deterministic pressure buckets, not an error radius. A
    /// measured body at an exact bucket boundary remains in the lower bucket so common authored
    /// equipment limits can be selected conservatively without revealing exact hidden hardness.
    #[must_use]
    pub fn with_excavation_hardness_resolution(mut self, resolution: Pressure) -> Self {
        assert!(
            self.excavation_hardness_resolution.is_none(),
            "prospecting method {} cannot define excavation-hardness resolution more than once",
            self.id.value()
        );
        assert!(
            self.equipment.is_some(),
            "excavation-hardness prospecting requires a physical instrument"
        );
        assert!(
            self.evidence.supports_excavation_hardness(),
            "prospecting method {} cannot attach excavation hardness to {:?} evidence",
            self.id.value(),
            self.evidence
        );
        assert!(
            !resolution.is_zero(),
            "excavation-hardness prospecting resolution must be nonzero"
        );
        self.excavation_hardness_resolution = Some(resolution);
        self
    }

    #[must_use]
    pub const fn id(self) -> ProspectingMethodId {
        self.id
    }

    #[must_use]
    pub const fn evidence(self) -> GeologicalEvidenceKind {
        self.evidence
    }

    #[must_use]
    pub const fn spatial_resolution(self) -> ProspectingSpatialResolution {
        self.spatial_resolution
    }

    /// Validates one requested region against the authored footprint and returns the exact
    /// persistent observation count it produces.
    ///
    /// Admission, completion, and trusted-load replay all use this path so region legality and
    /// identity/revision budgeting cannot drift apart.
    pub(crate) fn resolve_region_observation_count(
        self,
        region: VoxelBounds,
    ) -> Result<u32, ProspectingRegionError> {
        let voxels = region
            .voxel_count()
            .ok_or(ProspectingRegionError::VolumeOverflow)?;
        if voxels > self.maximum_region_voxels {
            return Err(ProspectingRegionError::TooLarge {
                actual: voxels,
                maximum: self.maximum_region_voxels,
            });
        }
        Ok(match self.spatial_resolution {
            ProspectingSpatialResolution::AggregateRegion => 1,
            ProspectingSpatialResolution::PerVoxel => u32::try_from(voxels).unwrap_or_else(|_| {
                unreachable!("per-voxel prospecting authoring bounds observation cardinality")
            }),
        })
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn maximum_region_voxels(self) -> u128 {
        self.maximum_region_voxels
    }

    #[must_use]
    pub const fn abundance_uncertainty_ppm(self) -> u32 {
        self.abundance_uncertainty_ppm
    }

    #[must_use]
    pub const fn excavation_hardness_resolution(self) -> Option<Pressure> {
        self.excavation_hardness_resolution
    }

    #[must_use]
    pub const fn resource_mass_resolution(self) -> Option<Mass> {
        self.resource_mass_resolution
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub const fn equipment(self) -> Option<ProspectingEquipmentProfile> {
        self.equipment
    }
}
