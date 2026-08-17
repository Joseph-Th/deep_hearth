//! Exhaustive persistence validation for player survival quantities.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::fluid::{FluidDefinitionId, FluidRegistry};
use crate::material::{MaterialId, MaterialRegistry};

use super::{SurvivalRegistry, SurvivalState, Vitality};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalValidationError {
    EnergyExceedsMaximum,
    HydrationExceedsMaximum,
    VitalityExceedsMaximum,
    OwnedMatterWithoutPlayer,
    UnknownMetabolicMaterial { material: MaterialId },
    ZeroMetabolicMass { material: MaterialId },
    UnknownIngestedFluid { fluid: FluidDefinitionId },
    ZeroIngestedFluidVolume { fluid: FluidDefinitionId },
}

impl Display for SurvivalValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnergyExceedsMaximum => {
                formatter.write_str("player metabolic energy exceeds authored maximum")
            }
            Self::HydrationExceedsMaximum => {
                formatter.write_str("player hydration exceeds authored maximum")
            }
            Self::VitalityExceedsMaximum => {
                formatter.write_str("player vitality exceeds normalized maximum")
            }
            Self::OwnedMatterWithoutPlayer => {
                formatter.write_str("survival owner contains ingested matter without a player")
            }
            Self::UnknownMetabolicMaterial { material } => write!(
                formatter,
                "survival metabolism references unknown material {}",
                material.value()
            ),
            Self::ZeroMetabolicMass { material } => write!(
                formatter,
                "survival metabolism stores zero mass for material {}",
                material.value()
            ),
            Self::UnknownIngestedFluid { fluid } => write!(
                formatter,
                "survival ingestion references unknown fluid {}",
                fluid.value()
            ),
            Self::ZeroIngestedFluidVolume { fluid } => write!(
                formatter,
                "survival ingestion stores zero volume for fluid {}",
                fluid.value()
            ),
        }
    }
}

impl Error for SurvivalValidationError {}

pub(crate) fn validate_loaded_survival(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
) -> Result<(), SurvivalValidationError> {
    let Some(player) = state.player() else {
        if state.metabolic_matter().next().is_some() || state.ingested_fluids().next().is_some() {
            return Err(SurvivalValidationError::OwnedMatterWithoutPlayer);
        }
        return Ok(());
    };
    let physiology = registry.physiology();
    if player.metabolic_energy() > physiology.maximum_metabolic_energy() {
        return Err(SurvivalValidationError::EnergyExceedsMaximum);
    }
    if player.hydration() > physiology.maximum_hydration() {
        return Err(SurvivalValidationError::HydrationExceedsMaximum);
    }
    if player.vitality().parts_per_million() > Vitality::MAXIMUM.parts_per_million() {
        return Err(SurvivalValidationError::VitalityExceedsMaximum);
    }
    for (material, mass) in state.metabolic_matter() {
        if materials.get_material(material).is_none() || !registry.has_food_material(material) {
            return Err(SurvivalValidationError::UnknownMetabolicMaterial { material });
        }
        if mass == crate::core::quantity::AggregateMass::ZERO {
            return Err(SurvivalValidationError::ZeroMetabolicMass { material });
        }
    }
    for (fluid, volume) in state.ingested_fluids() {
        if fluids.get_fluid(fluid).is_none() || registry.get_drink(fluid).is_none() {
            return Err(SurvivalValidationError::UnknownIngestedFluid { fluid });
        }
        if volume == crate::core::quantity::AggregateVolume::ZERO {
            return Err(SurvivalValidationError::ZeroIngestedFluidVolume { fluid });
        }
    }
    Ok(())
}
