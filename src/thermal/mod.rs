//! Deterministic material thermal-energy calculations; sibling process code owns physically resolved heating and phase change.

mod casting_execution;
mod melting_execution;
mod processes;

pub use casting_execution::{
    CastingBatchError, CastingJobValidationError, CastingProcessDefinition, CastingRequest,
    CastingResolutionError, ResolvedCasting, resolve_casting_process,
};
pub use melting_execution::{
    MeltingBatchError, MeltingJobValidationError, MeltingProcessDefinition, MeltingRequest,
    MeltingResolutionError, ResolvedMelting, resolve_melting_process,
};
pub use processes::{
    ResolvedSensibleHeating, SensibleHeatingProcessDefinition, SensibleHeatingRequest,
    SensibleHeatingResolutionError, ThermalJobValidationError, ThermalRegistry,
    resolve_sensible_heating_process,
};

pub(crate) use processes::validate_loaded_thermal_job;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialComposition, MaterialId, MaterialPhase,
    MaterialPhaseStateError, MaterialRegistry, validate_material_phase_state,
};

/// Direction of sensible heat transfer relative to the material lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeatDirection {
    None,
    IntoMaterial,
    OutOfMaterial,
}

/// Exact sensible-heat requirement for a temperature change that crosses no phase boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SensibleHeat {
    energy: Energy,
    direction: HeatDirection,
}

impl SensibleHeat {
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }

    #[must_use]
    pub const fn direction(self) -> HeatDirection {
        self.direction
    }
}

/// Failure to apply the linear sensible-heat approximation safely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleHeatError {
    InvalidComposition(CompositionError),
    UnknownMaterial {
        material: MaterialId,
    },
    PhaseBoundaryCrossed {
        material: MaterialId,
        melting_point: Temperature,
    },
    ArithmeticOverflow,
}

impl Display for SensibleHeatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidComposition(error) => {
                write!(formatter, "invalid material composition: {error}")
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown material {} in thermal composition",
                    material.value()
                )
            }
            Self::PhaseBoundaryCrossed {
                material,
                melting_point,
            } => write!(
                formatter,
                "sensible-heat calculation crosses material {} melting point at {} mK",
                material.value(),
                melting_point.millikelvin()
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("sensible-heat calculation overflowed authoritative energy")
            }
        }
    }
}

impl Error for SensibleHeatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::UnknownMaterial {
                material: _material,
            } => None,
            Self::PhaseBoundaryCrossed {
                material: _material,
                melting_point: _melting_point,
            } => None,
            Self::ArithmeticOverflow => None,
        }
    }
}

fn calculate_linear_sensible_heat(
    materials: &MaterialRegistry,
    mass: Mass,
    composition: &MaterialComposition,
    current: Temperature,
    target: Temperature,
) -> Result<SensibleHeat, SensibleHeatError> {
    composition
        .validate()
        .map_err(SensibleHeatError::InvalidComposition)?;

    if current == target || mass.is_zero() {
        return Ok(SensibleHeat {
            energy: Energy::ZERO,
            direction: HeatDirection::None,
        });
    }

    let mut weighted_specific_heat = 0_u128;
    for component in composition.components() {
        let Some(definition) = materials.get_material(component.material()) else {
            return Err(SensibleHeatError::UnknownMaterial {
                material: component.material(),
            });
        };
        let contribution = u128::from(definition.properties().thermal().specific_heat_j_per_kg_k())
            .checked_mul(u128::from(component.parts_per_million()))
            .ok_or(SensibleHeatError::ArithmeticOverflow)?;
        weighted_specific_heat = weighted_specific_heat
            .checked_add(contribution)
            .ok_or(SensibleHeatError::ArithmeticOverflow)?;
    }

    let delta_millikelvin = u128::from(current.millikelvin().abs_diff(target.millikelvin()));
    let numerator = u128::from(mass.milligrams())
        .checked_mul(delta_millikelvin)
        .and_then(|value| value.checked_mul(weighted_specific_heat))
        .ok_or(SensibleHeatError::ArithmeticOverflow)?;
    let energy = Energy::from_nanojoules(
        numerator / u128::from(crate::material::COMPOSITION_PARTS_PER_MILLION),
    );
    let direction = if target > current {
        HeatDirection::IntoMaterial
    } else {
        HeatDirection::OutOfMaterial
    };

    Ok(SensibleHeat { energy, direction })
}

