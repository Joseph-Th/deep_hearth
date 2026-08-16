//! Immutable definition aggregate loaded once and passed explicitly to simulation systems.

use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

use crate::capability::CapabilityRegistry;
use crate::core::quantity::Acceleration;
use crate::energy::EnergyRegistry;
use crate::equipment::EquipmentRegistry;
use crate::fluid::FluidRegistry;
use crate::material::MaterialRegistry;
use crate::ore_processing::OreProcessingRegistry;
use crate::production::ProductionRegistry;
use crate::shader::ShaderRegistry;
use crate::structural::StructuralRegistry;
use crate::texture::TextureRegistry;
use crate::thermal::ThermalRegistry;

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
    gravity: Acceleration,
}

impl CoreDefinitions {
    pub(crate) fn new(ticks_per_second: u16, gravity: Acceleration) -> Self {
        let Some(ticks_per_second) = NonZeroU16::new(ticks_per_second) else {
            panic!("core registry ticks_per_second must be nonzero");
        };

        Self {
            ticks_per_second,
            gravity,
        }
    }

    /// Returns the authoritative base tick frequency.
    #[must_use]
    pub const fn ticks_per_second(&self) -> NonZeroU16 {
        self.ticks_per_second
    }

    /// Returns the authored world gravitational acceleration used by weight-bearing systems.
    #[must_use]
    pub const fn gravity(&self) -> Acceleration {
        self.gravity
    }
}

/// Root immutable registry set for all authored static definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registries {
    schema_version: RegistrySchemaVersion,
    core: CoreDefinitions,
    domains: RegistryDomains,
}

/// Domain registry bundle used to assemble the immutable root without a wide positional API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegistryDomains {
    pub(crate) energy: EnergyRegistry,
    pub(crate) fluid: FluidRegistry,
    pub(crate) capabilities: CapabilityRegistry,
    pub(crate) equipment: EquipmentRegistry,
    pub(crate) structural: StructuralRegistry,
    pub(crate) materials: MaterialRegistry,
    pub(crate) ore_processing: OreProcessingRegistry,
    pub(crate) thermal: ThermalRegistry,
    pub(crate) production: ProductionRegistry,
    pub(crate) presentation: RegistryPresentation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegistryPresentation {
    pub(crate) textures: TextureRegistry,
    pub(crate) shaders: ShaderRegistry,
}

impl Registries {
    pub(crate) fn new(
        schema_version: RegistrySchemaVersion,
        core: CoreDefinitions,
        domains: RegistryDomains,
    ) -> Self {
        domains.fluid.validate_references(&domains.materials);
        domains.equipment.validate_references(&domains.capabilities);
        domains
            .production
            .validate_references(&domains.materials, &domains.capabilities);
        domains.ore_processing.validate_references(
            &domains.production,
            &domains.capabilities,
            &domains.materials,
        );
        domains.thermal.validate_references(
            &domains.production,
            &domains.capabilities,
            &domains.materials,
        );
        domains
            .presentation
            .textures
            .validate_references(&domains.materials, &domains.equipment);
        for process in domains.ore_processing.process_ids() {
            assert!(
                !domains.thermal.has_process(process),
                "process {} cannot own both ore-processing and thermal resolver semantics",
                process.value()
            );
        }
        Self {
            schema_version,
            core,
            domains,
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

    /// Returns immutable finite-energy store definitions.
    #[must_use]
    pub const fn energy(&self) -> &EnergyRegistry {
        &self.domains.energy
    }

    /// Returns immutable authored fluid identities.
    #[must_use]
    pub const fn fluid(&self) -> &FluidRegistry {
        &self.domains.fluid
    }

    /// Returns immutable authored physical/tool/equipment capability definitions.
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityRegistry {
        &self.domains.capabilities
    }

    /// Returns immutable maintainable equipment definitions.
    #[must_use]
    pub const fn equipment(&self) -> &EquipmentRegistry {
        &self.domains.equipment
    }

    /// Returns immutable structural load-response profiles.
    #[must_use]
    pub const fn structural(&self) -> &StructuralRegistry {
        &self.domains.structural
    }

    /// Returns immutable material and physical-form definitions.
    #[must_use]
    pub const fn materials(&self) -> &MaterialRegistry {
        &self.domains.materials
    }

    /// Returns immutable physical ore/material-preparation resolver definitions.
    #[must_use]
    pub const fn ore_processing(&self) -> &OreProcessingRegistry {
        &self.domains.ore_processing
    }

    /// Returns immutable physical thermal-process resolution semantics.
    #[must_use]
    pub const fn thermal(&self) -> &ThermalRegistry {
        &self.domains.thermal
    }

    /// Returns immutable material-transformation definitions.
    #[must_use]
    pub const fn production(&self) -> &ProductionRegistry {
        &self.domains.production
    }

    /// Returns immutable palette, texture, and block/object appearance definitions.
    #[must_use]
    pub const fn textures(&self) -> &TextureRegistry {
        &self.domains.presentation.textures
    }

    /// Returns immutable WGSL libraries and executable shader definitions.
    #[must_use]
    pub const fn shaders(&self) -> &ShaderRegistry {
        &self.domains.presentation.shaders
    }
}
