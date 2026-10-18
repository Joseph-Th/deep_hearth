//! Built-in physiology and primitive edible/drinkable content.

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::quantity::{
    Acceleration, Energy, Length, Mass, MassSpecificEnergy, Speed, Temperature, Volume,
};
use crate::core::time::TickSpan;
use crate::material::CommodityKey;
use crate::survival::{
    BodyKinematicsDefinition, BodyStanceDefinition, BodyViewDefinition,
    ConsumptionTemperatureRange, DirectConsumptionDefinition, DrinkDefinition, FoodCategory,
    FoodDefinition, HydrationDefinition, LocomotionDefinition, LocomotionSpeedDefinition,
    MetabolismDefinition, NutritionDefinition, PhysiologyDefinition, SurvivalExertion,
    SurvivalRegistry, SurvivalStressDefinition, ThermalComfortDefinition,
};

use super::{
    DEFAULT_TICKS_PER_DAY, FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN,
    MATERIAL_LEGUMES, MATERIAL_MEAT, MATERIAL_WILD_NUTS,
};

const MINIMUM_CONSUMPTION_TEMPERATURE_MK: u32 = 273_150;
const MAXIMUM_CONSUMPTION_TEMPERATURE_MK: u32 = 333_150;
const MINIMUM_MEAL_MASS_MG: u64 = 10_000;
/// Largest ordinary direct food-use serving. Deeper deficits require another intentional meal
/// action instead of collapsing most of a day's food into one interaction.
const MAXIMUM_MEAL_MASS_MG: u64 = 500_000;
const MAXIMUM_MEAL_DURATION_TICKS: u64 = 50;
/// Smallest ordinary drink action: one meaningful cup-sized serving rather than threshold-sipping.
const MINIMUM_DRINK_VOLUME_UL: u64 = 250_000;
/// Largest ordinary direct drink: a large bottle-sized serving rather than a litre-scale recovery action.
const MAXIMUM_DRINK_VOLUME_UL: u64 = 750_000;
const MAXIMUM_DRINK_DURATION_TICKS: u64 = 18;
/// Unaided carried custody. Twenty-five kilograms is deliberately useful for camp-scale material
/// handling without pretending the player can backpack machine-scale stone and timber assemblies.
const PLAYER_CARRIED_MASS_CAPACITY_MG: u64 = 25_000_000;
const PLAYER_CROUCHING_HEIGHT_UM: u64 = 1_500_000;
const PLAYER_STANDING_EYE_HEIGHT_UM: u64 = 1_620_000;
const PLAYER_CROUCHING_EYE_HEIGHT_UM: u64 = 1_270_000;
const PLAYER_INTERACTION_REACH_UM: u64 = 4_500_000;
const PLAYER_CROUCHING_SPEED_NM_PER_SECOND: u64 = 1_300_000_000;
const PLAYER_WALKING_SPEED_NM_PER_SECOND: u64 = 4_300_000_000;
const PLAYER_SPRINTING_SPEED_NM_PER_SECOND: u64 = 5_600_000_000;
const PLAYER_HORIZONTAL_ACCELERATION_UM_PER_SECOND_SQUARED: u64 = 20_000_000;
const PLAYER_UNLOADED_JUMP_SPEED_NM_PER_SECOND: u64 = 5_000_000_000;
const PLAYER_FULLY_LOADED_JUMP_SPEED_NM_PER_SECOND: u64 = 4_600_000_000;
const PLAYER_SAFE_LANDING_SPEED_NM_PER_SECOND: u64 = 8_000_000_000;
const PLAYER_MAXIMUM_FALL_SPEED_NM_PER_SECOND: u64 = 55_000_000_000;

fn direct_consumption_temperature() -> ConsumptionTemperatureRange {
    ConsumptionTemperatureRange::new(
        Temperature::from_millikelvin(MINIMUM_CONSUMPTION_TEMPERATURE_MK),
        Temperature::from_millikelvin(MAXIMUM_CONSUMPTION_TEMPERATURE_MK),
    )
}

