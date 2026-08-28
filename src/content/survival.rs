//! Built-in physiology and primitive edible/drinkable content.

use crate::core::quantity::{Energy, MassSpecificEnergy, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::material::CommodityKey;
use crate::survival::{
    ConsumptionTemperatureRange, DrinkDefinition, FoodCategory, FoodDefinition,
    HydrationDefinition, MetabolismDefinition, NutritionDefinition, PhysiologyDefinition,
    SurvivalRegistry,
};

use super::{
    DEFAULT_TICKS_PER_DAY, FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN,
    MATERIAL_LEGUMES, MATERIAL_MEAT,
};

const MINIMUM_CONSUMPTION_TEMPERATURE_MK: u32 = 273_150;
const MAXIMUM_CONSUMPTION_TEMPERATURE_MK: u32 = 333_150;

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
        NutritionDefinition::new(5, 10),
        25,
        50,
    )
}

fn foods() -> [FoodDefinition; 4] {
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
            1,
            TickSpan::new(DEFAULT_TICKS_PER_DAY * 4),
            direct_consumption_temperature(),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(10_000_000_000),
            1,
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
    ]
}

pub(crate) fn build_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(
        physiology(),
        foods(),
        [DrinkDefinition::new(
            FLUID_WATER,
            1_000_000,
            direct_consumption_temperature(),
        )],
    )
}

#[cfg(test)]
pub(super) fn build_test_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(physiology(), foods(), std::iter::empty())
}
