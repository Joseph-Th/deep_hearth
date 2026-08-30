//! Runtime material phase and particulate-state validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Temperature;

use super::composition::MaterialComposition;
use super::definitions::{MaterialPhase, ParticleSizeStatePolicy};
use super::identity::{CommodityKey, FormId, MaterialId};
use super::particle::ParticleSizeDistribution;
use super::registry::MaterialRegistry;

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

fn validate_solid_phase_state(
    materials: &MaterialRegistry,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<(), MaterialPhaseStateError> {
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

fn validate_liquid_phase_state(
    materials: &MaterialRegistry,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<(), MaterialPhaseStateError> {
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
        return Err(MaterialPhaseStateError::LiquidMaterialHasNoFusionProperties { material });
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

/// Validates that a material lot's authored form, composition, and temperature are physically
/// consistent with the represented solid/liquid phase model.
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
        MaterialPhase::Solid => validate_solid_phase_state(materials, composition, temperature),
        MaterialPhase::Liquid => {
            validate_liquid_phase_state(materials, commodity, composition, temperature)
        }
    }
}
