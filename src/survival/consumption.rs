//! Canonical conserved food and drink consumption transactions.

mod drinking;
mod freshness;

pub use drinking::{DrinkCommitError, DrinkError, DrinkOutcome, ValidatedDrink, validate_drink};
pub use freshness::{FoodFreshness, FoodFreshnessError, assess_food_freshness};

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{AggregateMass, Energy, Mass, Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialEgressError, MaterialLotId, MaterialLotSelection,
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_explicit_consumption_selection, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::labor::{
    PlayerAttentionError, PlayerWork, ValidatedPlayerAttention, validate_player_attention,
};
use crate::material::{CommodityKey, MaterialId};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::FoodCategory;
use super::state::{PlayerSurvivalRecord, player_record};

/// Failure while validating one exact eating action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatError {
    SurvivalNotInitialized,
    PlayerDead,
    PlayerBusy {
        active: PlayerWork,
    },
    EmptySelection,
    DuplicateLot {
        lot: MaterialLotId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownLot {
        lot: MaterialLotId,
    },
    LotOwnedElsewhere {
        lot: MaterialLotId,
        requested_source: StockpileId,
        actual_source: StockpileId,
    },
    ZeroMass {
        lot: MaterialLotId,
    },
    InsufficientLotMass {
        lot: MaterialLotId,
        available: Mass,
        requested: Mass,
    },
    InventoryMassOverflow {
        stockpile: StockpileId,
    },
    NotEdible {
        commodity: CommodityKey,
    },
    TemperatureOutsideConsumptionRange {
        lot: MaterialLotId,
        temperature: Temperature,
        minimum: Temperature,
        maximum: Temperature,
    },
    Spoiled {
        lot: MaterialLotId,
        age: TickSpan,
    },
    ShelfLifeOverflow,
    MetabolicEnergyOverflow,
    HydrationOverflow,
    NutritionOverflow,
    NoReserveGain {
        mass: Mass,
    },
    UnsupportedComposition {
        lot: MaterialLotId,
        material: MaterialId,
    },
    ConsumedMatterOverflow {
        material: MaterialId,
    },
    InventoryRevisionExhausted,
    SurvivalRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("player survival is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot eat"),
            Self::PlayerBusy { active } => {
                write!(formatter, "player cannot eat while occupied by {active:?}")
            }
            Self::EmptySelection => formatter.write_str("eating selection must not be empty"),
            Self::DuplicateLot { lot } => write!(
                formatter,
                "food lot {} is selected more than once in one meal",
                lot.value()
            ),
            Self::UnknownStockpile { stockpile } => {
                write!(
                    formatter,
                    "unknown food source stockpile {}",
                    stockpile.value()
                )
            }
            Self::UnknownLot { lot } => write!(formatter, "unknown food lot {}", lot.value()),
            Self::LotOwnedElsewhere {
                lot,
                requested_source,
                actual_source,
            } => write!(
                formatter,
                "food lot {} is in stockpile {} rather than requested stockpile {}",
                lot.value(),
                actual_source.value(),
                requested_source.value()
            ),
            Self::ZeroMass { lot } => write!(
                formatter,
                "food lot {} selection has zero mass",
                lot.value()
            ),
            Self::InsufficientLotMass {
                lot,
                available,
                requested,
            } => write!(
                formatter,
                "food lot {} contains {} mg but {} mg was requested",
                lot.value(),
                available.milligrams(),
                requested.milligrams()
            ),
            Self::InventoryMassOverflow { stockpile } => write!(
                formatter,
                "food consumption mass accounting overflowed stockpile {}",
                stockpile.value()
            ),
            Self::NotEdible { commodity } => write!(
                formatter,
                "material {} form {} is not authored as edible",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::TemperatureOutsideConsumptionRange {
                lot,
                temperature,
                minimum,
                maximum,
            } => write!(
                formatter,
                "food lot {} at {} mK lies outside its direct-consumption range {}..={} mK",
                lot.value(),
                temperature.millikelvin(),
                minimum.millikelvin(),
                maximum.millikelvin()
            ),
            Self::Spoiled { lot, age } => write!(
                formatter,
                "food lot {} is spoiled after {} ticks",
                lot.value(),
                age.value()
            ),
            Self::ShelfLifeOverflow => {
                formatter.write_str("food shelf-life calculation overflowed")
            }
            Self::MetabolicEnergyOverflow => {
                formatter.write_str("food metabolic-energy calculation overflowed")
            }
            Self::HydrationOverflow => formatter.write_str("food hydration calculation overflowed"),
            Self::NutritionOverflow => formatter.write_str("food nutrition calculation overflowed"),
            Self::NoReserveGain { mass } => write!(
                formatter,
                "eating {} mg would not increase metabolic, hydration, or nutrition reserves",
                mass.milligrams()
            ),
            Self::UnsupportedComposition { lot, material } => write!(
                formatter,
                "food lot {} is not pure material {} and cannot enter the current survival-consumption boundary",
                lot.value(),
                material.value()
            ),
            Self::ConsumedMatterOverflow { material } => write!(
                formatter,
                "consumed food matter accounting overflowed material {}",
                material.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(formatter, "food withdrawal structural load failed: {error}")
            }
        }
    }
}

