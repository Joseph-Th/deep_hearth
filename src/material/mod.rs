//! Immutable material/form/composition definitions; sibling `volume` resolves density-based physical volume.

mod volume;

pub use volume::{MaterialVolumeError, calculate_volume_ceiling};

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{Mass, Temperature};

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

/// One constituent fraction in a normalized runtime material composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CompositionComponent {
    material: MaterialId,
    parts_per_million: u32,
}

impl CompositionComponent {
    #[must_use]
    pub const fn new(material: MaterialId, parts_per_million: u32) -> Self {
        Self {
            material,
            parts_per_million,
        }
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.parts_per_million
    }
}

/// Structural validation failure for a normalized material composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionError {
    Empty,
    ZeroMaterialId,
    ZeroFraction {
        material: MaterialId,
    },
    DuplicateMaterial {
        material: MaterialId,
    },
    UnsortedMaterials {
        previous: MaterialId,
        current: MaterialId,
    },
    FractionSumOverflow,
    FractionSumMismatch {
        found: u64,
    },
}

impl Display for CompositionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("material composition must contain a constituent"),
            Self::ZeroMaterialId => {
                formatter.write_str("material composition must not reference material id zero")
            }
            Self::ZeroFraction { material } => write!(
                formatter,
                "material composition contains zero ppm for material {}",
                material.value()
            ),
            Self::DuplicateMaterial { material } => write!(
                formatter,
                "material composition contains duplicate material {}",
                material.value()
            ),
            Self::UnsortedMaterials { previous, current } => write!(
                formatter,
                "material composition is not sorted: material {} precedes {}",
                previous.value(),
                current.value()
            ),
            Self::FractionSumOverflow => {
                formatter.write_str("material composition fraction sum overflowed")
            }
            Self::FractionSumMismatch { found } => write!(
                formatter,
                "material composition totals {found} ppm instead of {COMPOSITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for CompositionError {}

/// Canonical normalized composition for one homogeneous material lot.
///
/// Components are mass fractions sorted by stable material ID and sum to exactly one million parts
/// per million.
/// This preserves deterministic serialization and bounded integer chemistry without multiplying
/// authored material definitions for every alloy ratio or ore grade.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MaterialComposition {
    components: Vec<CompositionComponent>,
}

#[derive(Deserialize)]
struct MaterialCompositionRepresentation {
    components: Vec<CompositionComponent>,
}

impl<'de> Deserialize<'de> for MaterialComposition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = MaterialCompositionRepresentation::deserialize(deserializer)?;
        let composition = Self {
            components: representation.components,
        };
        composition.validate().map_err(serde::de::Error::custom)?;
        Ok(composition)
    }
}

impl MaterialComposition {
    /// Builds a normalized composition, sorting components into canonical material-ID order.
    pub fn new(mut components: Vec<CompositionComponent>) -> Result<Self, CompositionError> {
        components.sort_by_key(|component| component.material());
        let composition = Self { components };
        composition.validate()?;
        Ok(composition)
    }

    /// Builds a pure single-material composition.
    #[must_use]
    pub fn pure(material: MaterialId) -> Self {
        assert!(
            material.value() != 0,
            "material composition host id must be nonzero"
        );
        Self {
            components: vec![CompositionComponent::new(
                material,
                COMPOSITION_PARTS_PER_MILLION,
            )],
        }
    }

    /// Validates canonical ordering and exact normalization, including after deserialization.
    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.components.is_empty() {
            return Err(CompositionError::Empty);
        }

