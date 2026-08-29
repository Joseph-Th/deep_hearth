//! Exhaustive persistence validation for player survival quantities.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::fluid::{FluidDefinitionId, FluidRegistry};
use crate::material::{MaterialId, MaterialRegistry};

use super::state::PlayerSurvivalRecord;
use super::{FoodCategory, NUTRITION_PARTS_PER_MILLION, SurvivalRegistry, SurvivalState, Vitality};

mod direct_consumption;

use direct_consumption::validate_pending_consumption;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalValidationError {
    EnergyExceedsMaximum,
    HydrationExceedsMaximum,
    VitalityExceedsMaximum,
    VitalityRecoveryRemainderOutOfRange { value: u32 },
    VitalityRecoveryRemainderAtMaximum { value: u32 },
    NutritionExceedsMaximum { category: FoodCategory, value: u32 },
    ConsumedMatterWithoutPlayer,
    UnknownConsumedMaterial { material: MaterialId },
    ZeroConsumedMass { material: MaterialId },
    UnknownConsumedFluid { fluid: FluidDefinitionId },
    ZeroConsumedFluidVolume { fluid: FluidDefinitionId },
    PendingConsumptionWithoutPlayer,
    PendingConsumptionScheduleInvalid,
    PendingEatingEmpty,
    PendingEatingMassOverflow,
    PendingEatingMassExceedsIntakeLimit,
    PendingEatingTraceInvalid,
    PendingEatingAccountingMismatch { material: MaterialId },
    PendingDrinkingVolumeInvalid,
    PendingDrinkingUnknownFluid { fluid: FluidDefinitionId },
    PendingDrinkingNotDrinkable { fluid: FluidDefinitionId },
    PendingDrinkingTemperatureInvalid,
    PendingDrinkingAccountingMismatch { fluid: FluidDefinitionId },
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
            Self::VitalityRecoveryRemainderOutOfRange { value } => write!(
                formatter,
                "player vitality recovery remainder {value} must be below {NUTRITION_PARTS_PER_MILLION}"
            ),
            Self::VitalityRecoveryRemainderAtMaximum { value } => write!(
                formatter,
                "player at maximum vitality cannot retain fractional recovery remainder {value}"
            ),
            Self::NutritionExceedsMaximum { category, value } => write!(
                formatter,
                "player {category:?} nutrition reserve {value} ppm exceeds normalized maximum"
            ),
            Self::ConsumedMatterWithoutPlayer => {
                formatter.write_str("survival owner contains consumed matter without a player")
            }
            Self::UnknownConsumedMaterial { material } => write!(
                formatter,
                "survival consumption references unknown material {}",
                material.value()
            ),
            Self::ZeroConsumedMass { material } => write!(
                formatter,
                "survival consumption stores zero mass for material {}",
                material.value()
            ),
            Self::UnknownConsumedFluid { fluid } => write!(
                formatter,
                "survival consumption references unknown fluid {}",
                fluid.value()
            ),
            Self::ZeroConsumedFluidVolume { fluid } => write!(
                formatter,
                "survival consumption stores zero volume for fluid {}",
                fluid.value()
            ),
            Self::PendingConsumptionWithoutPlayer => {
                formatter.write_str("pending direct consumption exists without a player")
            }
            Self::PendingConsumptionScheduleInvalid => {
                formatter.write_str("pending direct consumption has an invalid active schedule")
            }
            Self::PendingEatingEmpty => {
                formatter.write_str("pending eating contains no consumed food traces")
            }
            Self::PendingEatingMassOverflow => {
                formatter.write_str("pending eating consumed mass overflowed")
            }
            Self::PendingEatingMassExceedsIntakeLimit => {
                formatter.write_str("pending eating mass exceeds the authored direct meal limit")
            }
            Self::PendingEatingTraceInvalid => {
                formatter.write_str("pending eating contains an invalid consumed food trace")
            }
            Self::PendingEatingAccountingMismatch { material } => write!(
                formatter,
                "pending eating owns more material {} than survival consumed-matter accounting",
                material.value()
            ),
            Self::PendingDrinkingVolumeInvalid => {
                formatter.write_str("pending drinking volume or authored duration is invalid")
            }
            Self::PendingDrinkingUnknownFluid { fluid } => write!(
                formatter,
                "pending drinking references unknown fluid {}",
                fluid.value()
            ),
            Self::PendingDrinkingNotDrinkable { fluid } => write!(
                formatter,
                "pending drinking references unauthored drink fluid {}",
                fluid.value()
            ),
            Self::PendingDrinkingTemperatureInvalid => formatter
                .write_str("pending drinking temperature is outside the authored intake range"),
            Self::PendingDrinkingAccountingMismatch { fluid } => write!(
                formatter,
                "pending drinking owns more fluid {} than survival consumed-volume accounting",
                fluid.value()
            ),
        }
    }
}

impl Error for SurvivalValidationError {}

fn validate_player_reserves(
    registry: &SurvivalRegistry,
    player: &PlayerSurvivalRecord,
) -> Result<(), SurvivalValidationError> {
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
    let recovery_remainder = player.vitality_recovery_remainder();
    if recovery_remainder >= NUTRITION_PARTS_PER_MILLION {
        return Err(
            SurvivalValidationError::VitalityRecoveryRemainderOutOfRange {
                value: recovery_remainder,
            },
        );
    }
    if player.vitality() == Vitality::MAXIMUM && recovery_remainder != 0 {
        return Err(
            SurvivalValidationError::VitalityRecoveryRemainderAtMaximum {
                value: recovery_remainder,
            },
        );
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
    Ok(())
}

fn validate_consumed_matter(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    state: &SurvivalState,
) -> Result<(), SurvivalValidationError> {
    for (material, mass) in state.consumed_matter() {
        if materials.get_material(material).is_none() || !registry.has_food_material(material) {
            return Err(SurvivalValidationError::UnknownConsumedMaterial { material });
        }
        if mass == crate::core::quantity::AggregateMass::ZERO {
            return Err(SurvivalValidationError::ZeroConsumedMass { material });
        }
    }
    Ok(())
}

fn validate_consumed_fluids(
    registry: &SurvivalRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
) -> Result<(), SurvivalValidationError> {
    for (fluid, volume) in state.consumed_fluids() {
        if fluids.get_fluid(fluid).is_none() || registry.get_drink(fluid).is_none() {
            return Err(SurvivalValidationError::UnknownConsumedFluid { fluid });
        }
        if volume == crate::core::quantity::AggregateVolume::ZERO {
            return Err(SurvivalValidationError::ZeroConsumedFluidVolume { fluid });
        }
    }
    Ok(())
}

pub(crate) fn validate_loaded_survival(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
    current: SimulationTick,
) -> Result<(), SurvivalValidationError> {
    let Some(player) = state.player() else {
        if state.consumed_matter().next().is_some() || state.consumed_fluids().next().is_some() {
            return Err(SurvivalValidationError::ConsumedMatterWithoutPlayer);
        }
        validate_pending_consumption(registry, materials, fluids, state, current)?;
        return Ok(());
    };
    validate_player_reserves(registry, player)?;
    validate_consumed_matter(registry, materials, state)?;
    validate_consumed_fluids(registry, fluids, state)?;
    validate_pending_consumption(registry, materials, fluids, state, current)
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
