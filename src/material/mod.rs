//! Material identities, authored definitions, phase policy, and registry facade.

mod assembly;
mod composition;
mod lot;
mod particle;
mod volume;

pub use assembly::MaterialAssemblyProfile;
pub use composition::{
    CompositionComponent, CompositionConstraint, CompositionConstraintError, CompositionError,
    MaterialComposition,
};
pub use lot::{MaterialInputSpec, MaterialInputSpecError, MaterialLotSpec, MaterialLotSpecError};
pub use particle::{
    ParticleSizeClass, ParticleSizeClassError, ParticleSizeDistribution,
    ParticleSizeDistributionError, ParticleSizeRange, ParticleSizeRangeError,
};
pub use volume::{MaterialVolumeError, calculate_volume_ceiling};

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Temperature;

/// Normalization scale used by runtime material compositions.
pub const COMPOSITION_PARTS_PER_MILLION: u32 = 1_000_000;

/// Stable authored material identifier used by registry and runtime references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(u32);

impl MaterialId {
    /// Builds a material identifier from its stable authored representation.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the stable authored representation.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Stable authored identifier for a physical material form such as log, lump, or ingot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FormId(u16);

impl FormId {
    /// Builds a form identifier from its stable authored representation.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the stable authored representation.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Runtime key for fungible matter sharing one material and physical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommodityKey(u64);

impl CommodityKey {
    /// Builds a material/form key. Registry validity is checked at operation boundaries.
    #[must_use]
    pub const fn new(material: MaterialId, form: FormId) -> Self {
        Self((material.value() as u64) << 16 | form.value() as u64)
    }

    /// Returns the material reference.
    #[must_use]
    pub const fn material(self) -> MaterialId {
        MaterialId::new((self.0 >> 16) as u32)
    }

    /// Returns the physical-form reference.
    #[must_use]
    pub const fn form(self) -> FormId {
        FormId::new((self.0 & u16::MAX as u64) as u16)
    }

    /// Returns the packed stable representation used for ordered storage and serialization.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

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
    conductivity_milli_w_per_m_k: u32,
}

impl ThermalProperties {
    #[must_use]
    pub const fn new(
        specific_heat_j_per_kg_k: u32,
        fusion: Option<FusionProperties>,
        conductivity_milli_w_per_m_k: u32,
    ) -> Self {
        assert!(
            specific_heat_j_per_kg_k > 0,
            "material specific heat must be nonzero"
        );
        Self {
            specific_heat_j_per_kg_k,
            fusion,
            conductivity_milli_w_per_m_k,
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

    #[must_use]
    pub const fn conductivity_milli_w_per_m_k(&self) -> u32 {
        self.conductivity_milli_w_per_m_k
    }
}

/// Mechanical properties used by structural, wear, and tooling systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MechanicalProperties {
    compressive_strength_kpa: u32,
    tensile_strength_kpa: u32,
    hardness_mpa: u32,
}

impl MechanicalProperties {
    #[must_use]
    pub const fn new(
        compressive_strength_kpa: u32,
        tensile_strength_kpa: u32,
        hardness_mpa: u32,
    ) -> Self {
        Self {
            compressive_strength_kpa,
            tensile_strength_kpa,
            hardness_mpa,
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

    #[must_use]
    pub const fn hardness_mpa(&self) -> u32 {
        self.hardness_mpa
    }
}

/// Electrical properties used by future circuit and resistive-heating systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElectricalProperties {
    resistivity_nano_ohm_m: Option<u64>,
}

impl ElectricalProperties {
    #[must_use]
    pub const fn new(resistivity_nano_ohm_m: Option<u64>) -> Self {
        Self {
            resistivity_nano_ohm_m,
        }
    }

