//! Persistent player survival values and nutrition state.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Volume};

use crate::survival::FoodCategory;

use super::NUTRITION_PARTS_PER_MILLION;

/// Player vitality in normalized parts per million.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Vitality(u32);

impl Vitality {
    pub const MAXIMUM: Self = Self(crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION);
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.0
    }

    pub(crate) const fn from_parts_per_million_unchecked(value: u32) -> Self {
        Self(value)
    }
}

/// Persistent recent dietary contribution by broad food category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NutritionReserves {
    grain: u32,
    fruit: u32,
    protein: u32,
}

impl NutritionReserves {
    pub const FULL: Self = Self {
        grain: NUTRITION_PARTS_PER_MILLION,
        fruit: NUTRITION_PARTS_PER_MILLION,
        protein: NUTRITION_PARTS_PER_MILLION,
    };

    #[must_use]
    pub const fn get(self, category: FoodCategory) -> u32 {
        match category {
            FoodCategory::Grain => self.grain,
            FoodCategory::Fruit => self.fruit,
            FoodCategory::Protein => self.protein,
        }
    }

    #[must_use]
    pub const fn quality_ppm(self) -> u32 {
        // Vitality recovery is limited by the weakest broad dietary contribution rather than by
        // total calories averaged across categories. A player can therefore live on an unbalanced
        // diet without taking arbitrary direct damage, but recovering well requires maintaining all
        // represented categories instead of repeatedly overfilling one reserve.
        let grain_or_fruit = if self.grain < self.fruit {
            self.grain
        } else {
            self.fruit
        };
        if grain_or_fruit < self.protein {
            grain_or_fruit
        } else {
            self.protein
        }
    }

    pub(crate) fn add(self, category: FoodCategory, gain: u32) -> (Self, u32) {
        let before = self.get(category);
        let after = before.saturating_add(gain).min(NUTRITION_PARTS_PER_MILLION);
        let mut next = self;
        match category {
            FoodCategory::Grain => next.grain = after,
            FoodCategory::Fruit => next.fruit = after,
            FoodCategory::Protein => next.protein = after,
        }
        (next, after - before)
    }

    pub(crate) fn decay(self, amount: u32) -> Self {
        Self {
            grain: self.grain.saturating_sub(amount),
            fruit: self.fruit.saturating_sub(amount),
            protein: self.protein.saturating_sub(amount),
        }
    }

    pub(crate) const fn has_valid_bounds(self) -> bool {
        self.grain <= NUTRITION_PARTS_PER_MILLION
            && self.fruit <= NUTRITION_PARTS_PER_MILLION
            && self.protein <= NUTRITION_PARTS_PER_MILLION
    }

    #[cfg(test)]
    pub(in crate::survival) const fn from_parts_per_million(
        grain: u32,
        fruit: u32,
        protein: u32,
    ) -> Self {
        Self {
            grain,
            fruit,
            protein,
        }
    }
}

/// Persistent survival quantities for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSurvivalRecord {
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    nutrition: NutritionReserves,
    vitality_recovery_remainder: u32,
}

impl PlayerSurvivalRecord {
    #[must_use]
    pub const fn metabolic_energy(self) -> Energy {
        self.metabolic_energy
    }

    #[must_use]
    pub const fn hydration(self) -> Volume {
        self.hydration
    }

    #[must_use]
    pub const fn vitality(self) -> Vitality {
        self.vitality
    }

    #[must_use]
    pub const fn nutrition(self) -> NutritionReserves {
        self.nutrition
    }

    #[must_use]
    pub(crate) const fn vitality_recovery_remainder(self) -> u32 {
        self.vitality_recovery_remainder
    }
}

pub(crate) const fn player_record(
    metabolic_energy: Energy,
    hydration: Volume,
    vitality: Vitality,
    nutrition: NutritionReserves,
    vitality_recovery_remainder: u32,
) -> PlayerSurvivalRecord {
    PlayerSurvivalRecord {
        metabolic_energy,
        hydration,
        vitality,
        nutrition,
        vitality_recovery_remainder,
    }
}
