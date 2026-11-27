//! Exhaustive persistence validation for player survival quantities.

use crate::core::time::SimulationTick;
use crate::fluid::FluidRegistry;
use crate::material::MaterialRegistry;

use super::state::PlayerSurvivalRecord;
use super::{FoodCategory, NUTRITION_PARTS_PER_MILLION, SurvivalRegistry, SurvivalState, Vitality};

mod direct_consumption;
mod error;

use direct_consumption::validate_pending_consumption;
pub use error::SurvivalValidationError;

pub(crate) fn validate_player_reserves(
    physiology: super::PhysiologyDefinition,
    player: &PlayerSurvivalRecord,
) -> Result<(), SurvivalValidationError> {
    if player.metabolic_energy() > physiology.maximum_metabolic_energy() {
        return Err(SurvivalValidationError::EnergyExceedsMaximum);
    }
    if player.hydration() > physiology.maximum_hydration() {
        return Err(SurvivalValidationError::HydrationExceedsMaximum);
    }
    if player.vitality().parts_per_million() > Vitality::MAXIMUM.parts_per_million() {
        return Err(SurvivalValidationError::VitalityExceedsMaximum);
    }
    if player.thermal_comfort_ppm() > crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION {
        return Err(SurvivalValidationError::ThermalComfortExceedsMaximum);
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
) -> Result<u128, SurvivalValidationError> {
    let mut total = 0_u128;
    for (material, mass) in state.consumed_matter() {
        if materials.get_material(material).is_none() || !registry.has_food_material(material) {
            return Err(SurvivalValidationError::UnknownConsumedMaterial { material });
        }
        if mass == crate::core::quantity::AggregateMass::ZERO {
            return Err(SurvivalValidationError::ZeroConsumedMass { material });
        }
        total = total
            .checked_add(mass.milligrams())
            .ok_or(SurvivalValidationError::ConsumptionHistoryExceedsRevisionBudget)?;
    }
    Ok(total)
}

fn validate_consumed_fluids(
    registry: &SurvivalRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
) -> Result<u128, SurvivalValidationError> {
    let mut total = 0_u128;
    for (fluid, volume) in state.consumed_fluids() {
        if fluids.get_fluid(fluid).is_none() || registry.get_drink(fluid).is_none() {
            return Err(SurvivalValidationError::UnknownConsumedFluid { fluid });
        }
        if volume == crate::core::quantity::AggregateVolume::ZERO {
            return Err(SurvivalValidationError::ZeroConsumedFluidVolume { fluid });
        }
        total = total
            .checked_add(volume.microliters())
            .ok_or(SurvivalValidationError::ConsumptionHistoryExceedsRevisionBudget)?;
    }
    Ok(total)
}

fn validate_intake_history(
    registry: &SurvivalRegistry,
    materials: &MaterialRegistry,
    fluids: &FluidRegistry,
    state: &SurvivalState,
) -> Result<(), SurvivalValidationError> {
    let food = validate_consumed_matter(registry, materials, state)?;
    let drink = validate_consumed_fluids(registry, fluids, state)?;
    let direct = registry.physiology().direct_consumption();
    // Initialization and every intake admission consume distinct survival
    // revisions. Interrupted meals still contribute their entire debited
    // mass; use the authored maximum rather than assuming full absorption.
    let minimum_meals = food.div_ceil(u128::from(direct.maximum_meal_mass().milligrams()));
    let minimum_drinks = drink.div_ceil(u128::from(direct.maximum_drink_volume().microliters()));
    let available = u128::from(state.revision().saturating_sub(1));
    if available
        .checked_sub(minimum_meals)
        .is_none_or(|remaining| minimum_drinks > remaining)
    {
        return Err(SurvivalValidationError::ConsumptionHistoryExceedsRevisionBudget);
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
    validate_player_reserves(registry.physiology(), player)?;
    validate_intake_history(registry, materials, fluids, state)?;
    validate_pending_consumption(registry, materials, fluids, state, current)
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