    #[must_use]
    pub const fn resistivity_nano_ohm_m(&self) -> Option<u64> {
        self.resistivity_nano_ohm_m
    }
}

/// Authoritative material properties represented in integer engineering units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialProperties {
    density_kg_per_m3: u32,
    thermal: ThermalProperties,
    mechanical: MechanicalProperties,
    electrical: ElectricalProperties,
}

impl MaterialProperties {
    /// Builds a complete immutable material property profile from coherent subprofiles.
    #[must_use]
    pub const fn new(
        density_kg_per_m3: u32,
        thermal: ThermalProperties,
        mechanical: MechanicalProperties,
        electrical: ElectricalProperties,
    ) -> Self {
        assert!(density_kg_per_m3 > 0, "material density must be nonzero");
        Self {
            density_kg_per_m3,
            thermal,
            mechanical,
            electrical,
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
    pub const fn mechanical(&self) -> &MechanicalProperties {
        &self.mechanical
    }

    #[must_use]
    pub const fn electrical(&self) -> &ElectricalProperties {
        &self.electrical
    }
}

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

/// Failure because a lot's particle-size state disagrees with its authored physical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleSizeStateError {
    UnknownForm { form: FormId },
    MissingRequired { form: FormId },
    UnexpectedForUntrackedForm { form: FormId },
}

impl Display for ParticleSizeStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownForm { form } => {
                write!(
                    formatter,
                    "particle-size state references unknown form {}",
                    form.value()
                )
            }
            Self::MissingRequired { form } => write!(
                formatter,
                "material form {} requires particle-size state",
                form.value()
            ),
            Self::UnexpectedForUntrackedForm { form } => write!(
                formatter,
                "material form {} does not track particle-size state",
                form.value()
            ),
        }
    }
}

impl Error for ParticleSizeStateError {}

/// Validates the runtime particulate state carried by one material/form key.
pub fn validate_material_particle_size_state(
    materials: &MaterialRegistry,
    commodity: CommodityKey,
    particle_size: Option<&ParticleSizeDistribution>,
) -> Result<(), ParticleSizeStateError> {
    let form_id = commodity.form();
    let Some(form) = materials.get_form(form_id) else {
        return Err(ParticleSizeStateError::UnknownForm { form: form_id });
    };
    match (form.particle_size_policy(), particle_size) {
        (ParticleSizeStatePolicy::Required, None) => {
            Err(ParticleSizeStateError::MissingRequired { form: form_id })
        }
        (ParticleSizeStatePolicy::Untracked, Some(_)) => {
            Err(ParticleSizeStateError::UnexpectedForUntrackedForm { form: form_id })
        }
        (ParticleSizeStatePolicy::Required, Some(_))
        | (ParticleSizeStatePolicy::Untracked, None) => Ok(()),
    }
}

/// Failure because a material form, composition, and temperature do not describe a supported phase state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialPhaseStateError {
    UnknownForm {
        form: FormId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    SolidAboveMeltingPoint {
        material: MaterialId,
        temperature: Temperature,
        melting_point: Temperature,
    },
    LiquidRequiresPureComposition,
    LiquidHostMismatch {
        host: MaterialId,
        pure: MaterialId,
    },
    LiquidMaterialHasNoFusionProperties {
        material: MaterialId,
    },
    LiquidBelowMeltingPoint {
        material: MaterialId,
        temperature: Temperature,
        melting_point: Temperature,
    },
}

impl Display for MaterialPhaseStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownForm { form } => {
                write!(
                    formatter,
                    "material phase state references unknown form {}",
                    form.value()
                )
            }
            Self::UnknownMaterial { material } => write!(
                formatter,
                "material phase state references unknown material {}",
                material.value()
            ),
            Self::SolidAboveMeltingPoint {
                material,
                temperature,
                melting_point,
            } => write!(
                formatter,
                "solid material {} at {} mK exceeds its {} mK melting point",
                material.value(),
                temperature.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::LiquidRequiresPureComposition => formatter.write_str(
                "liquid material requires a pure composition until mixture phase diagrams exist",
            ),
            Self::LiquidHostMismatch { host, pure } => write!(
                formatter,
                "liquid commodity host material {} disagrees with pure composition material {}",
                host.value(),
                pure.value()
            ),
            Self::LiquidMaterialHasNoFusionProperties { material } => write!(
                formatter,
                "liquid material {} has no authored fusion properties",
                material.value()
            ),
            Self::LiquidBelowMeltingPoint {
                material,
                temperature,
                melting_point,
            } => write!(
                formatter,
                "liquid material {} at {} mK is below its {} mK melting point",
                material.value(),
                temperature.millikelvin(),
                melting_point.millikelvin()
            ),
        }
    }
}

impl Error for MaterialPhaseStateError {}

