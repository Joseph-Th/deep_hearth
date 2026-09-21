//! Persisted geological evidence identity, provenance, and observation records.

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

mod abundance;
mod hardness;
mod resource_mass;

pub use abundance::{AbundanceBound, MaterialAbundanceEstimate, MaterialAbundanceEstimateError};
pub(in crate::geology) use abundance::{PARTS_PER_MILLION, total_lower_bound_ppm};
pub(in crate::geology) use hardness::{
    ExcavationHardnessContextError, validate_excavation_hardness_context,
};
pub use hardness::{ExcavationHardnessEstimate, ExcavationHardnessEstimateError};
pub(in crate::geology) use resource_mass::{
    ResourceMassContextError, validate_resource_mass_context,
};
pub use resource_mass::{ResourceMassEstimate, ResourceMassEstimateError};

/// Persistent identity of one acquired geological observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeologicalObservationId(u32);

impl GeologicalObservationId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "geological observation id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Physical or observational provenance for geological evidence.
///
/// These are evidence sources, not technology levels. Information quality is represented by the
/// quantitative spatial footprint and abundance bounds recorded by the resolving instrument or
/// sampling system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GeologicalEvidenceKind {
    SurfaceExposure,
    LooseIndicator,
    PannedConcentrate,
    ExcavationSample,
    CoreSample,
    LaboratoryAssay,
    MagneticSurvey,
    ElectricalSurvey,
    SeismicSurvey,
}

impl GeologicalEvidenceKind {
    /// Whether this evidence provenance can directly measure host-rock excavation resistance.
    pub(crate) const fn supports_excavation_hardness(self) -> bool {
        matches!(self, Self::ExcavationSample | Self::CoreSample)
    }

    pub(crate) const fn supports_resource_mass(self) -> bool {
        matches!(self, Self::ExcavationSample | Self::CoreSample)
    }
}

/// Persisted geological observation acquired at one simulation tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalObservationRecord {
    pub(in crate::geology) id: GeologicalObservationId,
    pub(in crate::geology) region: VoxelBounds,
    pub(in crate::geology) evidence: GeologicalEvidenceKind,
    pub(in crate::geology) findings: Vec<MaterialAbundanceEstimate>,
    pub(in crate::geology) excavation_hardness: Option<ExcavationHardnessEstimate>,
    pub(in crate::geology) resource_mass: Option<ResourceMassEstimate>,
    pub(in crate::geology) observed_at: SimulationTick,
}

impl GeologicalObservationRecord {
    #[must_use]
    pub const fn id(&self) -> GeologicalObservationId {
        self.id
    }

    #[must_use]
    pub const fn region(&self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn evidence(&self) -> GeologicalEvidenceKind {
        self.evidence
    }

    #[must_use]
    pub fn findings(&self) -> &[MaterialAbundanceEstimate] {
        &self.findings
    }

    /// Returns the acquired excavation-resistance band when this observation physically measured it.
    #[must_use]
    pub const fn excavation_hardness(&self) -> Option<ExcavationHardnessEstimate> {
        self.excavation_hardness
    }

    /// Returns an acquired conservative estimate of the represented extractable body mass.
    ///
    /// This is historical player knowledge, not live geological truth or mining authorization.
    #[must_use]
    pub const fn resource_mass(&self) -> Option<ResourceMassEstimate> {
        self.resource_mass
    }

    #[must_use]
    pub const fn observed_at(&self) -> SimulationTick {
        self.observed_at
    }

    #[must_use]
    pub fn finding(&self, material: MaterialId) -> Option<MaterialAbundanceEstimate> {
        self.findings
            .binary_search_by_key(&material, |finding| finding.material())
            .ok()
            .map(|index| self.findings[index])
    }
}
