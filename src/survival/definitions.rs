//! Immutable survival, food, and drink definitions.

use std::collections::BTreeMap;

use crate::core::quantity::{Energy, MassSpecificEnergy, Temperature, Volume};
use crate::core::time::TickSpan;
use crate::fluid::{FluidDefinitionId, FluidRegistry};
use crate::material::{CommodityKey, MaterialId, MaterialRegistry};

/// Broad dietary identity used for planning and later nutrition systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FoodCategory {
    Grain,
    Fruit,
    Protein,
}

/// Inclusive authored temperature envelope for direct human consumption.
///
/// This is deliberately separate from storage compatibility. A container may physically tolerate
/// material that is too hot or too cold to consume safely without another thermal operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumptionTemperatureRange {
    minimum: Temperature,
    maximum: Temperature,
}

impl ConsumptionTemperatureRange {
    #[must_use]
    pub fn new(minimum: Temperature, maximum: Temperature) -> Self {
        assert!(
            minimum.millikelvin() != 0,
            "consumption minimum temperature must be above absolute zero"
        );
        assert!(
            minimum <= maximum,
            "consumption minimum temperature cannot exceed maximum temperature"
        );
        Self { minimum, maximum }
    }

    #[must_use]
    pub const fn minimum(self) -> Temperature {
        self.minimum
    }

    #[must_use]
    pub const fn maximum(self) -> Temperature {
        self.maximum
    }

    #[must_use]
    pub const fn contains(self, temperature: Temperature) -> bool {
        temperature.millikelvin() >= self.minimum.millikelvin()
            && temperature.millikelvin() <= self.maximum.millikelvin()
    }
}

/// Authored rate at which recent dietary balance fades and can support vitality recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NutritionDefinition {
    decay_ppm_per_tick: u32,
    vitality_recovery_ppm_per_tick: u32,
}

impl NutritionDefinition {
    #[must_use]
    pub fn new(decay_ppm_per_tick: u32, vitality_recovery_ppm_per_tick: u32) -> Self {
        assert!(decay_ppm_per_tick > 0, "nutrition decay must be nonzero");
        assert!(
            decay_ppm_per_tick <= 1_000_000,
            "nutrition decay cannot exceed the normalized reserve range"
        );
        assert!(
            vitality_recovery_ppm_per_tick > 0,
            "nutrition-supported vitality recovery must be nonzero"
        );
        assert!(
            vitality_recovery_ppm_per_tick <= 1_000_000,
            "nutrition-supported vitality recovery cannot exceed normalized vitality"
        );
        Self {
            decay_ppm_per_tick,
            vitality_recovery_ppm_per_tick,
        }
    }

    #[must_use]
    pub const fn decay_ppm_per_tick(self) -> u32 {
        self.decay_ppm_per_tick
    }

    #[must_use]
    pub const fn vitality_recovery_ppm_per_tick(self) -> u32 {
        self.vitality_recovery_ppm_per_tick
    }
}

/// Immutable physiology parameters for the player survival owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetabolismDefinition {
    maximum: Energy,
    hungry_below: Energy,
    basal_cost_per_tick: Energy,
}

impl MetabolismDefinition {
    #[must_use]
    pub fn new(maximum: Energy, hungry_below: Energy, basal_cost_per_tick: Energy) -> Self {
        assert!(
            !maximum.is_zero(),
            "maximum metabolic energy must be nonzero"
        );
        assert!(
            hungry_below < maximum,
            "hunger warning threshold must be below maximum metabolic energy"
        );
        assert!(
            !basal_cost_per_tick.is_zero(),
            "basal energy cost per tick must be nonzero"
        );
        assert!(
            basal_cost_per_tick <= maximum,
            "basal energy cost per tick cannot exceed maximum metabolic reserve"
        );
        Self {
            maximum,
            hungry_below,
            basal_cost_per_tick,
        }
    }
}

/// Immutable hydration capacity, warning threshold, and passive loss rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HydrationDefinition {
    maximum: Volume,
    thirsty_below: Volume,
    loss_per_tick: Volume,
}

impl HydrationDefinition {
    #[must_use]
    pub fn new(maximum: Volume, thirsty_below: Volume, loss_per_tick: Volume) -> Self {
        assert!(!maximum.is_zero(), "maximum hydration must be nonzero");
        assert!(
            thirsty_below < maximum,
            "thirst warning threshold must be below maximum hydration"
        );
        assert!(
            !loss_per_tick.is_zero(),
            "hydration loss per tick must be nonzero"
        );
        assert!(
            loss_per_tick <= maximum,
            "hydration loss per tick cannot exceed maximum hydration reserve"
        );
        Self {
            maximum,
            thirsty_below,
            loss_per_tick,
        }
    }
}

/// Immutable physiology parameters for the player survival owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysiologyDefinition {
    metabolism: MetabolismDefinition,
    hydration: HydrationDefinition,
    nutrition: NutritionDefinition,
    starvation_vitality_loss_ppm_per_tick: u32,
    dehydration_vitality_loss_ppm_per_tick: u32,
}