/// Validates that a material lot's authored form, composition, and temperature are physically
/// consistent with the currently represented solid/liquid phase model.
///
/// Solid mixtures remain supported because each constituent can be checked independently against
/// its authored melting point. Liquid mixtures are deliberately rejected until alloy/solution phase
/// diagrams exist, because a generic weighted melting point would create false physics.
pub fn validate_material_phase_state(
    materials: &MaterialRegistry,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<(), MaterialPhaseStateError> {
    let form_id = commodity.form();
    let Some(form) = materials.get_form(form_id) else {
        return Err(MaterialPhaseStateError::UnknownForm { form: form_id });
    };
    match form.phase() {
        MaterialPhase::Solid => {
            for component in composition.components() {
                let material = component.material();
                let Some(definition) = materials.get_material(material) else {
                    return Err(MaterialPhaseStateError::UnknownMaterial { material });
                };
                if let Some(melting_point) = definition.properties().thermal().melting_point()
                    && temperature > melting_point
                {
                    return Err(MaterialPhaseStateError::SolidAboveMeltingPoint {
                        material,
                        temperature,
                        melting_point,
                    });
                }
            }
            Ok(())
        }
        MaterialPhase::Liquid => {
            let Some(material) = composition.pure_material() else {
                return Err(MaterialPhaseStateError::LiquidRequiresPureComposition);
            };
            if commodity.material() != material {
                return Err(MaterialPhaseStateError::LiquidHostMismatch {
                    host: commodity.material(),
                    pure: material,
                });
            }
            let Some(definition) = materials.get_material(material) else {
                return Err(MaterialPhaseStateError::UnknownMaterial { material });
            };
            let Some(fusion) = definition.properties().thermal().fusion() else {
                return Err(
                    MaterialPhaseStateError::LiquidMaterialHasNoFusionProperties { material },
                );
            };
            if temperature < fusion.melting_point() {
                return Err(MaterialPhaseStateError::LiquidBelowMeltingPoint {
                    material,
                    temperature,
                    melting_point: fusion.melting_point(),
                });
            }
            Ok(())
        }
    }
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

/// Immutable deterministic lookup tables for materials and their physical forms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialRegistry {
    materials: BTreeMap<MaterialId, MaterialDefinition>,
    forms: BTreeMap<FormId, FormDefinition>,
    commodities: BTreeSet<CommodityKey>,
}

impl MaterialRegistry {
    /// Builds an empty registry for code-owned startup assembly.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            materials: BTreeMap::new(),
            forms: BTreeMap::new(),
            commodities: BTreeSet::new(),
        }
    }

    /// Registers one authored material, panicking immediately on an ID collision.
    pub(crate) fn register_material(&mut self, definition: MaterialDefinition) {
        let id = definition.id();
        assert!(
            self.materials.insert(id, definition).is_none(),
            "duplicate material id {}",
            id.value()
        );
    }

    /// Registers one exact authored material/form combination.
    pub(crate) fn register_commodity(&mut self, commodity: CommodityKey) {
        let material = self
            .materials
            .get(&commodity.material())
            .unwrap_or_else(|| {
                panic!(
                    "commodity references missing material {}",
                    commodity.material().value()
                )
            });
        let form = self.forms.get(&commodity.form()).unwrap_or_else(|| {
            panic!(
                "commodity references missing form {}",
                commodity.form().value()
            )
        });
        assert!(
            form.phase() != MaterialPhase::Liquid
                || material.properties().thermal().fusion().is_some(),
            "liquid commodity material {} form {} requires authored fusion properties",
            commodity.material().value(),
            commodity.form().value()
        );
        assert!(
            self.commodities.insert(commodity),
            "duplicate commodity material {} form {}",
            commodity.material().value(),
            commodity.form().value()
        );
    }

    /// Registers one authored form, panicking immediately on an ID collision.
    pub(crate) fn register_form(&mut self, definition: FormDefinition) {
        let id = definition.id();
        assert!(
            self.forms.insert(id, definition).is_none(),
            "duplicate material form id {}",
            id.value()
        );
    }

    /// Returns one material definition by stable ID.
    #[must_use]
    pub fn get_material(&self, id: MaterialId) -> Option<&MaterialDefinition> {
        self.materials.get(&id)
    }

    /// Iterates authored materials deterministically by stable material ID.
    pub(crate) fn definitions(&self) -> impl Iterator<Item = &MaterialDefinition> {
        self.materials.values()
    }

    /// Returns one physical-form definition by stable ID.
    #[must_use]
    pub fn get_form(&self, id: FormId) -> Option<&FormDefinition> {
        self.forms.get(&id)
    }

    /// Reports whether the exact material/form combination is authored for runtime ownership.
    #[must_use]
    pub fn has_commodity(&self, commodity: CommodityKey) -> bool {
        self.commodities.contains(&commodity)
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
