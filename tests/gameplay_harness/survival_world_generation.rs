//! Deterministic survival provisioning world generation without live-state execution.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::inventory::{StockpileStorageProfile, StorageDefinitionId};
use deep_hearth::registry::Registries;
use deep_hearth::survival::{DrinkDefinition, FoodCategory, FoodDefinition};

use super::seed::mix64;
use super::survival_preservation_catalog::preservation_storage_candidates;
use super::survival_provisioning_policy::{
    DietProvisioningPolicy, category_salt, mass_for_target_energy, selected_food_indices,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SurvivalStartProfile {
    FullReserve,
    HungerWarningBoundary,
    HydrationWarningBoundary,
}

impl SurvivalStartProfile {
    #[cfg_attr(test, allow(dead_code, reason = "report-only start-profile label"))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::FullReserve => "full-reserve",
            Self::HungerWarningBoundary => "hunger-warning-boundary",
            Self::HydrationWarningBoundary => "hydration-warning-boundary",
        }
    }
}

pub(super) struct ProvisioningWorld {
    pub(super) start_profile: SurvivalStartProfile,
    pub(super) foods: Vec<FoodDefinition>,
    pub(super) offered_masses: Vec<Mass>,
    pub(super) witness_index: usize,
    pub(super) preserved_reserve_mass: Mass,
    pub(super) inherited_preservation_definition: StorageDefinitionId,
    pub(super) inherited_preservation_multiplier_ppm: u32,
    pub(super) age_ticks: u64,
    pub(super) provisioning_wait_ticks: u64,
    pub(super) drink: DrinkDefinition,
}

pub(super) fn minimum_visible_preservation_age_ticks(preservation_multiplier_ppm: u32) -> u64 {
    let ambient_preservation_ppm =
        u64::from(StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm());
    let preservation = u64::from(preservation_multiplier_ppm);
    assert!(
        preservation > ambient_preservation_ppm,
        "preservation witness requires a rate strictly better than ambient"
    );
    // Choose the first whole elapsed tick n for which even the conservative rounded preserved
    // age is at most n-1 ambient ticks:
    // n * ambient / preservation <= n - 1.
    preservation.div_ceil(preservation - ambient_preservation_ppm)
}