fn physiology() -> PhysiologyDefinition {
    PhysiologyDefinition::new(
        MetabolismDefinition::new(
            Energy::from_nanojoules(20_000_000_000_000_000),
            Energy::from_nanojoules(5_000_000_000_000_000),
            Energy::from_nanojoules(333_000_000_000),
        ),
        HydrationDefinition::new(
            Volume::from_microliters(4_000_000),
            Volume::from_microliters(1_000_000),
            Volume::from_microliters(125),
        ),
        // Recent diet is a roughly two-day reserve, not an eight-day immunity to food choice.
        // This makes a varied meal a relevant recovery decision during sustained field projects.
        NutritionDefinition::new(25, 10, Mass::from_milligrams(500_000)),
        DirectConsumptionDefinition::new(
            Mass::from_milligrams(MINIMUM_MEAL_MASS_MG),
            Mass::from_milligrams(MAXIMUM_MEAL_MASS_MG),
            TickSpan::new(MAXIMUM_MEAL_DURATION_TICKS),
            Volume::from_microliters(MINIMUM_DRINK_VOLUME_UL),
            Volume::from_microliters(MAXIMUM_DRINK_VOLUME_UL),
            TickSpan::new(MAXIMUM_DRINK_DURATION_TICKS),
        ),
        LocomotionDefinition::new(
            SurvivalExertion::new(
                Energy::from_nanojoules(600_000_000_000),
                Volume::from_microliters(150),
            ),
            SurvivalExertion::new(
                Energy::from_nanojoules(1_200_000_000_000),
                Volume::from_microliters(300),
            ),
            BodyKinematicsDefinition::new(
                Length::from_micrometers(600_000),
                BodyStanceDefinition::new(
                    Length::from_micrometers(1_800_000),
                    Length::from_micrometers(PLAYER_CROUCHING_HEIGHT_UM),
                ),
                BodyViewDefinition::new(
                    Length::from_micrometers(PLAYER_STANDING_EYE_HEIGHT_UM),
                    Length::from_micrometers(PLAYER_CROUCHING_EYE_HEIGHT_UM),
                    Length::from_micrometers(PLAYER_INTERACTION_REACH_UM),
                ),
                LocomotionSpeedDefinition::new(
                    Speed::from_nanometers_per_second(PLAYER_CROUCHING_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_WALKING_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_SPRINTING_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_UNLOADED_JUMP_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_FULLY_LOADED_JUMP_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_SAFE_LANDING_SPEED_NM_PER_SECOND),
                    Speed::from_nanometers_per_second(PLAYER_MAXIMUM_FALL_SPEED_NM_PER_SECOND),
                ),
                Acceleration::from_micrometers_per_second_squared(
                    PLAYER_HORIZONTAL_ACCELERATION_UM_PER_SECOND_SQUARED,
                ),
            ),
        ),
        Mass::from_milligrams(PLAYER_CARRIED_MASS_CAPACITY_MG),
        SurvivalStressDefinition::new(
            25,
            50,
            ThermalComfortDefinition::new(
                800_000,
                1_000,
                500,
                20_000,
                Energy::from_nanojoules(10_800_000_000_000),
                25,
            ),
        ),
    )
}

fn foods() -> [FoodDefinition; 5] {
    [
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            FoodCategory::Grain,
            MassSpecificEnergy::from_nanojoules_per_milligram(14_000_000_000),
            0,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 32),
            direct_consumption_temperature(),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            FoodCategory::Fruit,
            MassSpecificEnergy::from_nanojoules_per_milligram(2_500_000_000),
            850_000,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 4),
            direct_consumption_temperature(),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(10_000_000_000),
            650_000,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 3),
            direct_consumption_temperature(),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD),
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(8_000_000_000),
            0,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 24),
            direct_consumption_temperature(),
        ),
        // A dense but finite wild protein source. Nuts supply no hydration; berries still
        // have a place when the player needs water as well as calories.
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(7_000_000_000),
            0,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 10),
            direct_consumption_temperature(),
        ),
    ]
}

pub(crate) fn build_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(
        physiology(),
        foods(),
        [DrinkDefinition::new(
            FLUID_WATER,
            NORMALIZED_PARTS_PER_MILLION,
            direct_consumption_temperature(),
        )],
    )
}

#[cfg(test)]
pub(super) fn build_test_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(physiology(), foods(), std::iter::empty())
}