impl Error for EatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::PlayerBusy { active: _ }
            | Self::EmptySelection
            | Self::DuplicateLot { lot: _ }
            | Self::UnknownStockpile { stockpile: _ }
            | Self::UnknownLot { lot: _ }
            | Self::LotOwnedElsewhere { .. }
            | Self::ZeroMass { lot: _ }
            | Self::InsufficientLotMass { .. }
            | Self::InventoryMassOverflow { stockpile: _ }
            | Self::NotEdible { commodity: _ }
            | Self::TemperatureOutsideConsumptionRange { .. }
            | Self::Spoiled { .. }
            | Self::ShelfLifeOverflow
            | Self::MetabolicEnergyOverflow
            | Self::HydrationOverflow
            | Self::NutritionOverflow
            | Self::NoReserveGain { .. }
            | Self::UnsupportedComposition { .. }
            | Self::ConsumedMatterOverflow { material: _ }
            | Self::InventoryRevisionExhausted
            | Self::SurvivalRevisionExhausted => None,
        }
    }
}

/// Failure when a validated eating action is committed against changed owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatCommitError {
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EatCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "validated eating expected player-work revision {expected} but current revision is {actual}"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated eating expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated eating expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated eating structural commit failed: {error}"
            ),
        }
    }
}

impl Error for EatCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StalePlayerWorkRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::StaleInventoryRevision { .. } => None,
        }
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EatPortionOutcome {
    lot: MaterialLotId,
    mass: Mass,
    category: FoodCategory,
}

impl EatPortionOutcome {
    #[must_use]
    pub const fn lot(&self) -> MaterialLotId {
        self.lot
    }
    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }
    #[must_use]
    pub const fn category(&self) -> FoodCategory {
        self.category
    }
}

/// Nutrition credited by one eating action after metabolic absorption and reserve caps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NutritionGain {
    grain_ppm: u32,
    fruit_ppm: u32,
    protein_ppm: u32,
}

impl NutritionGain {
    #[must_use]
    pub const fn get(self, category: FoodCategory) -> u32 {
        match category {
            FoodCategory::Grain => self.grain_ppm,
            FoodCategory::Fruit => self.fruit_ppm,
            FoodCategory::Protein => self.protein_ppm,
        }
    }