        let mut total = 0_u64;
        let mut previous = None;
        for component in &self.components {
            if component.material().value() == 0 {
                return Err(CompositionError::ZeroMaterialId);
            }
            if component.parts_per_million() == 0 {
                return Err(CompositionError::ZeroFraction {
                    material: component.material(),
                });
            }
            if let Some(previous_material) = previous {
                if component.material() == previous_material {
                    return Err(CompositionError::DuplicateMaterial {
                        material: component.material(),
                    });
                }
                if component.material() < previous_material {
                    return Err(CompositionError::UnsortedMaterials {
                        previous: previous_material,
                        current: component.material(),
                    });
                }
            }
            total = total
                .checked_add(u64::from(component.parts_per_million()))
                .ok_or(CompositionError::FractionSumOverflow)?;
            previous = Some(component.material());
        }
        if total != u64::from(COMPOSITION_PARTS_PER_MILLION) {
            return Err(CompositionError::FractionSumMismatch { found: total });
        }
        Ok(())
    }

    /// Returns canonical constituent entries in stable material-ID order.
    #[must_use]
    pub fn components(&self) -> &[CompositionComponent] {
        &self.components
    }

    /// Returns one constituent fraction, or zero when the material is absent.
    #[must_use]
    pub fn parts_per_million(&self, material: MaterialId) -> u32 {
        match self
            .components
            .binary_search_by_key(&material, |component| component.material())
        {
            Ok(index) => self.components[index].parts_per_million(),
            Err(_) => 0,
        }
    }

    /// Projects one constituent's mass by flooring at the authoritative milligram boundary.
    ///
    /// Flooring is deliberate: this query never manufactures mass through rounding. Systems that
    /// need sub-milligram conservation must persist their own process remainder explicitly.
    #[must_use]
    pub fn constituent_mass_floor(&self, total_mass: Mass, material: MaterialId) -> Mass {
        let numerator =
            u128::from(total_mass.milligrams()) * u128::from(self.parts_per_million(material));
        let milligrams = numerator / u128::from(COMPOSITION_PARTS_PER_MILLION);
        debug_assert!(milligrams <= u128::from(u64::MAX));
        Mass::from_milligrams(milligrams as u64)
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

/// Inclusive constituent range required by a material-consuming operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompositionConstraint {
    material: MaterialId,
    minimum_parts_per_million: u32,
    maximum_parts_per_million: u32,
}

impl CompositionConstraint {
    /// Builds one inclusive constituent range.
    pub fn new(
        material: MaterialId,
        minimum_parts_per_million: u32,
        maximum_parts_per_million: u32,
    ) -> Result<Self, CompositionConstraintError> {
        if material.value() == 0 {
            return Err(CompositionConstraintError::ZeroMaterialId);
        }
        if minimum_parts_per_million > maximum_parts_per_million {
            return Err(CompositionConstraintError::MinimumExceedsMaximum {
                minimum: minimum_parts_per_million,
                maximum: maximum_parts_per_million,
            });
        }
        if maximum_parts_per_million > COMPOSITION_PARTS_PER_MILLION {
            return Err(CompositionConstraintError::MaximumExceedsNormalization {
                maximum: maximum_parts_per_million,
            });
        }
        Ok(Self {
            material,
            minimum_parts_per_million,
            maximum_parts_per_million,
        })
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn minimum_parts_per_million(self) -> u32 {
        self.minimum_parts_per_million
    }

    #[must_use]
    pub const fn maximum_parts_per_million(self) -> u32 {
        self.maximum_parts_per_million
    }

    #[must_use]
    pub fn matches(self, composition: &MaterialComposition) -> bool {
        let fraction = composition.parts_per_million(self.material);
        fraction >= self.minimum_parts_per_million && fraction <= self.maximum_parts_per_million
    }
}

/// Invalid constituent range for a material input specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionConstraintError {
    ZeroMaterialId,
    MinimumExceedsMaximum { minimum: u32, maximum: u32 },
    MaximumExceedsNormalization { maximum: u32 },
}

impl Display for CompositionConstraintError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMaterialId => {
                formatter.write_str("composition constraint material id must be nonzero")
            }
            Self::MinimumExceedsMaximum { minimum, maximum } => write!(
                formatter,
                "composition constraint minimum {minimum} ppm exceeds maximum {maximum} ppm"
            ),
            Self::MaximumExceedsNormalization { maximum } => write!(
                formatter,
                "composition constraint maximum {maximum} ppm exceeds {COMPOSITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for CompositionConstraintError {}

/// Matter requirement for a process, including optional composition ranges.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaterialInputSpec {
    commodity: CommodityKey,
    mass: Mass,
    constraints: Vec<CompositionConstraint>,
}

