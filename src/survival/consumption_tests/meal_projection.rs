//! Read-only meal planning contracts including the cost of eating itself.

use super::*;

#[test]
fn minimum_meal_prices_the_eating_tick_instead_of_repeating_tiny_snacks() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let grain = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("grain food definition disappeared"));
    let target = Energy::from_nanojoules(physiology.maximum_metabolic_energy().nanojoules() / 2);
    let current = target
        .checked_sub(Energy::from_nanojoules(336_000_000_000))
        .unwrap_or_else(|| panic!("fixture target must exceed 336 J"));

    let projection = project_minimum_meal_to_metabolic_target(physiology, grain, current, target)
        .unwrap_or_else(|error| panic!("meal projection failed: {error}"))
        .unwrap_or_else(|| panic!("depleted fixture should require a meal"));

    assert_eq!(
        projection.mass(),
        physiology.direct_consumption().minimum_meal_mass()
    );
    assert_eq!(projection.duration(), TickSpan::new(1));
    assert_eq!(
        projection.energy_offered(),
        grain.dietary_energy_for_mass(physiology.direct_consumption().minimum_meal_mass())
    );
    assert!(projection.metabolic_energy_after() >= target);
}

#[test]
fn minimum_meal_clamps_large_valid_energy_without_intermediate_overflow() {
    use crate::core::quantity::Volume;
    use crate::survival::{
        DirectConsumptionDefinition, HydrationDefinition, MetabolismDefinition,
        NutritionDefinition, PhysiologyDefinition, SurvivalStressDefinition,
    };

    let registries = build_registries();
    let built = registries.survival().physiology();
    let grain = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap();
    let maximum = Energy::from_nanojoules(u128::MAX);
    let physiology = PhysiologyDefinition::new(
        MetabolismDefinition::new(maximum, Energy::ZERO, Energy::from_nanojoules(1)),
        HydrationDefinition::new(
            Volume::from_microliters(1),
            Volume::ZERO,
            Volume::from_microliters(1),
        ),
        NutritionDefinition::new(1, 1, Mass::from_milligrams(1)),
        DirectConsumptionDefinition::new(
            Mass::from_milligrams(1),
            Mass::from_milligrams(1),
            TickSpan::new(1),
            Volume::from_microliters(1),
            Volume::from_microliters(1),
            TickSpan::new(1),
        ),
        built.locomotion(),
        Mass::from_milligrams(1),
        SurvivalStressDefinition::new(1, 1, built.thermal_comfort()),
    );
    let current = Energy::from_nanojoules(u128::MAX - 3);
    let target = Energy::from_nanojoules(u128::MAX - 1);
    let projected = project_minimum_meal_to_metabolic_target(physiology, grain, current, target)
        .unwrap()
        .unwrap();
    assert_eq!(projected.mass(), Mass::from_milligrams(1));
    assert_eq!(projected.duration(), TickSpan::new(1));
    assert_eq!(projected.metabolic_energy_after(), maximum);
    assert!(projected.energy_offered() > Energy::from_nanojoules(3));
}

#[test]
fn minimum_meal_returns_none_when_target_is_already_met() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let grain = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("grain food definition disappeared"));
    let current = physiology.maximum_metabolic_energy();

    assert_eq!(
        project_minimum_meal_to_metabolic_target(physiology, grain, current, current),
        Ok(None)
    );
}
