//! Absolute modeled thermal-energy accounting across solid and liquid material forms.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, PreciseEnergy, Temperature};
use crate::material::{
    CommodityKey, FormId, MaterialComposition, MaterialId, MaterialPhase, MaterialRegistry,
};

use super::{
    FusionHeatError, SensibleHeatError, calculate_fusion_heat,
    calculate_linear_sensible_heat_precise, validate_sensible_heat_interval,
};

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
                write!(
                    formatter,
                    "unknown material form {} in thermal accounting",
                    form.value()
                )
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
) -> Result<PreciseEnergy, MaterialThermalEnergyError> {
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(MaterialThermalEnergyError::UnknownForm {
            form: commodity.form(),
        });
    };
    match form.phase() {
        MaterialPhase::Solid => {
            composition
                .validate()
                .map_err(SensibleHeatError::InvalidComposition)
                .map_err(MaterialThermalEnergyError::SensibleHeat)?;
            validate_sensible_heat_interval(materials, composition, Temperature::ZERO, temperature)
                .map_err(MaterialThermalEnergyError::SensibleHeat)?;
            calculate_linear_sensible_heat_precise(
                materials,
                mass,
                composition,
                Temperature::ZERO,
                temperature,
            )
            .map(|(energy, _direction)| energy)
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
            let (sensible, _direction) = calculate_linear_sensible_heat_precise(
                materials,
                mass,
                composition,
                Temperature::ZERO,
                temperature,
            )
            .map_err(MaterialThermalEnergyError::SensibleHeat)?;
            sensible
                .checked_add(PreciseEnergy::from_energy(fusion.energy()))
                .ok_or(MaterialThermalEnergyError::ArithmeticOverflow)
        }
    }
}
