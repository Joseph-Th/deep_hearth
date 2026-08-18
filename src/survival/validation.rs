//! Exhaustive persistence validation for player survival quantities.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::fluid::{FluidDefinitionId, FluidRegistry};
use crate::material::{MaterialId, MaterialRegistry};

use super::{FoodCategory, NUTRITION_PARTS_PER_MILLION, SurvivalRegistry, SurvivalState, Vitality};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalValidationError {
    EnergyExceedsMaximum,
    HydrationExceedsMaximum,
    VitalityExceedsMaximum,
    NutritionExceedsMaximum { category: FoodCategory, value: u32 },
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
            Self::NutritionExceedsMaximum { category, value } => write!(
                formatter,
                "player {category:?} nutrition reserve {value} ppm exceeds normalized maximum"
            ),
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
    for category in [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ] {
        let value = player.nutrition().get(category);
        if value > NUTRITION_PARTS_PER_MILLION {
            return Err(SurvivalValidationError::NutritionExceedsMaximum { category, value });
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::build_registries;
    use crate::core::state::{AppState, StateValidationError};
    use crate::core::time::WorldSeed;
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::survival::initialize_player_survival;

    #[test]
    fn load_rejects_nutrition_reserve_above_normalized_maximum() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_1001));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("nutrition validation setup failed: {error}"));
        let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("nutrition validation serialization failed: {error}"));
        encoded["state"]["systems"]["survival"]["player"]["nutrition"]["grain"] =
            serde_json::json!(NUTRITION_PARTS_PER_MILLION + 1);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
            .unwrap_or_else(|error| panic!("nutrition validation decode failed: {error}"));

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Survival(
                SurvivalValidationError::NutritionExceedsMaximum {
                    category: FoodCategory::Grain,
                    value: NUTRITION_PARTS_PER_MILLION + 1,
                }
            )))
        );
    }
}
