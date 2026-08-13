//! Immutable definition aggregate loaded once and passed explicitly to simulation systems.

use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRegistry;
use crate::material::MaterialRegistry;
use crate::production::ProductionRegistry;

/// Compatibility version for stable authored registry identities and cross-reference semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegistrySchemaVersion(u32);

impl RegistrySchemaVersion {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "registry schema version must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Domain-neutral immutable definitions needed by the simulation core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreDefinitions {
    ticks_per_second: NonZeroU16,
}

impl CoreDefinitions {
    pub(crate) fn new(ticks_per_second: u16) -> Self {
        let Some(ticks_per_second) = NonZeroU16::new(ticks_per_second) else {
            panic!("core registry ticks_per_second must be nonzero");
        };

        Self { ticks_per_second }
    }

    /// Returns the authoritative base tick frequency.
    #[must_use]
    pub const fn ticks_per_second(&self) -> NonZeroU16 {
        self.ticks_per_second
    }
}

/// Root immutable registry set for all authored static definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registries {
    schema_version: RegistrySchemaVersion,
    core: CoreDefinitions,
    capabilities: CapabilityRegistry,
    materials: MaterialRegistry,
    production: ProductionRegistry,
}

impl Registries {
    pub(crate) fn new(
        schema_version: RegistrySchemaVersion,
        core: CoreDefinitions,
        capabilities: CapabilityRegistry,
        materials: MaterialRegistry,
        production: ProductionRegistry,
    ) -> Self {
        production.validate_references(&materials, &capabilities);
        Self {
            schema_version,
            core,
            capabilities,
            materials,
            production,
        }
    }

    /// Returns the authored-ID compatibility version required by persisted runtime references.
    #[must_use]
    pub const fn schema_version(&self) -> RegistrySchemaVersion {
        self.schema_version
    }

    /// Returns immutable core definitions.
    #[must_use]
    pub const fn core(&self) -> &CoreDefinitions {
        &self.core
    }

    /// Returns immutable authored physical/tool/equipment capability definitions.
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityRegistry {
        &self.capabilities
    }

    /// Returns immutable material and physical-form definitions.
    #[must_use]
    pub const fn materials(&self) -> &MaterialRegistry {
        &self.materials
    }

    /// Returns immutable material-transformation definitions.
    #[must_use]
    pub const fn production(&self) -> &ProductionRegistry {
        &self.production
    }
}
