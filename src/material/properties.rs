//! Immutable thermophysical and structural material properties.

use crate::core::quantity::Temperature;

/// Authored solid/liquid fusion boundary and latent-energy requirement for one material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FusionProperties {
    melting_point: Temperature,
    latent_heat_j_per_kg: u32,
}

impl FusionProperties {
    #[must_use]
    pub const fn new(melting_point: Temperature, latent_heat_j_per_kg: u32) -> Self {
        assert!(
            melting_point.millikelvin() != 0,
            "material melting point must be above absolute zero"
        );
        assert!(
            latent_heat_j_per_kg > 0,
            "material latent heat of fusion must be nonzero"
        );
        Self {
            melting_point,
            latent_heat_j_per_kg,
        }
    }

    #[must_use]
    pub const fn melting_point(self) -> Temperature {
        self.melting_point
    }

    #[must_use]
    pub const fn latent_heat_j_per_kg(self) -> u32 {
        self.latent_heat_j_per_kg
    }
}

/// Thermal properties used by heat transfer and phase-change systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThermalProperties {
    specific_heat_j_per_kg_k: u32,
    fusion: Option<FusionProperties>,
}

impl ThermalProperties {
    #[must_use]
    pub const fn new(specific_heat_j_per_kg_k: u32, fusion: Option<FusionProperties>) -> Self {
        assert!(
            specific_heat_j_per_kg_k > 0,
            "material specific heat must be nonzero"
        );
        Self {
            specific_heat_j_per_kg_k,
            fusion,
        }
    }

    #[must_use]
    pub const fn specific_heat_j_per_kg_k(&self) -> u32 {
        self.specific_heat_j_per_kg_k
    }

    #[must_use]
    pub const fn melting_point(&self) -> Option<Temperature> {
        match self.fusion {
            Some(fusion) => Some(fusion.melting_point()),
            None => None,
        }
    }

    #[must_use]
    pub const fn fusion(&self) -> Option<FusionProperties> {
        self.fusion
    }
}

/// Axial material strengths used by the currently authored structural model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralProperties {
    compressive_strength_kpa: u32,
    tensile_strength_kpa: u32,
}

impl StructuralProperties {
    #[must_use]
    pub const fn new(compressive_strength_kpa: u32, tensile_strength_kpa: u32) -> Self {
        Self {
            compressive_strength_kpa,
            tensile_strength_kpa,
        }
    }

    #[must_use]
    pub const fn compressive_strength_kpa(&self) -> u32 {
        self.compressive_strength_kpa
    }

    #[must_use]
    pub const fn tensile_strength_kpa(&self) -> u32 {
        self.tensile_strength_kpa
    }
}

/// Authoritative material properties represented in integer engineering units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialProperties {
    density_kg_per_m3: u32,
    thermal: ThermalProperties,
    structural: Option<StructuralProperties>,
}

impl MaterialProperties {
    /// Builds a complete immutable material property profile from coherent subprofiles.
    #[must_use]
    pub const fn new(
        density_kg_per_m3: u32,
        thermal: ThermalProperties,
        structural: Option<StructuralProperties>,
    ) -> Self {
        assert!(density_kg_per_m3 > 0, "material density must be nonzero");
        Self {
            density_kg_per_m3,
            thermal,
            structural,
        }
    }

    #[must_use]
    pub const fn density_kg_per_m3(&self) -> u32 {
        self.density_kg_per_m3
    }

    #[must_use]
    pub const fn thermal(&self) -> &ThermalProperties {
        &self.thermal
    }

    #[must_use]
    pub const fn structural(&self) -> Option<&StructuralProperties> {
        self.structural.as_ref()
    }
}