impl PhysiologyDefinition {
    #[must_use]
    pub fn new(
        metabolism: MetabolismDefinition,
        hydration: HydrationDefinition,
        nutrition: NutritionDefinition,
        starvation_vitality_loss_ppm_per_tick: u32,
        dehydration_vitality_loss_ppm_per_tick: u32,
    ) -> Self {
        assert!(
            starvation_vitality_loss_ppm_per_tick > 0,
            "starvation vitality loss must be nonzero"
        );
        assert!(
            starvation_vitality_loss_ppm_per_tick <= 1_000_000,
            "starvation vitality loss cannot exceed normalized vitality"
        );
        assert!(
            dehydration_vitality_loss_ppm_per_tick > 0,
            "dehydration vitality loss must be nonzero"
        );
        assert!(
            dehydration_vitality_loss_ppm_per_tick <= 1_000_000,
            "dehydration vitality loss cannot exceed normalized vitality"
        );
        Self {
            metabolism,
            hydration,
            nutrition,
            starvation_vitality_loss_ppm_per_tick,
            dehydration_vitality_loss_ppm_per_tick,
        }
    }

    #[must_use]
    pub const fn maximum_metabolic_energy(self) -> Energy {
        self.metabolism.maximum
    }

    #[must_use]
    pub const fn maximum_hydration(self) -> Volume {
        self.hydration.maximum
    }

    #[must_use]
    pub const fn hungry_below(self) -> Energy {
        self.metabolism.hungry_below
    }

    #[must_use]
    pub const fn thirsty_below(self) -> Volume {
        self.hydration.thirsty_below
    }

    #[must_use]
    pub const fn basal_energy_cost_per_tick(self) -> Energy {
        self.metabolism.basal_cost_per_tick
    }

    #[must_use]
    pub const fn hydration_loss_per_tick(self) -> Volume {
        self.hydration.loss_per_tick
    }

    #[must_use]
    pub const fn nutrition(self) -> NutritionDefinition {
        self.nutrition
    }

    #[must_use]
    pub const fn starvation_vitality_loss_ppm_per_tick(self) -> u32 {
        self.starvation_vitality_loss_ppm_per_tick
    }

    #[must_use]
    pub const fn dehydration_vitality_loss_ppm_per_tick(self) -> u32 {
        self.dehydration_vitality_loss_ppm_per_tick
    }
}

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;

/// Edibility and perishability of one exact material/form identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoodDefinition {
    commodity: CommodityKey,
    category: FoodCategory,
    dietary_energy: MassSpecificEnergy,
    hydration_microliters_per_milligram: u32,
    shelf_life: TickSpan,
    consumption_temperature: ConsumptionTemperatureRange,
}

impl FoodDefinition {
    #[must_use]
    pub fn new(
        commodity: CommodityKey,
        category: FoodCategory,
        dietary_energy: MassSpecificEnergy,
        hydration_microliters_per_milligram: u32,
        shelf_life: TickSpan,
        consumption_temperature: ConsumptionTemperatureRange,
    ) -> Self {
        assert!(
            !dietary_energy.is_zero(),
            "food dietary energy must be nonzero"
        );
        assert!(!shelf_life.is_zero(), "food shelf life must be nonzero");
        Self {
            commodity,
            category,
            dietary_energy,
            hydration_microliters_per_milligram,
            shelf_life,
            consumption_temperature,
        }
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn category(self) -> FoodCategory {
        self.category
    }

    #[must_use]
    pub const fn dietary_energy(self) -> MassSpecificEnergy {
        self.dietary_energy
    }

    #[must_use]
    pub const fn hydration_microliters_per_milligram(self) -> u32 {
        self.hydration_microliters_per_milligram
    }

    #[must_use]
    pub const fn shelf_life(self) -> TickSpan {
        self.shelf_life
    }

    #[must_use]
    pub const fn consumption_temperature(self) -> ConsumptionTemperatureRange {
        self.consumption_temperature
    }
}

/// Hydration contribution of one exact finite fluid identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrinkDefinition {
    fluid: FluidDefinitionId,
    hydration_multiplier_ppm: u32,
    consumption_temperature: ConsumptionTemperatureRange,
}

impl DrinkDefinition {
    #[must_use]
    pub fn new(
        fluid: FluidDefinitionId,
        hydration_multiplier_ppm: u32,
        consumption_temperature: ConsumptionTemperatureRange,
    ) -> Self {
        assert!(
            (1..=1_000_000).contains(&hydration_multiplier_ppm),
            "drink hydration multiplier must be inside 1..=1,000,000 ppm"
        );
        Self {
            fluid,
            hydration_multiplier_ppm,
            consumption_temperature,
        }
    }

    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn hydration_multiplier_ppm(self) -> u32 {
        self.hydration_multiplier_ppm
    }

    #[must_use]
    pub const fn consumption_temperature(self) -> ConsumptionTemperatureRange {
        self.consumption_temperature
    }
}

/// Immutable survival lookup bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurvivalRegistry {
    physiology: PhysiologyDefinition,
    foods: BTreeMap<CommodityKey, FoodDefinition>,
    drinks: BTreeMap<FluidDefinitionId, DrinkDefinition>,
}

impl SurvivalRegistry {
    pub(crate) fn new(
        physiology: PhysiologyDefinition,
        foods: impl IntoIterator<Item = FoodDefinition>,
        drinks: impl IntoIterator<Item = DrinkDefinition>,
    ) -> Self {
        let mut foods_by_commodity = BTreeMap::new();
        for food in foods {
            assert!(
                foods_by_commodity.insert(food.commodity(), food).is_none(),
                "duplicate food definition for commodity {}",
                food.commodity().value()
            );
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
        self.foods
            .values()
            .any(|definition| definition.commodity().material() == material)
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
