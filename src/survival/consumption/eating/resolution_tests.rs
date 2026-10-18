//! Unit tests for pure eating-resolution arithmetic.

use super::*;

#[test]
fn nutrition_allocation_handles_full_width_mass_without_intermediate_overflow() {
    let offered = NutritionMass {
        grain: u128::MAX - 2,
        fruit: 1,
        protein: 1,
    };

    let gain = allocate_nutrition(u128::from(NUTRITION_PARTS_PER_MILLION), offered);

    assert_eq!(gain.total_ppm(), NUTRITION_PARTS_PER_MILLION);
    assert_eq!(gain.get(FoodCategory::Grain), NUTRITION_PARTS_PER_MILLION);
    assert_eq!(gain.get(FoodCategory::Fruit), 0);
    assert_eq!(gain.get(FoodCategory::Protein), 0);
}

#[test]
fn zero_nutrition_target_allocates_nothing_even_with_offered_mass() {
    let offered = NutritionMass {
        grain: 3,
        fruit: 2,
        protein: 1,
    };

    assert_eq!(allocate_nutrition(0, offered), NutritionGain::default());
}

#[test]
fn nutrition_normalization_handles_full_width_mass_without_scaled_overflow() {
    let full = Mass::from_milligrams(500_000);
    assert_eq!(normalized_nutrition_gain_ppm(500_000, full), Ok(1_000_000));
    assert_eq!(normalized_nutrition_gain_ppm(250_000, full), Ok(500_000));
    assert_eq!(
        normalized_nutrition_gain_ppm(u128::from(u64::MAX), Mass::from_milligrams(1)),
        Ok(u128::from(u64::MAX) * u128::from(NUTRITION_PARTS_PER_MILLION)),
    );
}
