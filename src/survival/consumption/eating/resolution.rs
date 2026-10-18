//! Pure meal-offer and physiological reserve resolution for exact eating transactions.

use std::collections::BTreeMap;

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{ConsumedMaterialTrace, ConsumptionSelection, MaterialLotSelection};
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::survival::{
    FoodCategory, FoodDefinition, NUTRITION_PARTS_PER_MILLION, calculate_food_hydration_offer,
};

use super::super::{FoodFreshness, FoodFreshnessError, assess_food_freshness};
use super::{EatError, EatPortionOutcome, NutritionGain};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NutritionMass {
    grain: u128,
    fruit: u128,
    protein: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EatingAbsorptionOffer {
    energy: Energy,
    hydration: AggregateVolume,
    nutrition: NutritionGain,
}

impl EatingAbsorptionOffer {
    pub(crate) const fn energy(self) -> Energy {
        self.energy
    }

    pub(crate) const fn hydration(self) -> AggregateVolume {
        self.hydration
    }

    pub(crate) const fn nutrition(self) -> NutritionGain {
        self.nutrition
    }
}

impl NutritionMass {
    fn checked_add(&mut self, category: FoodCategory, mass_mg: u128) -> Option<()> {
        let target = match category {
            FoodCategory::Grain => &mut self.grain,
            FoodCategory::Fruit => &mut self.fruit,
            FoodCategory::Protein => &mut self.protein,
        };
        *target = target.checked_add(mass_mg)?;
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

fn resolve_nutrition_offer(
    category_mass: NutritionMass,
    full_reserve_food_mass: Mass,
) -> Result<NutritionGain, EatError> {
    let nutrition_gain_ppm =
        normalized_nutrition_gain_ppm(category_mass.total(), full_reserve_food_mass)?;
    Ok(allocate_nutrition(nutrition_gain_ppm, category_mass))
}

pub(super) fn meal_absorption_offer(
    offer: &MealOffer,
    full_reserve_food_mass: Mass,
) -> Result<EatingAbsorptionOffer, EatError> {
    Ok(EatingAbsorptionOffer {
        energy: offer.offered_energy,
        hydration: offer.offered_hydration,
        nutrition: resolve_nutrition_offer(offer.category_mass, full_reserve_food_mass)?,
    })
}

fn accumulate_offer_energy(
    offered_energy_nj: &mut u128,
    category_mass: &mut NutritionMass,
    mass_mg: u64,
    food: FoodDefinition,
) -> Result<(), EatError> {
    let energy_nj = food
        .dietary_energy_for_mass(crate::core::quantity::Mass::from_milligrams(mass_mg))
        .nanojoules();
    *offered_energy_nj = offered_energy_nj
        .checked_add(energy_nj)
        .ok_or(EatError::MetabolicEnergyOverflow)?;
    category_mass
        .checked_add(food.category(), u128::from(mass_mg))
        .ok_or(EatError::NutritionOverflow)?;
    Ok(())
}

pub(crate) fn trace_absorption_offer<'a>(
    registries: &Registries,
    traces: impl ExactSizeIterator<Item = &'a ConsumedMaterialTrace>,
) -> EatingAbsorptionOffer {
    let mut offered_energy_nj = 0_u128;
    let mut category_mass = NutritionMass::default();
    let mut hydration_portions = Vec::with_capacity(traces.len());
    for trace in traces {
        let commodity = trace.profile().commodity();
        let food = registries.survival().get_food(commodity).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: pending eating trace references non-food commodity {}",
                commodity.value()
            )
        });
        accumulate_offer_energy(
            &mut offered_energy_nj,
            &mut category_mass,
            trace.mass().milligrams(),
            *food,
        )
        .unwrap_or_else(|error| panic!("validated pending meal offer failed at runtime: {error}"));
        hydration_portions.push((*food, trace.mass()));
    }
    let hydration = calculate_food_hydration_offer(hydration_portions)
        .unwrap_or_else(|| panic!("validated pending meal hydration overflowed at runtime"));
    let energy = Energy::from_nanojoules(offered_energy_nj);
    let nutrition = resolve_nutrition_offer(
        category_mass,
        registries
            .survival()
            .physiology()
            .nutrition()
            .full_reserve_food_mass(),
    )
    .unwrap_or_else(|error| panic!("validated pending meal nutrition failed at runtime: {error}"));
    EatingAbsorptionOffer {
        energy,
        hydration: AggregateVolume::from_volume(hydration),
        nutrition,
    }
}

fn allocate_nutrition(total_ppm: u128, offered: NutritionMass) -> NutritionGain {
    let offered_total = offered.total();
    if offered_total == 0 {
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

fn normalized_nutrition_gain_ppm(
    offered_mass_mg: u128,
    full_reserve_food_mass: Mass,
) -> Result<u128, EatError> {
    debug_assert!(!full_reserve_food_mass.is_zero());
    let (gain, _) = checked_mul_div_with_remainder(
        offered_mass_mg,
        u128::from(NUTRITION_PARTS_PER_MILLION),
        u128::from(full_reserve_food_mass.milligrams()),
        0,
    )
    .ok_or(EatError::NutritionOverflow)?;
    Ok(gain)
}

#[derive(Debug)]
struct ResolvedFoodPortion {
    outcome: EatPortionOutcome,
    energy_nj: u128,
    food: FoodDefinition,
    material: MaterialId,
}

#[derive(Debug)]
pub(super) struct MealOffer {
    pub(super) portions: Vec<EatPortionOutcome>,
    pub(super) offered_energy: Energy,
    offered_hydration: AggregateVolume,
    category_mass: NutritionMass,
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
    let energy_nj = food.dietary_energy_for_mass(selection.mass()).nanojoules();
    Ok(ResolvedFoodPortion {
        outcome: EatPortionOutcome {
            lot: selection.lot(),
            mass: selection.mass(),
            category: food.category(),
        },
        energy_nj,
        food,
        material,
    })
}

pub(super) fn resolve_meal_offer(
    registries: &Registries,
    state: &AppState,
    selection: &ConsumptionSelection,
) -> Result<MealOffer, EatError> {
    let selections = selection.lot_selections();
    let mut offered_energy_nj = 0_u128;
    let mut category_mass = NutritionMass::default();
    let mut consumed_additions = BTreeMap::<MaterialId, AggregateMass>::new();
    let mut portions = Vec::with_capacity(selections.len());
    let mut hydration_portions = Vec::with_capacity(selections.len());

    for selection in selections {
        let resolved = resolve_food_portion(registries, state, selection)?;
        offered_energy_nj = offered_energy_nj
            .checked_add(resolved.energy_nj)
            .ok_or(EatError::MetabolicEnergyOverflow)?;
        category_mass
            .checked_add(
                resolved.outcome.category,
                u128::from(resolved.outcome.mass.milligrams()),
            )
            .ok_or(EatError::NutritionOverflow)?;
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
        hydration_portions.push((resolved.food, resolved.outcome.mass));
        portions.push(resolved.outcome);
    }
    let offered_hydration = calculate_food_hydration_offer(hydration_portions)
        .map(AggregateVolume::from_volume)
        .ok_or(EatError::HydrationOverflow)?;

    Ok(MealOffer {
        portions,
        offered_energy: Energy::from_nanojoules(offered_energy_nj),
        offered_hydration,
        category_mass,
        consumed_additions,
    })
}

#[cfg(test)]
#[path = "resolution_tests.rs"]
mod tests;
