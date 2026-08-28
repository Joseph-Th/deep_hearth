//! Pure meal-offer and physiological reserve resolution for exact eating transactions.

use std::collections::BTreeMap;

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{AggregateMass, Energy, Volume};
use crate::core::state::AppState;
use crate::inventory::MaterialLotSelection;
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::survival::{FoodCategory, NUTRITION_PARTS_PER_MILLION, NutritionReserves};

use super::super::{FoodFreshness, FoodFreshnessError, assess_food_freshness};
use super::{EatError, EatPortionOutcome, NutritionGain, PlayerSurvivalRecord};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NutritionEnergy {
    grain: u128,
    fruit: u128,
    protein: u128,
}

impl NutritionEnergy {
    fn checked_add(&mut self, category: FoodCategory, energy: u128) -> Option<()> {
        let target = match category {
            FoodCategory::Grain => &mut self.grain,
            FoodCategory::Fruit => &mut self.fruit,
            FoodCategory::Protein => &mut self.protein,
        };
        *target = target.checked_add(energy)?;
        Some(())
    }

    const fn get(self, category: FoodCategory) -> u128 {
        match category {
            FoodCategory::Grain => self.grain,
            FoodCategory::Fruit => self.fruit,
            FoodCategory::Protein => self.protein,
        }
    }

    const fn total(self) -> u128 {
        self.grain + self.fruit + self.protein
    }
}