    #[must_use]
    pub const fn total_ppm(self) -> u32 {
        self.grain_ppm + self.fruit_ppm + self.protein_ppm
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EatOutcome {
    portions: Vec<EatPortionOutcome>,
    total_mass: Mass,
    energy_gained: Energy,
    hydration_gained: Volume,
    nutrition_gained: NutritionGain,
}

impl EatOutcome {
    pub fn portions(&self) -> &[EatPortionOutcome] {
        &self.portions
    }
    #[must_use]
    pub const fn total_mass(&self) -> Mass {
        self.total_mass
    }
    #[must_use]
    pub const fn energy_gained(&self) -> Energy {
        self.energy_gained
    }
    #[must_use]
    pub const fn hydration_gained(&self) -> Volume {
        self.hydration_gained
    }
    #[must_use]
    pub const fn nutrition_gained(&self) -> NutritionGain {
        self.nutrition_gained
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEat {
    attention: ValidatedPlayerAttention,
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedMaterialEgress,
    structural: Option<ValidatedStockpileStructuralLoad>,
    after: PlayerSurvivalRecord,
    next_consumed_masses: Vec<(MaterialId, AggregateMass)>,
    outcome: EatOutcome,
}

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
    let reserve_maximum = u128::from(super::NUTRITION_PARTS_PER_MILLION);
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
        u128::from(super::NUTRITION_PARTS_PER_MILLION),
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
struct MealOffer {
    portions: Vec<EatPortionOutcome>,
    offered_energy: Energy,
    offered_hydration_ul: u128,
    category_energy: NutritionEnergy,
    consumed_additions: BTreeMap<MaterialId, AggregateMass>,
}

fn map_eat_selection_error(error: ExplicitConsumptionSelectionError) -> EatError {
    match error {
        ExplicitConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EatError::UnknownStockpile { stockpile }
        }
        ExplicitConsumptionSelectionError::EmptySelection => EatError::EmptySelection,
        ExplicitConsumptionSelectionError::ZeroMass { lot } => EatError::ZeroMass { lot },
        ExplicitConsumptionSelectionError::DuplicateLot { lot } => EatError::DuplicateLot { lot },
        ExplicitConsumptionSelectionError::UnknownLot { lot } => EatError::UnknownLot { lot },
        ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot,
            requested_source,
            actual_source,
        } => EatError::LotOwnedElsewhere {
            lot,
            requested_source,
            actual_source,
        },
        ExplicitConsumptionSelectionError::InsufficientLotMass {
            lot,
            available,
            requested,
        } => EatError::InsufficientLotMass {
            lot,
            available,
            requested,
        },
        ExplicitConsumptionSelectionError::MassOverflow { stockpile } => {
            EatError::InventoryMassOverflow { stockpile }
        }
    }
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

fn resolve_meal_offer(
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

fn resolve_energy_gain(
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

fn resolve_hydration_and_nutrition(
    player: PlayerSurvivalRecord,
    maximum_hydration: Volume,
    maximum_metabolic_energy: Energy,
    offer: &MealOffer,
) -> Result<(Volume, Volume, super::NutritionReserves, NutritionGain), EatError> {
    let hydration_gain_ul =
        u64::try_from(offer.offered_hydration_ul).map_err(|_| EatError::HydrationOverflow)?;
    let available_hydration = maximum_hydration
        .checked_sub(player.hydration())
        .ok_or(EatError::HydrationOverflow)?;
    let hydration_gained = Volume::from_microliters(hydration_gain_ul).min(available_hydration);
    let hydration_after = player
        .hydration()
        .checked_add(hydration_gained)
        .ok_or(EatError::HydrationOverflow)?;
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

fn resolve_consumed_mass_totals(
    state: &AppState,
    additions: BTreeMap<MaterialId, AggregateMass>,
) -> Result<Vec<(MaterialId, AggregateMass)>, EatError> {
    additions
        .into_iter()
        .map(|(material, addition)| {
            state
                .survival()
                .consumed_mass(material)
                .checked_add(addition)
                .map(|next| (material, next))
                .ok_or(EatError::ConsumedMatterOverflow { material })
        })
        .collect()
}

pub fn validate_eat(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedEat, EatError> {
    let attention = validate_player_attention(state).map_err(|error| match error {
        PlayerAttentionError::SurvivalNotInitialized => EatError::SurvivalNotInitialized,
        PlayerAttentionError::PlayerDead => EatError::PlayerDead,
        PlayerAttentionError::Busy { active } => EatError::PlayerBusy { active },
    })?;
    let Some(player) = state.survival().player().copied() else {
        return Err(EatError::SurvivalNotInitialized);
    };
    let exact_selection =
        validate_explicit_consumption_selection(state.inventory(), source, selections)
            .map_err(map_eat_selection_error)?;
    let offer = resolve_meal_offer(registries, state, selections)?;
    let physiology = registries.survival().physiology();
    let (energy_gained, energy_after) = resolve_energy_gain(
        player,
        physiology.maximum_metabolic_energy(),
        offer.offered_energy,
    )?;

    let egress = validate_material_egress_from_selection(state.inventory(), exact_selection)
        .map_err(|error| match error {
            MaterialEgressError::StaleSelection {
                expected: _,
                actual: _,
            } => unreachable!("synchronous eating selection cannot become stale before validation"),
            MaterialEgressError::RevisionExhausted => EatError::InventoryRevisionExhausted,
        })?;
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(EatError::UnknownStockpile { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(egress.total_consumed())
        .ok_or(EatError::InventoryMassOverflow { stockpile: source })?;
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(EatError::StructuralLoad)?;

    let (hydration_gained, hydration_after, nutrition_after, nutrition_gained) =
        resolve_hydration_and_nutrition(
            player,
            physiology.maximum_hydration(),
            physiology.maximum_metabolic_energy(),
            &offer,
        )?;
    let total_mass = egress.total_consumed();
    if energy_gained.is_zero() && hydration_gained.is_zero() && nutrition_gained.total_ppm() == 0 {
        return Err(EatError::NoReserveGain { mass: total_mass });
    }
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(EatError::SurvivalRevisionExhausted)?;
    let next_consumed_masses = resolve_consumed_mass_totals(state, offer.consumed_additions)?;

    Ok(ValidatedEat {
        attention,
        expected_survival_revision,
        next_survival_revision,
        egress,
        structural,
        after: player_record(
            energy_after,
            hydration_after,
            player.vitality(),
            nutrition_after,
            player.vitality_recovery_remainder(),
        ),
        next_consumed_masses,
        outcome: EatOutcome {
            portions: offer.portions,
            total_mass,
            energy_gained,
            hydration_gained,
            nutrition_gained,
        },
    })
}

impl ValidatedEat {
    pub fn commit(self, state: &mut AppState) -> Result<EatOutcome, EatCommitError> {
        if let Err(conflict) = self.attention.precheck(state) {
            return Err(EatCommitError::StalePlayerWorkRevision {
                expected: conflict.expected(),
                actual: conflict.actual(),
            });
        }
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(EatCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.egress.expected_revision() {
            return Err(EatCommitError::StaleInventoryRevision {
                expected: self.egress.expected_revision(),
                actual: actual_inventory_revision,
            });
        }
        if let Some(structural) = self.structural {
            structural
                .commit(state)
                .map_err(EatCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        state.survival_state_mut().apply_food_consumption(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.after,
            self.next_consumed_masses,
        );
        Ok(self.outcome)
    }
}

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;