impl MaterialInputSpec {
    /// Builds an input requirement that accepts any composition with the requested host/form.
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass) -> Self {
        assert!(
            !mass.is_zero(),
            "material input specification mass must be nonzero"
        );
        Self {
            commodity,
            mass,
            constraints: Vec::new(),
        }
    }

    /// Builds a composition-constrained input requirement in canonical material-ID order.
    pub fn with_constraints(
        commodity: CommodityKey,
        mass: Mass,
        mut constraints: Vec<CompositionConstraint>,
    ) -> Result<Self, MaterialInputSpecError> {
        if mass.is_zero() {
            return Err(MaterialInputSpecError::ZeroMass);
        }
        constraints.sort_by_key(|constraint| constraint.material());
        for pair in constraints.windows(2) {
            if pair[0].material() == pair[1].material() {
                return Err(MaterialInputSpecError::DuplicateConstraint {
                    material: pair[0].material(),
                });
            }
        }
        Ok(Self {
            commodity,
            mass,
            constraints,
        })
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub fn constraints(&self) -> &[CompositionConstraint] {
        &self.constraints
    }

    #[must_use]
    pub fn matches_composition(&self, composition: &MaterialComposition) -> bool {
        self.constraints
            .iter()
            .all(|constraint| constraint.matches(composition))
    }
}

/// Construction failure for a composition-aware material input requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialInputSpecError {
    ZeroMass,
    DuplicateConstraint { material: MaterialId },
}

impl Display for MaterialInputSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("material input specification mass must be nonzero")
            }
            Self::DuplicateConstraint { material } => write!(
                formatter,
                "material input specification repeats constraint for material {}",
                material.value()
            ),
        }
    }
}

impl Error for MaterialInputSpecError {}

/// Specification for creating one homogeneous runtime material lot.
///
/// This is a boundary value shared by systems that produce matter. It is not a runtime record and
/// carries no owner or persistent lot ID; the inventory owner allocates those during canonical
/// commit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialLotSpec {
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
}

impl MaterialLotSpec {
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass, temperature: Temperature) -> Self {
        assert!(
            !mass.is_zero(),
            "material lot specification mass must be nonzero"
        );
        Self {
            commodity,
            mass,
            temperature,
            composition: MaterialComposition::pure(commodity.material()),
        }
    }

    /// Builds a lot specification with an explicit normalized composition.
    pub fn with_composition(
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
    ) -> Result<Self, MaterialLotSpecError> {
        if mass.is_zero() {
            return Err(MaterialLotSpecError::ZeroMass);
        }
        composition
            .validate()
            .map_err(MaterialLotSpecError::InvalidComposition)?;
        if composition.parts_per_million(commodity.material()) == 0 {
            return Err(MaterialLotSpecError::MissingHostMaterial {
                host: commodity.material(),
            });
        }
        Ok(Self {
            commodity,
            mass,
            temperature,
            composition,
        })
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
}

/// Construction failure for a material lot specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialLotSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

impl Display for MaterialLotSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("material lot specification mass must be nonzero")
            }
            Self::InvalidComposition(error) => {
                write!(formatter, "invalid lot composition: {error}")
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "material lot composition does not contain host material {}",
                host.value()
            ),
        }
    }
}

impl Error for MaterialLotSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass | Self::MissingHostMaterial { .. } => None,
        }
    }
}

/// Thermal properties used by heat transfer and phase-change systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThermalProperties {
    specific_heat_j_per_kg_k: u32,
    melting_point: Option<Temperature>,
    conductivity_milli_w_per_m_k: u32,
}

impl ThermalProperties {
    #[must_use]
    pub const fn new(
        specific_heat_j_per_kg_k: u32,
        melting_point: Option<Temperature>,
        conductivity_milli_w_per_m_k: u32,
    ) -> Self {
        assert!(
            specific_heat_j_per_kg_k > 0,
            "material specific heat must be nonzero"
        );
        Self {
            specific_heat_j_per_kg_k,
            melting_point,
            conductivity_milli_w_per_m_k,
        }
    }

    #[must_use]
    pub const fn specific_heat_j_per_kg_k(&self) -> u32 {
        self.specific_heat_j_per_kg_k
    }

    #[must_use]
    pub const fn melting_point(&self) -> Option<Temperature> {
        self.melting_point
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

/// Immutable authored physical-form definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDefinition {
    id: FormId,
    name: String,
}

impl FormDefinition {
    /// Builds an immutable material-form definition for registry insertion.
    #[must_use]
    pub fn new(id: FormId, name: impl Into<String>) -> Self {
        assert!(id.value() != 0, "material form id must be nonzero");
        let name = name.into();
        assert!(
            !name.trim().is_empty(),
            "material form name must not be empty"
        );
        Self { id, name }
    }

