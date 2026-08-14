//! Deterministic sensible-heat calculations over material composition; phase-changing systems must resolve boundaries explicitly.

mod processes;

pub use processes::{
    ResolvedSensibleHeating, SensibleHeatingProcessDefinition, SensibleHeatingRequest,
    SensibleHeatingResolutionError, ThermalJobValidationError, ThermalRegistry,
    resolve_sensible_heating_process,
};

pub(crate) use processes::validate_loaded_thermal_job;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::inventory::MaterialLotRecord;
use crate::material::{CompositionError, MaterialComposition, MaterialId, MaterialRegistry};

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
            Self::UnknownMaterial { .. }
            | Self::PhaseBoundaryCrossed { .. }
            | Self::ArithmeticOverflow => None,
        }
    }
}

/// Calculates sensible heat for a homogeneous composition without crossing a constituent melt.
///
/// The calculation integrates each constituent's authored specific heat by integer ppm. Latent
/// heat is deliberately not approximated: if the interval touches any constituent melting point,
/// the caller must resolve phase change through a dedicated metallurgy/thermal process.
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

    if current == target || mass.is_zero() {
        return Ok(SensibleHeat {
            energy: Energy::ZERO,
            direction: HeatDirection::None,
        });
    }

    let lower = std::cmp::min(current, target);
    let upper = std::cmp::max(current, target);
    let mut weighted_specific_heat = 0_u128;
    for component in composition.components() {
        let Some(definition) = materials.get_material(component.material()) else {
            return Err(SensibleHeatError::UnknownMaterial {
                material: component.material(),
            });
        };
        if let Some(melting_point) = definition.properties().thermal().melting_point()
            && lower <= melting_point
            && melting_point <= upper
        {
            return Err(SensibleHeatError::PhaseBoundaryCrossed {
                material: component.material(),
                melting_point,
            });
        }

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

/// Convenience calculation for one authoritative material lot.
pub fn calculate_lot_sensible_heat(
    materials: &MaterialRegistry,
    lot: &MaterialLotRecord,
    target: Temperature,
) -> Result<SensibleHeat, SensibleHeatError> {
    calculate_sensible_heat(
        materials,
        lot.mass(),
        lot.composition(),
        lot.temperature(),
        target,
    )
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
    fn sensible_heat_refuses_to_cross_a_melting_point() {
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
}
