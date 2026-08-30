//! Authored material and physical-form definitions.

use serde::{Deserialize, Serialize};

use super::identity::{FormId, MaterialId};
use super::properties::MaterialProperties;

/// Immutable authored material definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialDefinition {
    id: MaterialId,
    name: String,
    properties: MaterialProperties,
}

impl MaterialDefinition {
    /// Builds an immutable material definition for registry insertion.
    #[must_use]
    pub fn new(id: MaterialId, name: impl Into<String>, properties: MaterialProperties) -> Self {
        assert!(id.value() != 0, "material definition id must be nonzero");
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "material definition name must not be empty"
        );
        Self {
            id,
            name,
            properties,
        }
    }

    #[must_use]
    pub const fn id(&self) -> MaterialId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn properties(&self) -> &MaterialProperties {
        &self.properties
    }
}

/// Phase carried by an authored physical material form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaterialPhase {
    Solid,
    Liquid,
}

/// Authored contract for whether lots of one physical form carry particulate size state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParticleSizeStatePolicy {
    Untracked,
    Required,
}

/// Authored physical cohesion of one material form.
///
/// A consolidated form can directly participate in rigid assemblies. Loose forms require an
/// explicit shaping, compaction, casting, or other consolidation process before they can become a
/// load-bearing or otherwise rigid component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaterialFormCohesion {
    Consolidated,
    Loose,
}

/// Immutable authored physical-form definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDefinition {
    id: FormId,
    name: String,
    phase: MaterialPhase,
    particle_size_policy: ParticleSizeStatePolicy,
    cohesion: MaterialFormCohesion,
}

impl FormDefinition {
    /// Builds an immutable material-form definition for registry insertion.
    #[must_use]
    pub fn new(
        id: FormId,
        name: impl Into<String>,
        phase: MaterialPhase,
        particle_size_policy: ParticleSizeStatePolicy,
        cohesion: MaterialFormCohesion,
    ) -> Self {
        assert!(id.value() != 0, "material form id must be nonzero");
        assert!(
            phase == MaterialPhase::Solid
                || particle_size_policy == ParticleSizeStatePolicy::Untracked,
            "liquid forms cannot require discrete particle-size state"
        );
        assert!(
            cohesion != MaterialFormCohesion::Consolidated
                || (phase == MaterialPhase::Solid
                    && particle_size_policy == ParticleSizeStatePolicy::Untracked),
            "consolidated forms must be non-particulate solids"
        );
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "material form name must not be empty"
        );
        Self {
            id,
            name,
            phase,
            particle_size_policy,
            cohesion,
        }
    }

    #[must_use]
    pub const fn id(&self) -> FormId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn phase(&self) -> MaterialPhase {
        self.phase
    }

    #[must_use]
    pub const fn particle_size_policy(&self) -> ParticleSizeStatePolicy {
        self.particle_size_policy
    }

    #[must_use]
    pub const fn cohesion(&self) -> MaterialFormCohesion {
        self.cohesion
    }

    #[must_use]
    pub const fn is_consolidated(&self) -> bool {
        matches!(self.cohesion, MaterialFormCohesion::Consolidated)
    }
}
