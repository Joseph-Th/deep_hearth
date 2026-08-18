//! Built-in physiology and primitive edible/drinkable content.

use crate::core::quantity::{Energy, MassSpecificEnergy, Volume};
use crate::core::time::TickSpan;
use crate::material::CommodityKey;
use crate::survival::{
    DrinkDefinition, FoodCategory, FoodDefinition, HydrationDefinition, MetabolismDefinition,
    NutritionDefinition, PhysiologyDefinition, SurvivalRegistry,
};

use super::{FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN, MATERIAL_MEAT};

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

fn foods() -> [FoodDefinition; 3] {
    [
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            FoodCategory::Grain,
            MassSpecificEnergy::from_nanojoules_per_milligram(14_000_000_000),
            0,
            TickSpan::new(24_000 * 32),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            FoodCategory::Fruit,
            MassSpecificEnergy::from_nanojoules_per_milligram(2_500_000_000),
            1,
            TickSpan::new(24_000 * 4),
        ),
        FoodDefinition::new(
            CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
            FoodCategory::Protein,
            MassSpecificEnergy::from_nanojoules_per_milligram(10_000_000_000),
            1,
            TickSpan::new(24_000 * 3),
        ),
    ]
}

pub(crate) fn build_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(
        physiology(),
        foods(),
        [DrinkDefinition::new(FLUID_WATER, 1_000_000)],
    )
}

#[cfg(test)]
pub(super) fn build_test_survival_registry() -> SurvivalRegistry {
    SurvivalRegistry::new(physiology(), foods(), std::iter::empty())
}
