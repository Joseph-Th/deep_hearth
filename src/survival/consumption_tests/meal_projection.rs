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

    assert_eq!(projection.mass(), Mass::from_milligrams(48));
    assert_eq!(projection.duration(), TickSpan::new(1));
    assert_eq!(
        projection.energy_offered(),
        Energy::from_nanojoules(672_000_000_000)
    );
    assert!(projection.metabolic_energy_after() >= target);
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