pub(super) fn provisioning_world(registries: &Registries, seed: u64) -> ProvisioningWorld {
    let physiology = registries.survival().physiology();
    let mut foods_by_category = BTreeMap::<FoodCategory, Vec<FoodDefinition>>::new();
    for food in registries.survival().foods().copied() {
        foods_by_category
            .entry(food.category())
            .or_default()
            .push(food);
    }
    for options in foods_by_category.values_mut() {
        options.sort_by_key(|food| food.commodity());
    }
    assert!(
        !foods_by_category.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored edible food"
    );
    let mut foods = foods_by_category
        .iter()
        .enumerate()
        .map(|(index, (category, options))| {
            let choice = usize::try_from(
                mix64(seed ^ category_salt(*category) ^ index as u64) % options.len() as u64,
            )
            .unwrap_or_else(|_| unreachable!("food option index fits usize"));
            options[choice]
        })
        .collect::<Vec<_>>();
    let available_count = if foods.len() <= 2 {
        foods.len()
    } else {
        2 + usize::try_from(mix64(seed ^ 0x4341_5445_474F_5259) % (foods.len() - 1) as u64)
            .unwrap_or_else(|_| unreachable!("bounded survival category count fits usize"))
    };
    let rotation = usize::try_from(mix64(seed ^ 0x464F_4F44_5F52_4F54) % foods.len() as u64)
        .unwrap_or_else(|_| unreachable!("survival food rotation fits usize"));
    foods.rotate_left(rotation);
    foods.truncate(available_count);
    foods.sort_by_key(|food| food.category());
    let start_profile = match mix64(seed ^ 0x5354_4152_5450_5246) % 3 {
        0 => SurvivalStartProfile::FullReserve,
        1 => SurvivalStartProfile::HungerWarningBoundary,
        _ => SurvivalStartProfile::HydrationWarningBoundary,
    };
    let compact_indices = selected_food_indices(&foods, DietProvisioningPolicy::CompactCalories);
    let balanced_indices = selected_food_indices(&foods, DietProvisioningPolicy::BalancedRecovery);
    let maximum_absorbed_energy = physiology.maximum_metabolic_energy().nanojoules();
    let compact_target = Energy::from_nanojoules(
        maximum_absorbed_energy
            .div_ceil(compact_indices.len() as u128)
            .max(1),
    );
    let balanced_target = Energy::from_nanojoules(
        maximum_absorbed_energy
            .div_ceil(balanced_indices.len() as u128)
            .max(1),
    );
    // Supply margin spans genuine scarcity through oversupply so organic worlds exercise both
    // tight provisioning and comfortable reserves. The band runs from 80% through 130%.
    let supply_margin_ppm = 800_000 + (mix64(seed ^ 0x5355_5050_4C59_4D47) % 500_001) as u32;
    let offered_masses = foods
        .iter()
        .enumerate()
        .map(|(index, food)| {
            let balanced = mass_for_target_energy(*food, balanced_target);
            let required = if compact_indices.contains(&index) {
                balanced.max(mass_for_target_energy(*food, compact_target))
            } else {
                balanced
            };
            let scaled = u128::from(required.milligrams())
                .checked_mul(u128::from(supply_margin_ppm))
                .map(|value| value.div_ceil(1_000_000))
                .unwrap_or_else(|| panic!("survival offered-food margin overflowed"));
            Mass::from_milligrams(
                u64::try_from(scaled)
                    .unwrap_or_else(|_| panic!("survival offered-food mass exceeds range")),
            )
        })
        .collect::<Vec<_>>();
    let preserving_storage = preservation_storage_candidates(registries);
    let preservation_capacities = preserving_storage
        .iter()
        .map(|candidate| candidate.capacity.milligrams())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let maximum_preservation_capacity = Mass::from_milligrams(
        *preservation_capacities
            .last()
            .unwrap_or_else(|| unreachable!("preservation candidates are nonempty")),
    );
    let witness_options = compact_indices
        .iter()
        .copied()
        .filter(|witness_index| offered_masses[*witness_index] <= maximum_preservation_capacity)
        .collect::<Vec<_>>();
    assert!(
        !witness_options.is_empty(),
        "no authored preservation enclosure can hold any generated compact-calorie reserve parcel"
    );
    let witness_option_index =
        usize::try_from(mix64(seed ^ 0x5052_4553_5749_544E) % witness_options.len() as u64)
            .unwrap_or_else(|_| {
                unreachable!("bounded preservation witness option index fits usize")
            });
    let witness_index = witness_options[witness_option_index];
    let witness_food = foods[witness_index];
    let minimum_reserve_mass = offered_masses[witness_index];
    // Reserve demand is a world need, not a property of whichever container happens to exist.
    // Four low-bit strata span the authored capacity frontier while high entropy varies within it.
    let requested_band = usize::try_from(seed & 0b11)
        .unwrap_or_else(|_| unreachable!("two-bit preservation stratum fits usize"));
    let requested_capacity_index = requested_band
        .checked_mul(preservation_capacities.len().saturating_sub(1))
        .map(|scaled| scaled / 3)
        .unwrap_or_else(|| unreachable!("bounded preservation capacity index cannot overflow"));
    let minimum_reserve_mg = minimum_reserve_mass.milligrams();
    let capacity_index = if preservation_capacities[requested_capacity_index] >= minimum_reserve_mg
    {
        requested_capacity_index
    } else {
        preservation_capacities
            .iter()
            .position(|capacity| *capacity >= minimum_reserve_mg)
            .unwrap_or_else(|| unreachable!("witness filtering guarantees a fitting enclosure"))
    };
    let reserve_upper_mg = preservation_capacities[capacity_index];
    let reserve_lower_mg = if capacity_index == 0 {
        minimum_reserve_mg
    } else {
        preservation_capacities[capacity_index - 1]
            .checked_add(1)
            .unwrap_or_else(|| panic!("preservation capacity-band lower bound overflowed"))
            .max(minimum_reserve_mg)
    };
    assert!(reserve_lower_mg <= reserve_upper_mg);
    let reserve_span = reserve_upper_mg - reserve_lower_mg;
    let reserve_offset = if reserve_span == 0 {
        0
    } else {
        mix64(seed ^ 0x5052_4553_5253_5256) % (reserve_span + 1)
    };
    let preserved_reserve_mass = Mass::from_milligrams(reserve_lower_mg + reserve_offset);
    let inherited_options = preserving_storage
        .iter()
        .filter(|candidate| candidate.capacity >= preserved_reserve_mass)
        .collect::<Vec<_>>();
    assert!(
        !inherited_options.is_empty(),
        "generated preserved reserve has no authored enclosure capacity"
    );
    let inherited_index =
        usize::try_from(mix64(seed ^ 0x494E_4845_5249_5445) % inherited_options.len() as u64)
            .unwrap_or_else(|_| unreachable!("bounded inherited-preservation index fits usize"));
    let inherited_preservation = inherited_options[inherited_index];
    let inherited_preservation_multiplier_ppm = inherited_preservation.preservation_multiplier_ppm;
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let provisioning_wait_ticks = match start_profile {
        SurvivalStartProfile::FullReserve => {
            // Rested worlds exercise proactive provisioning after a meaningful fraction of a day.
            let base = (ticks_per_day / 4).max(1);
            let jitter = (ticks_per_day / 4).max(1);
            base.checked_add(mix64(seed ^ 0x4441_5946_5241_4354) % jitter)
                .unwrap_or_else(|| panic!("survival probe provisioning wait overflowed"))
        }
        SurvivalStartProfile::HungerWarningBoundary
        | SurvivalStartProfile::HydrationWarningBoundary => {
            let base = (ticks_per_day / 24).max(1);
            base.checked_add(mix64(seed ^ 0x5052_4553_5355_5245) % base)
                .unwrap_or_else(|| panic!("survival pressure-world wait overflowed"))
        }
    };
    let minimum_age_ticks =
        minimum_visible_preservation_age_ticks(inherited_preservation_multiplier_ppm);
    let age_limit = (witness_food.shelf_life().value() / 4)
        .max(1)
        .min(provisioning_wait_ticks.saturating_sub(1).max(1));
    assert!(
        age_limit >= minimum_age_ticks,
        "survival preservation witness has no room for a visibly different preserved age: limit={age_limit}t minimum={minimum_age_ticks}t multiplier={inherited_preservation_multiplier_ppm}ppm"
    );
    let age_span = age_limit - minimum_age_ticks;
    let age_ticks =
        minimum_age_ticks + mix64(seed ^ 0x4147_455F_464F_4F44) % age_span.saturating_add(1);
    assert!(provisioning_wait_ticks > age_ticks);
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    assert!(
        !drinks.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored drinkable fluid"
    );
    let drink_index = usize::try_from(mix64(seed ^ 0x4452_494E_4B00_0001) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("drink index fits usize"));

    ProvisioningWorld {
        start_profile,
        foods,
        offered_masses,
        witness_index,
        preserved_reserve_mass,
        inherited_preservation_definition: inherited_preservation.definition,
        inherited_preservation_multiplier_ppm,
        age_ticks,
        provisioning_wait_ticks,
        drink: drinks[drink_index],
    }
}