/// Calculates sensible heat for a homogeneous composition without crossing a constituent melt.
///
/// The calculation integrates each constituent's authored specific heat by integer ppm. Latent
/// heat is deliberately not approximated. Reaching a melting point from the solid side is allowed;
/// moving beyond it requires a dedicated phase-change resolver.
pub fn calculate_sensible_heat(
    materials: &MaterialRegistry,
    mass: Mass,
    composition: &MaterialComposition,
    current: Temperature,
    target: Temperature,
) -> Result<SensibleHeat, SensibleHeatError> {
    composition
        .validate()
        .map_err(SensibleHeatError::InvalidComposition)?;
    for component in composition.components() {
        let Some(definition) = materials.get_material(component.material()) else {
            return Err(SensibleHeatError::UnknownMaterial {
                material: component.material(),
            });
        };
        if let Some(melting_point) = definition.properties().thermal().melting_point() {
            let crosses = if target > current {
                current <= melting_point && melting_point < target
            } else if target < current {
                target < melting_point && melting_point <= current
            } else {
                false
            };
            if crosses {
                return Err(SensibleHeatError::PhaseBoundaryCrossed {
                    material: component.material(),
                    melting_point,
                });
            }
        }
    }
    calculate_linear_sensible_heat(materials, mass, composition, current, target)
}

/// Failure to calculate sensible heat while preserving one authored material phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhaseSensibleHeatError {
    InvalidCurrentState(MaterialPhaseStateError),
    InvalidTargetState(MaterialPhaseStateError),
    Heat(SensibleHeatError),
}

impl Display for PhaseSensibleHeatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCurrentState(error) => {
                write!(
                    formatter,
                    "current material phase state is invalid: {error}"
                )
            }
            Self::InvalidTargetState(error) => {
                write!(formatter, "target material phase state is invalid: {error}")
            }
            Self::Heat(error) => write!(formatter, "sensible-heat calculation failed: {error}"),
        }
    }
}

impl Error for PhaseSensibleHeatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCurrentState(error) | Self::InvalidTargetState(error) => Some(error),
            Self::Heat(error) => Some(error),
        }
    }
}

/// Calculates sensible heat while retaining the lot's explicitly authored solid or liquid form.
///
/// Both endpoints must be valid for the same unchanged commodity form. Once that phase constraint
/// is proven, the temperature interval can use the linear sensible-heat calculation directly. This
/// permits a liquid already at its fusion boundary to heat upward without falsely treating that as
/// a new phase transition, while still rejecting a solid target above melting or a liquid target
/// below melting.
pub fn calculate_phase_sensible_heat(
    materials: &MaterialRegistry,
    mass: Mass,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    current: Temperature,
    target: Temperature,
) -> Result<SensibleHeat, PhaseSensibleHeatError> {
    validate_material_phase_state(materials, commodity, composition, current)
        .map_err(PhaseSensibleHeatError::InvalidCurrentState)?;
    validate_material_phase_state(materials, commodity, composition, target)
        .map_err(PhaseSensibleHeatError::InvalidTargetState)?;
    calculate_linear_sensible_heat(materials, mass, composition, current, target)
        .map_err(PhaseSensibleHeatError::Heat)
}

/// Exact latent-energy requirement for melting one pure material mass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FusionHeat {
    energy: Energy,
    melting_point: Temperature,
}

impl FusionHeat {
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }

    #[must_use]
    pub const fn melting_point(self) -> Temperature {
        self.melting_point
    }
}

/// Failure to resolve latent heat from authored material properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionHeatError {
    UnknownMaterial { material: MaterialId },
    MissingFusionProperties { material: MaterialId },
    ArithmeticOverflow,
}

impl Display for FusionHeatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown material {} in fusion calculation",
                    material.value()
                )
            }
            Self::MissingFusionProperties { material } => write!(
                formatter,
                "material {} has no authored solid/liquid fusion properties",
                material.value()
            ),
            Self::ArithmeticOverflow => formatter
                .write_str("fusion latent-heat calculation overflowed authoritative energy"),
        }
    }
}

impl Error for FusionHeatError {}

/// Calculates exact latent heat for melting a pure material mass at its authored fusion boundary.
pub fn calculate_fusion_heat(
    materials: &MaterialRegistry,
    mass: Mass,
    material: MaterialId,
) -> Result<FusionHeat, FusionHeatError> {
    let Some(definition) = materials.get_material(material) else {
        return Err(FusionHeatError::UnknownMaterial { material });
    };
    let Some(fusion) = definition.properties().thermal().fusion() else {
        return Err(FusionHeatError::MissingFusionProperties { material });
    };
    let nanojoules = u128::from(mass.milligrams())
        .checked_mul(u128::from(fusion.latent_heat_j_per_kg()))
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(FusionHeatError::ArithmeticOverflow)?;
    Ok(FusionHeat {
        energy: Energy::from_nanojoules(nanojoules),
        melting_point: fusion.melting_point(),
    })
}

/// Failure to project a material lot's modeled sensible plus latent thermal energy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialThermalEnergyError {
    UnknownForm {
        form: FormId,
    },
    ImpureLiquidComposition,
    LiquidHostMismatch {
        host: MaterialId,
        pure: MaterialId,
    },
    LiquidBelowMeltingPoint {
        material: MaterialId,
        temperature: Temperature,
        melting_point: Temperature,
    },
    SensibleHeat(SensibleHeatError),
    FusionHeat(FusionHeatError),
    ArithmeticOverflow,
}

