//! Immutable survival definition lookup and reference validation.

use std::collections::{BTreeMap, BTreeSet};

use crate::fluid::{FluidDefinitionId, FluidRegistry};
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};

use super::intake::{DrinkDefinition, FoodDefinition};
use super::physiology::PhysiologyDefinition;

/// Immutable survival lookup bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurvivalRegistry {
    physiology: PhysiologyDefinition,
    foods: BTreeMap<CommodityKey, FoodDefinition>,
    food_materials: BTreeSet<MaterialId>,
    drinks: BTreeMap<FluidDefinitionId, DrinkDefinition>,
}

impl SurvivalRegistry {
    pub(crate) fn new(
        physiology: PhysiologyDefinition,
        foods: impl IntoIterator<Item = FoodDefinition>,
        drinks: impl IntoIterator<Item = DrinkDefinition>,
    ) -> Self {
        let mut foods_by_commodity = BTreeMap::new();
        let mut food_materials = BTreeSet::new();
        for food in foods {
            let commodity = food.commodity();
            assert!(
                foods_by_commodity.insert(commodity, food).is_none(),
                "duplicate food definition for commodity {}",
                commodity.value()
            );
            food_materials.insert(commodity.material());
        }
        let mut drinks_by_fluid = BTreeMap::new();
        for drink in drinks {
            assert!(
                drinks_by_fluid.insert(drink.fluid(), drink).is_none(),
                "duplicate drink definition for fluid {}",
                drink.fluid().value()
            );
        }
        Self {
            physiology,
            foods: foods_by_commodity,
            food_materials,
            drinks: drinks_by_fluid,
        }
    }

    #[must_use]
    pub const fn physiology(&self) -> PhysiologyDefinition {
        self.physiology
    }

    #[must_use]
    pub fn get_food(&self, commodity: CommodityKey) -> Option<&FoodDefinition> {
        self.foods.get(&commodity)
    }

    /// Iterates authored edible commodities in stable commodity-key order.
    pub fn foods(&self) -> impl Iterator<Item = &FoodDefinition> {
        self.foods.values()
    }

    #[must_use]
    pub fn get_drink(&self, fluid: FluidDefinitionId) -> Option<&DrinkDefinition> {
        self.drinks.get(&fluid)
    }

    /// Iterates authored drinkable fluids in stable definition-ID order.
    pub fn drinks(&self) -> impl Iterator<Item = &DrinkDefinition> {
        self.drinks.values()
    }

    #[must_use]
    pub(crate) fn has_food_material(&self, material: MaterialId) -> bool {
        self.food_materials.contains(&material)
    }

    pub(crate) fn validate_references(&self, materials: &MaterialRegistry, fluids: &FluidRegistry) {
        for food in self.foods.values() {
            assert!(
                materials.has_commodity(food.commodity()),
                "food definition references unknown commodity {}",
                food.commodity().value()
            );
        }
        for drink in self.drinks.values() {
            assert!(
                fluids.get_fluid(drink.fluid()).is_some(),
                "drink definition references unknown fluid {}",
                drink.fluid().value()
            );
        }
    }
}