fn allocate_nutrition(total_ppm: u128, offered: NutritionEnergy) -> NutritionGain {
    let offered_total = offered.total();
    if total_ppm == 0 || offered_total == 0 {
        return NutritionGain::default();
    }
    let categories = [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ];
    let mut allocated = [0_u128; 3];
    let mut remainders = [(0_u128, 0_usize); 3];
    let mut allocated_total = 0_u128;
    for (index, category) in categories.into_iter().enumerate() {
        let (share, remainder) =
            checked_mul_div_with_remainder(offered.get(category), total_ppm, offered_total, 0)
                .unwrap_or_else(|| panic!("bounded nutrition allocation overflowed"));
        allocated[index] = share;
        remainders[index] = (remainder, index);
        allocated_total += allocated[index];
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let remainder_units = usize::try_from(total_ppm - allocated_total)
        .unwrap_or_else(|_| panic!("three-category nutrition remainder exceeded usize"));
    for (_, index) in remainders.into_iter().take(remainder_units) {
        allocated[index] += 1;
    }
    let reserve_maximum = u128::from(NUTRITION_PARTS_PER_MILLION);
    let bounded = allocated.map(|gain| {
        u32::try_from(gain.min(reserve_maximum))
            .unwrap_or_else(|_| unreachable!("bounded nutrition gain always fits u32"))
    });
    NutritionGain {
        grain_ppm: bounded[0],
        fruit_ppm: bounded[1],
        protein_ppm: bounded[2],
    }
}

fn normalized_nutrition_gain_ppm(offered: Energy, maximum: Energy) -> Result<u128, EatError> {
    debug_assert!(!maximum.is_zero());
    let (gain, _) = checked_mul_div_with_remainder(
        offered.nanojoules(),
        u128::from(NUTRITION_PARTS_PER_MILLION),
        maximum.nanojoules(),
        0,
    )
    .ok_or(EatError::NutritionOverflow)?;
    Ok(gain)
}

#[derive(Debug)]
struct ResolvedFoodPortion {
    outcome: EatPortionOutcome,
    energy_nj: u128,
    hydration_ul: u128,
    material: MaterialId,
}

#[derive(Debug)]
pub(super) struct MealOffer {
    pub(super) portions: Vec<EatPortionOutcome>,
    pub(super) offered_energy: Energy,
    offered_hydration_ul: u128,
    category_energy: NutritionEnergy,
    pub(super) consumed_additions: BTreeMap<MaterialId, AggregateMass>,
}

fn map_food_freshness_error(error: FoodFreshnessError) -> EatError {
    match error {
        FoodFreshnessError::UnknownLot { lot } => EatError::UnknownLot { lot },
        FoodFreshnessError::UnknownStockpile { stockpile } => {
            EatError::UnknownStockpile { stockpile }
        }
        FoodFreshnessError::NotEdible { commodity } => EatError::NotEdible { commodity },
        FoodFreshnessError::ShelfLifeOverflow => EatError::ShelfLifeOverflow,
    }
}

fn resolve_food_portion(
    registries: &Registries,
    state: &AppState,
    selection: MaterialLotSelection,
) -> Result<ResolvedFoodPortion, EatError> {
    let lot = state
        .inventory()
        .get_lot(selection.lot())
        .ok_or(EatError::UnknownLot {
            lot: selection.lot(),
        })?;
    let food = registries
        .survival()
        .get_food(lot.commodity())
        .copied()
        .ok_or(EatError::NotEdible {
            commodity: lot.commodity(),
        })?;
    let consumption_temperature = food.consumption_temperature();
    if !consumption_temperature.contains(lot.temperature()) {
        return Err(EatError::TemperatureOutsideConsumptionRange {
            lot: selection.lot(),
            temperature: lot.temperature(),
            minimum: consumption_temperature.minimum(),
            maximum: consumption_temperature.maximum(),
        });
    }
    let material = food.commodity().material();
    if lot.composition().pure_material() != Some(material) {
        return Err(EatError::UnsupportedComposition {
            lot: selection.lot(),
            material,
        });
    }
    if let FoodFreshness::Spoiled { age } =
        assess_food_freshness(registries, state, selection.lot())
            .map_err(map_food_freshness_error)?
    {
        return Err(EatError::Spoiled {
            lot: selection.lot(),
            age,
        });
    }
    let energy_nj = u128::from(selection.mass().milligrams())
        .checked_mul(u128::from(food.dietary_energy().nanojoules_per_milligram()))
        .ok_or(EatError::MetabolicEnergyOverflow)?;
    let hydration_ul = u128::from(selection.mass().milligrams())
        .checked_mul(u128::from(food.hydration_microliters_per_milligram()))
        .ok_or(EatError::HydrationOverflow)?;
    Ok(ResolvedFoodPortion {
        outcome: EatPortionOutcome {
            lot: selection.lot(),
            mass: selection.mass(),
            category: food.category(),
        },
        energy_nj,
        hydration_ul,
        material,
    })
}

pub(super) fn resolve_meal_offer(
    registries: &Registries,
    state: &AppState,
    selections: &[MaterialLotSelection],
) -> Result<MealOffer, EatError> {
    let mut ordered = selections.to_vec();
    ordered.sort_unstable();
    let mut offered_energy_nj = 0_u128;
    let mut offered_hydration_ul = 0_u128;
    let mut category_energy = NutritionEnergy::default();
    let mut consumed_additions = BTreeMap::<MaterialId, AggregateMass>::new();
    let mut portions = Vec::with_capacity(ordered.len());

    for selection in ordered {
        let resolved = resolve_food_portion(registries, state, selection)?;
        offered_energy_nj = offered_energy_nj
            .checked_add(resolved.energy_nj)
            .ok_or(EatError::MetabolicEnergyOverflow)?;
        category_energy
            .checked_add(resolved.outcome.category, resolved.energy_nj)
            .ok_or(EatError::NutritionOverflow)?;
        offered_hydration_ul = offered_hydration_ul
            .checked_add(resolved.hydration_ul)
            .ok_or(EatError::HydrationOverflow)?;
        let current = consumed_additions
            .get(&resolved.material)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        consumed_additions.insert(
            resolved.material,
            current
                .checked_add(AggregateMass::from_mass(resolved.outcome.mass))
                .ok_or(EatError::ConsumedMatterOverflow {
                    material: resolved.material,
                })?,
        );
        portions.push(resolved.outcome);
    }

    Ok(MealOffer {
        portions,
        offered_energy: Energy::from_nanojoules(offered_energy_nj),
        offered_hydration_ul,
        category_energy,
        consumed_additions,
    })
}

pub(super) fn resolve_energy_gain(
    player: PlayerSurvivalRecord,
    maximum: Energy,
    offered: Energy,
) -> Result<(Energy, Energy), EatError> {
    let available = maximum
        .checked_sub(player.metabolic_energy())
        .ok_or(EatError::MetabolicEnergyOverflow)?;
    let gained = offered.min(available);
    let after = player
        .metabolic_energy()
        .checked_add(gained)
        .ok_or(EatError::MetabolicEnergyOverflow)?;
    Ok((gained, after))
}

fn resolve_hydration_gain(
    current: Volume,
    maximum: Volume,
    offered_microliters: u128,
) -> Result<(Volume, Volume), EatError> {
    let available = maximum
        .checked_sub(current)
        .ok_or(EatError::HydrationOverflow)?;
    let gained_microliters = offered_microliters.min(u128::from(available.microliters()));
    let gained_microliters = u64::try_from(gained_microliters)
        .unwrap_or_else(|_| unreachable!("hydration gain is bounded by u64-backed capacity"));
    let gained = Volume::from_microliters(gained_microliters);
    let after = current
        .checked_add(gained)
        .ok_or(EatError::HydrationOverflow)?;
    Ok((gained, after))
}

pub(super) fn resolve_hydration_and_nutrition(
    player: PlayerSurvivalRecord,
    maximum_hydration: Volume,
    maximum_metabolic_energy: Energy,
    offer: &MealOffer,
) -> Result<(Volume, Volume, NutritionReserves, NutritionGain), EatError> {
    let (hydration_gained, hydration_after) = resolve_hydration_gain(
        player.hydration(),
        maximum_hydration,
        offer.offered_hydration_ul,
    )?;
    let nutrition_gain_ppm =
        normalized_nutrition_gain_ppm(offer.offered_energy, maximum_metabolic_energy)?;
    let allocated = allocate_nutrition(nutrition_gain_ppm, offer.category_energy);
    let (after_grain, grain_ppm) = player
        .nutrition()
        .add(FoodCategory::Grain, allocated.get(FoodCategory::Grain));
    let (after_fruit, fruit_ppm) =
        after_grain.add(FoodCategory::Fruit, allocated.get(FoodCategory::Fruit));
    let (nutrition_after, protein_ppm) =
        after_fruit.add(FoodCategory::Protein, allocated.get(FoodCategory::Protein));
    Ok((
        hydration_gained,
        hydration_after,
        nutrition_after,
        NutritionGain {
            grain_ppm,
            fruit_ppm,
            protein_ppm,
        },
    ))
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod tests;