impl Display for MaterialThermalEnergyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownForm { form } => {
                write!(formatter, "unknown material form {} in thermal accounting", form.value())
            }
            Self::ImpureLiquidComposition => formatter.write_str(
                "liquid thermal accounting requires a pure material until mixture phase diagrams exist",
            ),
            Self::LiquidHostMismatch { host, pure } => write!(
                formatter,
                "liquid commodity host material {} disagrees with pure composition material {}",
                host.value(),
                pure.value()
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
            Self::SensibleHeat(error) => write!(formatter, "sensible heat failed: {error}"),
            Self::FusionHeat(error) => write!(formatter, "fusion heat failed: {error}"),
            Self::ArithmeticOverflow => {
                formatter.write_str("material thermal-energy accounting overflowed")
            }
        }
    }
}

impl Error for MaterialThermalEnergyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat(error) => Some(error),
            Self::FusionHeat(error) => Some(error),
            Self::UnknownForm { form: _form } => None,
            Self::LiquidHostMismatch {
                host: _host,
                pure: _pure,
            } => None,
            Self::LiquidBelowMeltingPoint {
                material: _material,
                temperature: _temperature,
                melting_point: _melting_point,
            } => None,
            Self::ImpureLiquidComposition | Self::ArithmeticOverflow => None,
        }
    }
}

