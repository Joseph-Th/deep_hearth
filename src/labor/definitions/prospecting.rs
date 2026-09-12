//! Authored bounded geological observation methods.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Pressure;
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::geology::GeologicalEvidenceKind;
use crate::maintenance::assert_valid_condition_wear_ppm_per_tick;
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

/// Physical instrument requirement and wear for one prospecting method.
///
/// The accepted definitions are explicit authored tool identities rather than a generic tier score.
/// This keeps prospecting precision owned by the method while equipment remains responsible for
/// persistent condition, maintenance, assembly, and upgrade identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProspectingEquipmentProfile {
    primary: EquipmentDefinitionId,
    alternative: Option<EquipmentDefinitionId>,
    condition_wear_ppm_per_active_tick: u32,
}

impl ProspectingEquipmentProfile {
    #[must_use]
    pub fn new(
        primary: EquipmentDefinitionId,
        alternative: Option<EquipmentDefinitionId>,
        condition_wear_ppm_per_active_tick: u32,
    ) -> Self {
        assert!(
            alternative != Some(primary),
            "prospecting equipment alternatives must identify distinct definitions"
        );
        assert_valid_condition_wear_ppm_per_tick(condition_wear_ppm_per_active_tick);
        Self {
            primary,
            alternative,
            condition_wear_ppm_per_active_tick,
        }
    }

    #[must_use]
    pub fn accepts(self, definition: EquipmentDefinitionId) -> bool {
        definition == self.primary || self.alternative == Some(definition)
    }

    #[must_use]
    pub const fn primary(self) -> EquipmentDefinitionId {
        self.primary
    }

    #[must_use]
    pub const fn alternative(self) -> Option<EquipmentDefinitionId> {
        self.alternative
    }

    #[must_use]
    pub const fn condition_wear_ppm_per_active_tick(self) -> u32 {
        self.condition_wear_ppm_per_active_tick
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
            abundance_uncertainty_ppm <= 1_000_000,
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
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub const fn equipment(self) -> Option<ProspectingEquipmentProfile> {
        self.equipment
    }
}
