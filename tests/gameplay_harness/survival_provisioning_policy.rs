//! Pure survival provisioning policy shared by world generation and live evaluation.

use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::survival::{FoodCategory, FoodDefinition};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DietProvisioningPolicy {
    CompactCalories,
    BalancedRecovery,
}

impl DietProvisioningPolicy {
    #[cfg_attr(test, allow(dead_code, reason = "report-only policy label"))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::CompactCalories => "compact-calories",
            Self::BalancedRecovery => "balanced-recovery",
        }
    }
}

pub(super) fn diet_provisioning_policy_for_behavior_seed(
    behavior_seed: u64,
) -> DietProvisioningPolicy {
    // Focused behavior generation deliberately stratifies this low bit in exploratory samples while
    // leaving the physical world seed independent. Maintained/replay seeds remain exact and stable.
    if behavior_seed.is_multiple_of(2) {
        DietProvisioningPolicy::CompactCalories
    } else {
        DietProvisioningPolicy::BalancedRecovery
    }
}

pub(super) fn selected_food_indices(
    foods: &[FoodDefinition],
    policy: DietProvisioningPolicy,
) -> Vec<usize> {
    fn compact_category_rank(category: FoodCategory) -> u8 {
        // Explicit actor policy for otherwise equivalent calorie-density choices. Keeping this
        // exhaustive prevents enum declaration order from becoming an accidental tie-breaker.
        match category {
            FoodCategory::Grain => 0,
            FoodCategory::Fruit => 1,
            FoodCategory::Protein => 2,
        }
    }

    let mut indices = (0..foods.len()).collect::<Vec<_>>();
    match policy {
        DietProvisioningPolicy::BalancedRecovery => indices,
        DietProvisioningPolicy::CompactCalories => {
            indices.sort_by(|left, right| {
                foods[*right]
                    .dietary_energy()
                    .nanojoules_per_milligram()
                    .cmp(&foods[*left].dietary_energy().nanojoules_per_milligram())
                    .then_with(|| {
                        compact_category_rank(foods[*left].category())
                            .cmp(&compact_category_rank(foods[*right].category()))
                    })
            });
            indices.truncate(indices.len().min(2));
            indices
        }
    }
}

pub(super) fn mass_for_target_energy(food: FoodDefinition, target: Energy) -> Mass {
    food.minimum_mass_for_dietary_energy(target)
        .unwrap_or_else(|| panic!("survival probe meal mass exceeds authoritative range"))
}

pub(super) const fn category_salt(category: FoodCategory) -> u64 {
    match category {
        FoodCategory::Grain => 0x4752_4149_4E00_0001,
        FoodCategory::Fruit => 0x4652_5549_5400_0002,
        FoodCategory::Protein => 0x5052_4F54_4549_4E03,
    }
}