/// Calculates modeled material thermal energy relative to absolute zero.
///
/// Solid forms carry sensible heat only and may reach, but not cross, a fusion boundary. Liquid
/// forms additionally carry authored latent heat and are restricted to pure materials until alloy
/// phase diagrams are represented explicitly.
pub fn calculate_material_thermal_energy(
    materials: &MaterialRegistry,
    mass: Mass,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<Energy, MaterialThermalEnergyError> {
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(MaterialThermalEnergyError::UnknownForm {
            form: commodity.form(),
        });
    };
    match form.phase() {
        MaterialPhase::Solid => {
            calculate_sensible_heat(materials, mass, composition, Temperature::ZERO, temperature)
                .map(SensibleHeat::energy)
                .map_err(MaterialThermalEnergyError::SensibleHeat)
        }
        MaterialPhase::Liquid => {
            let Some(material) = composition.pure_material() else {
                return Err(MaterialThermalEnergyError::ImpureLiquidComposition);
            };
            if commodity.material() != material {
                return Err(MaterialThermalEnergyError::LiquidHostMismatch {
                    host: commodity.material(),
                    pure: material,
                });
            }
            let fusion = calculate_fusion_heat(materials, mass, material)
                .map_err(MaterialThermalEnergyError::FusionHeat)?;
            if temperature < fusion.melting_point() {
                return Err(MaterialThermalEnergyError::LiquidBelowMeltingPoint {
                    material,
                    temperature,
                    melting_point: fusion.melting_point(),
                });
            }
            let sensible = calculate_linear_sensible_heat(
                materials,
                mass,
                composition,
                Temperature::ZERO,
                temperature,
            )
            .map_err(MaterialThermalEnergyError::SensibleHeat)?;
            sensible
                .energy()
                .checked_add(fusion.energy())
                .ok_or(MaterialThermalEnergyError::ArithmeticOverflow)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
    use crate::material::{CompositionComponent, MaterialComposition};

    #[test]
    fn pure_copper_sensible_heat_is_exact_at_integer_scales() {
        let registries = build_registries();
        let composition = MaterialComposition::pure(MATERIAL_COPPER);

        let heat = match calculate_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(10_000),
            &composition,
            Temperature::from_millikelvin(300_000),
            Temperature::from_millikelvin(301_000),
        ) {
            Ok(heat) => heat,
            Err(error) => panic!("thermal calculation failed: {error}"),
        };

        assert_eq!(heat.direction(), HeatDirection::IntoMaterial);
        assert_eq!(heat.energy(), Energy::from_nanojoules(3_850_000_000));
    }

    #[test]
    fn mixed_composition_weights_specific_heat_by_normalized_fraction() {
        let registries = build_registries();
        let composition = match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, 500_000),
            CompositionComponent::new(MATERIAL_SLAG, 500_000),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition fixture failed: {error}"),
        };
        let copper_cp = match registries.materials().get_material(MATERIAL_COPPER) {
            Some(material) => material.properties().thermal().specific_heat_j_per_kg_k(),
            None => panic!("built-in copper disappeared"),
        };
        let slag_cp = match registries.materials().get_material(MATERIAL_SLAG) {
            Some(material) => material.properties().thermal().specific_heat_j_per_kg_k(),
            None => panic!("built-in slag disappeared"),
        };
        let expected_energy =
            1_000_u128 * 1_000_u128 * u128::from(copper_cp + slag_cp) * 500_000_u128
                / 1_000_000_u128;

        let heat = match calculate_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            &composition,
            Temperature::from_millikelvin(300_000),
            Temperature::from_millikelvin(301_000),
        ) {
            Ok(heat) => heat,
            Err(error) => panic!("mixed thermal calculation failed: {error}"),
        };

        assert_eq!(heat.energy().nanojoules(), expected_energy);
    }

    #[test]
    fn sensible_heat_reaches_but_does_not_cross_a_melting_point() {
        let registries = build_registries();
        let composition = MaterialComposition::pure(MATERIAL_COPPER);
        let copper = match registries.materials().get_material(MATERIAL_COPPER) {
            Some(material) => material,
            None => panic!("built-in copper disappeared"),
        };
        let melting_point = match copper.properties().thermal().melting_point() {
            Some(value) => value,
            None => panic!("built-in copper has no melting point"),
        };

        let to_boundary = calculate_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            &composition,
            Temperature::from_millikelvin(melting_point.millikelvin() - 1_000),
            melting_point,
        );
        assert!(to_boundary.is_ok());

        let result = calculate_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            &composition,
            Temperature::from_millikelvin(melting_point.millikelvin() - 1_000),
            Temperature::from_millikelvin(melting_point.millikelvin() + 1_000),
        );

        assert_eq!(
            result,
            Err(SensibleHeatError::PhaseBoundaryCrossed {
                material: MATERIAL_COPPER,
                melting_point,
            })
        );
    }

    #[test]
    fn copper_fusion_heat_uses_authored_latent_energy_exactly() {
        let registries = build_registries();
        let fusion = match calculate_fusion_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            MATERIAL_COPPER,
        ) {
            Ok(fusion) => fusion,
            Err(error) => panic!("fusion calculation failed: {error}"),
        };

        assert_eq!(fusion.energy(), Energy::from_nanojoules(205_000_000_000));
        assert_eq!(
            fusion.melting_point(),
            Temperature::from_millikelvin(1_357_770)
        );
    }

    #[test]
    fn liquid_internal_energy_adds_latent_heat_at_the_phase_boundary() {
        let registries = build_registries();
        let composition = MaterialComposition::pure(MATERIAL_COPPER);
        let melting_point = Temperature::from_millikelvin(1_357_770);
        let mass = Mass::from_milligrams(1_000);
        let solid = match calculate_material_thermal_energy(
            registries.materials(),
            mass,
            crate::material::CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_INGOT),
            &composition,
            melting_point,
        ) {
            Ok(energy) => energy,
            Err(error) => panic!("solid internal-energy calculation failed: {error}"),
        };
        let liquid = match calculate_material_thermal_energy(
            registries.materials(),
            mass,
            crate::material::CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_MOLTEN),
            &composition,
            melting_point,
        ) {
            Ok(energy) => energy,
            Err(error) => panic!("liquid internal-energy calculation failed: {error}"),
        };

        assert_eq!(
            liquid.checked_sub(solid),
            Some(Energy::from_nanojoules(205_000_000_000))
        );
    }

    #[test]
    fn phase_sensible_heat_allows_liquid_to_heat_up_from_fusion_boundary() {
        let registries = build_registries();
        let composition = MaterialComposition::pure(MATERIAL_COPPER);
        let current = Temperature::from_millikelvin(1_357_770);
        let target = Temperature::from_millikelvin(1_400_000);

        let heat = match calculate_phase_sensible_heat(
            registries.materials(),
            Mass::from_milligrams(1_000),
            CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_MOLTEN),
            &composition,
            current,
            target,
        ) {
            Ok(heat) => heat,
            Err(error) => panic!("liquid sensible heating failed: {error}"),
        };

        assert_eq!(heat.direction(), HeatDirection::IntoMaterial);
        assert_eq!(heat.energy(), Energy::from_nanojoules(16_258_550_000));
    }

    #[test]
    fn phase_sensible_heat_rejects_solid_target_above_fusion_boundary() {
        let registries = build_registries();
        let composition = MaterialComposition::pure(MATERIAL_COPPER);
        let target = Temperature::from_millikelvin(1_357_771);

        assert!(matches!(
            calculate_phase_sensible_heat(
                registries.materials(),
                Mass::from_milligrams(1_000),
                CommodityKey::new(MATERIAL_COPPER, crate::content::FORM_INGOT),
                &composition,
                Temperature::from_millikelvin(1_357_770),
                target,
            ),
            Err(PhaseSensibleHeatError::InvalidTargetState(
                MaterialPhaseStateError::SolidAboveMeltingPoint {
                    material: _material,
                    temperature: _temperature,
                    melting_point: _melting_point,
                }
            ))
        ));
    }
}
