//! Unit tests for pure eating-resolution arithmetic.

use super::*;

#[test]
fn nutrition_allocation_handles_full_width_energy_without_intermediate_overflow() {
    let offered = NutritionEnergy {
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
fn nutrition_normalization_handles_full_width_energy_without_scaled_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX);
    assert_eq!(
        normalized_nutrition_gain_ppm(Energy::from_nanojoules(u128::MAX), maximum),
        Ok(u128::from(NUTRITION_PARTS_PER_MILLION))
    );
    assert_eq!(
        normalized_nutrition_gain_ppm(
            Energy::from_nanojoules(10_000_000_000_000_000),
            Energy::from_nanojoules(20_000_000_000_000_000),
        ),
        Ok(500_000_u128)
    );
}

#[test]
fn hydration_gain_clamps_before_narrowing_extreme_offer() {
    let current = Volume::from_microliters(99);
    let maximum = Volume::from_microliters(100);

    let (gained, after) = resolve_hydration_gain(current, maximum, u128::MAX)
        .unwrap_or_else(|error| panic!("bounded hydration gain failed: {error}"));

    assert_eq!(gained, Volume::from_microliters(1));
    assert_eq!(after, maximum);
}