    #[must_use]
    pub const fn id(&self) -> FormId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Immutable deterministic lookup tables for materials and their physical forms.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialRegistry {
    materials: BTreeMap<MaterialId, MaterialDefinition>,
    forms: BTreeMap<FormId, FormDefinition>,
}

impl MaterialRegistry {
    /// Builds an empty registry for code-owned startup assembly.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            materials: BTreeMap::new(),
            forms: BTreeMap::new(),
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

    /// Returns one physical-form definition by stable ID.
    #[must_use]
    pub fn get_form(&self, id: FormId) -> Option<&FormDefinition> {
        self.forms.get(&id)
    }

    /// Reports whether both references in a commodity key resolve.
    #[must_use]
    pub fn has_commodity(&self, commodity: CommodityKey) -> bool {
        self.materials.contains_key(&commodity.material())
            && self.forms.contains_key(&commodity.form())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_properties() -> MaterialProperties {
        MaterialProperties::new(
            1_000,
            ThermalProperties::new(1_000, None, 100),
            MechanicalProperties::new(10, 10, 10),
            ElectricalProperties::new(None),
        )
    }

    #[test]
    fn commodity_requires_both_material_and_form_to_resolve() {
        let mut registry = MaterialRegistry::new();
        let material = MaterialId::new(3);
        let form = FormId::new(7);
        registry.register_material(MaterialDefinition::new(
            material,
            "test material",
            make_test_properties(),
        ));

        assert!(!registry.has_commodity(CommodityKey::new(material, form)));

        registry.register_form(FormDefinition::new(form, "test form"));
        assert!(registry.has_commodity(CommodityKey::new(material, form)));
    }

    #[test]
    fn composition_normalizes_order_and_projects_constituent_mass_without_rounding_up() {
        let copper = MaterialId::new(3);
        let slag = MaterialId::new(4);
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(slag, 275_000),
            CompositionComponent::new(copper, 725_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition unexpectedly failed: {error}"),
        };

        assert_eq!(composition.components()[0].material(), copper);
        assert_eq!(composition.components()[1].material(), slag);
        assert_eq!(composition.parts_per_million(copper), 725_000);
        assert_eq!(
            composition.constituent_mass_floor(Mass::from_milligrams(3), copper),
            Mass::from_milligrams(2)
        );
        assert_eq!(
            composition.constituent_mass_floor(Mass::from_milligrams(3), slag),
            Mass::ZERO
        );
    }

    #[test]
    fn composition_rejects_non_normalized_fraction_total() {
        let result = MaterialComposition::new(vec![
            CompositionComponent::new(MaterialId::new(3), 500_000),
            CompositionComponent::new(MaterialId::new(4), 499_999),
        ]);

        assert_eq!(
            result,
            Err(CompositionError::FractionSumMismatch { found: 999_999 })
        );
    }

    #[test]
    fn composition_deserialization_rejects_noncanonical_order() {
        let encoded = br#"{
            "components": [
                {"material":4,"parts_per_million":500000},
                {"material":3,"parts_per_million":500000}
            ]
        }"#;
        let result: Result<MaterialComposition, _> = serde_json::from_slice(encoded);

        assert!(result.is_err());
    }

    #[test]
    fn material_input_constraints_match_composition_ranges_inclusively() {
        let copper = MaterialId::new(3);
        let slag = MaterialId::new(4);
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(copper, 800_000),
            CompositionComponent::new(slag, 200_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition unexpectedly failed: {error}"),
        };
        let accepts = match CompositionConstraint::new(copper, 800_000, 900_000) {
            Ok(constraint) => constraint,
            Err(error) => panic!("constraint unexpectedly failed: {error}"),
        };
        let rejects = match CompositionConstraint::new(slag, 0, 199_999) {
            Ok(constraint) => constraint,
            Err(error) => panic!("constraint unexpectedly failed: {error}"),
        };

        assert!(accepts.matches(&composition));
        assert!(!rejects.matches(&composition));
    }
}
