//! Canonical conserved food and drink consumption transactions.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::fluid::{
    FluidEgressCommitError, FluidEgressError, FluidStoreId, FluidStructuralLoadError,
    ValidatedFluidEgress, validate_fluid_egress,
};
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialEgressError, MaterialLotId, MaterialLotSelection,
    STORAGE_AGE_PARTS_PER_TICK, StockpileId, StockpileStoredMassChange,
    StockpileStructuralLoadError, ValidatedMaterialEgress, ValidatedStockpileStructuralLoad,
    apply_material_egress, validate_explicit_consumption_selection,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::material::{CommodityKey, MaterialComposition, MaterialId};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::{PlayerSurvivalRecord, player_record};
use super::{FoodCategory, Vitality};

/// Read-only perishability state for one food lot in its current storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodFreshness {
    Fresh { age: TickSpan, remaining: TickSpan },
    Spoiled { age: TickSpan },
}

/// Failure while projecting freshness for one lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodFreshnessError {
    UnknownLot { lot: MaterialLotId },
    UnknownStockpile { stockpile: StockpileId },
    NotEdible { commodity: CommodityKey },
    ShelfLifeOverflow,
}

/// Projects whether one exact food lot remains edible under its current storage profile.
pub fn assess_food_freshness(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
) -> Result<FoodFreshness, FoodFreshnessError> {
    let record = state
        .inventory()
        .get_lot(lot)
        .ok_or(FoodFreshnessError::UnknownLot { lot })?;
    let food = registries.survival().get_food(record.commodity()).ok_or(
        FoodFreshnessError::NotEdible {
            commodity: record.commodity(),
        },
    )?;
    let stockpile = state.inventory().get_stockpile(record.stockpile()).ok_or(
        FoodFreshnessError::UnknownStockpile {
            stockpile: record.stockpile(),
        },
    )?;
    let age_parts = record
        .storage_history()
        .project(
            state.tick(),
            stockpile.storage_profile().preservation_multiplier_ppm(),
        )
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
    let shelf_life_parts = u128::from(food.shelf_life().value())
        .checked_mul(STORAGE_AGE_PARTS_PER_TICK)
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
    let age_ticks = age_parts.div_ceil(STORAGE_AGE_PARTS_PER_TICK);
    let age =
        TickSpan::new(u64::try_from(age_ticks).map_err(|_| FoodFreshnessError::ShelfLifeOverflow)?);
    if age_parts >= shelf_life_parts {
        Ok(FoodFreshness::Spoiled { age })
    } else {
        let remaining_parts = shelf_life_parts - age_parts;
        let remaining_ticks = remaining_parts
            .checked_mul(u128::from(
                stockpile.storage_profile().preservation_multiplier_ppm(),
            ))
            .ok_or(FoodFreshnessError::ShelfLifeOverflow)?
            .div_ceil(STORAGE_AGE_PARTS_PER_TICK * STORAGE_AGE_PARTS_PER_TICK);
        Ok(FoodFreshness::Fresh {
            age,
            remaining: TickSpan::new(
                u64::try_from(remaining_ticks)
                    .map_err(|_| FoodFreshnessError::ShelfLifeOverflow)?,
            ),
        })
    }
}

/// Failure while validating one exact eating action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatError {
    SurvivalNotInitialized,
    PlayerDead,
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
    Spoiled {
        lot: MaterialLotId,
        age: TickSpan,
    },
    ShelfLifeOverflow,
    NutritionOverflow,
    UnsupportedComposition {
        lot: MaterialLotId,
        material: MaterialId,
    },
    MetabolicEnergyCapacityExceeded {
        available: Energy,
        requested: Energy,
    },
    MetabolicMatterOverflow {
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
            Self::Spoiled { lot, age } => write!(
                formatter,
                "food lot {} is spoiled after {} ticks",
                lot.value(),
                age.value()
            ),
            Self::ShelfLifeOverflow => {
                formatter.write_str("food shelf-life calculation overflowed")
            }
            Self::NutritionOverflow => formatter.write_str("food nutrition calculation overflowed"),
            Self::UnsupportedComposition { lot, material } => write!(
                formatter,
                "food lot {} is not pure material {} and cannot enter the current metabolism boundary",
                lot.value(),
                material.value()
            ),
            Self::MetabolicEnergyCapacityExceeded {
                available,
                requested,
            } => write!(
                formatter,
                "meal provides {} nJ but only {} nJ of metabolic-energy reserve capacity remains",
                requested.nanojoules(),
                available.nanojoules()
            ),
            Self::MetabolicMatterOverflow { material } => write!(
                formatter,
                "metabolic matter accounting overflowed material {}",
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
            | Self::EmptySelection
            | Self::DuplicateLot { lot: _ }
            | Self::UnknownStockpile { stockpile: _ }
            | Self::UnknownLot { lot: _ }
            | Self::LotOwnedElsewhere { .. }
            | Self::ZeroMass { lot: _ }
            | Self::InsufficientLotMass { .. }
            | Self::InventoryMassOverflow { stockpile: _ }
            | Self::NotEdible { commodity: _ }
            | Self::Spoiled { .. }
            | Self::ShelfLifeOverflow
            | Self::NutritionOverflow
            | Self::UnsupportedComposition { .. }
            | Self::MetabolicEnergyCapacityExceeded { .. }
            | Self::MetabolicMatterOverflow { material: _ }
            | Self::InventoryRevisionExhausted
            | Self::SurvivalRevisionExhausted => None,
        }
    }
}

/// Failure when a validated eating action is committed against changed owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for EatCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::StaleSurvivalRevision { .. } | Self::StaleInventoryRevision { .. } => None,
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
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedMaterialEgress,
    structural: Option<ValidatedStockpileStructuralLoad>,
    after: PlayerSurvivalRecord,
    next_metabolic_masses: Vec<(MaterialId, AggregateMass)>,
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

fn allocate_nutrition(total_ppm: u32, offered: NutritionEnergy) -> NutritionGain {
    let offered_total = offered.total();
    if total_ppm == 0 || offered_total == 0 {
        return NutritionGain::default();
    }
    let categories = [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ];
    let mut allocated = [0_u32; 3];
    let mut remainders = [(0_u128, 0_usize); 3];
    let mut allocated_total = 0_u32;
    for (index, category) in categories.into_iter().enumerate() {
        let numerator = offered.get(category) * u128::from(total_ppm);
        allocated[index] = (numerator / offered_total) as u32;
        remainders[index] = (numerator % offered_total, index);
        allocated_total += allocated[index];
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    for (_, index) in remainders
        .into_iter()
        .take((total_ppm - allocated_total) as usize)
    {
        allocated[index] += 1;
    }
    NutritionGain {
        grain_ppm: allocated[0],
        fruit_ppm: allocated[1],
        protein_ppm: allocated[2],
    }
}

fn add_food_hydration_up_to_capacity(
    current: Volume,
    gain: Volume,
    maximum: Volume,
) -> (Volume, Volume) {
    let after = current.checked_add(gain).unwrap_or(maximum).min(maximum);
    let gained = after.checked_sub(current).unwrap_or(Volume::ZERO);
    (after, gained)
}

pub fn validate_eat(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedEat, EatError> {
    let Some(player) = state.survival().player().copied() else {
        return Err(EatError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(EatError::PlayerDead);
    }
    let exact_selection = validate_explicit_consumption_selection(
        state.inventory(),
        source,
        selections,
    )
    .map_err(|error| match error {
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
    })?;
    let mut ordered = selections.to_vec();
    ordered.sort_unstable();
    let mut offered_energy_nj = 0_u128;
    let mut offered_hydration_ul = 0_u128;
    let mut category_energy = NutritionEnergy::default();
    let mut metabolic_additions = BTreeMap::<MaterialId, AggregateMass>::new();
    let mut portions = Vec::with_capacity(ordered.len());
    for selection in ordered {
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
        let metabolic_material = food.commodity().material();
        if lot.composition() != &MaterialComposition::pure(metabolic_material) {
            return Err(EatError::UnsupportedComposition {
                lot: selection.lot(),
                material: metabolic_material,
            });
        }
        match assess_food_freshness(registries, state, selection.lot()).map_err(
            |error| match error {
                FoodFreshnessError::UnknownLot { lot } => EatError::UnknownLot { lot },
                FoodFreshnessError::UnknownStockpile { stockpile } => {
                    EatError::UnknownStockpile { stockpile }
                }
                FoodFreshnessError::NotEdible { commodity } => EatError::NotEdible { commodity },
                FoodFreshnessError::ShelfLifeOverflow => EatError::ShelfLifeOverflow,
            },
        )? {
            FoodFreshness::Fresh {
                age: _,
                remaining: _,
            } => {}
            FoodFreshness::Spoiled { age } => {
                return Err(EatError::Spoiled {
                    lot: selection.lot(),
                    age,
                });
            }
        }
        let portion_energy = u128::from(selection.mass().milligrams())
            .checked_mul(u128::from(food.dietary_energy().nanojoules_per_milligram()))
            .ok_or(EatError::NutritionOverflow)?;
        offered_energy_nj = offered_energy_nj
            .checked_add(portion_energy)
            .ok_or(EatError::NutritionOverflow)?;
        category_energy
            .checked_add(food.category(), portion_energy)
            .ok_or(EatError::NutritionOverflow)?;
        offered_hydration_ul = offered_hydration_ul
            .checked_add(
                u128::from(selection.mass().milligrams())
                    .checked_mul(u128::from(food.hydration_microliters_per_milligram()))
                    .ok_or(EatError::NutritionOverflow)?,
            )
            .ok_or(EatError::NutritionOverflow)?;
        let addition = AggregateMass::from_mass(selection.mass());
        let current = metabolic_additions
            .get(&metabolic_material)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        metabolic_additions.insert(
            metabolic_material,
            current
                .checked_add(addition)
                .ok_or(EatError::MetabolicMatterOverflow {
                    material: metabolic_material,
                })?,
        );
        portions.push(EatPortionOutcome {
            lot: selection.lot(),
            mass: selection.mass(),
            category: food.category(),
        });
    }
    let physiology = registries.survival().physiology();
    let offered_energy = Energy::from_nanojoules(offered_energy_nj);
    let available_energy = physiology
        .maximum_metabolic_energy()
        .checked_sub(player.metabolic_energy())
        .ok_or(EatError::NutritionOverflow)?;
    if offered_energy > available_energy {
        return Err(EatError::MetabolicEnergyCapacityExceeded {
            available: available_energy,
            requested: offered_energy,
        });
    }
    let energy_after = player
        .metabolic_energy()
        .checked_add(offered_energy)
        .ok_or(EatError::NutritionOverflow)?;
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

    let hydration_gain_ul =
        u64::try_from(offered_hydration_ul).map_err(|_| EatError::NutritionOverflow)?;
    let energy_gained = offered_energy;
    let (hydration_after, hydration_gained) = add_food_hydration_up_to_capacity(
        player.hydration(),
        Volume::from_microliters(hydration_gain_ul),
        physiology.maximum_hydration(),
    );
    let nutrition_gain_ppm = energy_gained
        .nanojoules()
        .checked_mul(u128::from(super::NUTRITION_PARTS_PER_MILLION))
        .ok_or(EatError::NutritionOverflow)?
        / physiology.maximum_metabolic_energy().nanojoules();
    let nutrition_gain_ppm =
        u32::try_from(nutrition_gain_ppm).map_err(|_| EatError::NutritionOverflow)?;
    let allocated_nutrition = allocate_nutrition(nutrition_gain_ppm, category_energy);
    let (nutrition_after_grain, grain_ppm) = player.nutrition().add(
        FoodCategory::Grain,
        allocated_nutrition.get(FoodCategory::Grain),
    );
    let (nutrition_after_fruit, fruit_ppm) = nutrition_after_grain.add(
        FoodCategory::Fruit,
        allocated_nutrition.get(FoodCategory::Fruit),
    );
    let (nutrition_after, protein_ppm) = nutrition_after_fruit.add(
        FoodCategory::Protein,
        allocated_nutrition.get(FoodCategory::Protein),
    );
    let nutrition_gained = NutritionGain {
        grain_ppm,
        fruit_ppm,
        protein_ppm,
    };
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(EatError::SurvivalRevisionExhausted)?;
    let next_metabolic_masses = metabolic_additions
        .into_iter()
        .map(|(material, addition)| {
            state
                .survival()
                .metabolic_mass(material)
                .checked_add(addition)
                .map(|next| (material, next))
                .ok_or(EatError::MetabolicMatterOverflow { material })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total_mass = egress.total_consumed();
    Ok(ValidatedEat {
        expected_survival_revision,
        next_survival_revision,
        egress,
        structural,
        after: player_record(
            energy_after,
            hydration_after,
            player.vitality(),
            nutrition_after,
        ),
        next_metabolic_masses,
        outcome: EatOutcome {
            portions,
            total_mass,
            energy_gained,
            hydration_gained,
            nutrition_gained,
        },
    })
}

impl ValidatedEat {
    pub fn commit(self, state: &mut AppState) -> Result<EatOutcome, EatCommitError> {
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
        state.survival_state_mut().apply_food_ingestion(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.after,
            self.next_metabolic_masses,
        );
        Ok(self.outcome)
    }
}

/// Failure while validating finite-fluid drinking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrinkError {
    SurvivalNotInitialized,
    PlayerDead,
    UnknownStore {
        store: FluidStoreId,
    },
    EmptyStore {
        store: FluidStoreId,
    },
    NotDrinkable,
    ZeroVolume,
    InsufficientVolume {
        store: FluidStoreId,
        available: Volume,
        requested: Volume,
    },
    FluidRevisionExhausted,
    SurvivalRevisionExhausted,
    HydrationOverflow,
    NoHydrationGain {
        volume: Volume,
    },
    HydrationCapacityExceeded {
        available: Volume,
        requested: Volume,
    },
    IngestedFluidOverflow,
    StructuralLoad(FluidStructuralLoadError),
}

impl Display for DrinkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("player survival is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot drink"),
            Self::UnknownStore { store } => {
                write!(formatter, "unknown drink source store {}", store.value())
            }
            Self::EmptyStore { store } => {
                write!(formatter, "drink source store {} is empty", store.value())
            }
            Self::NotDrinkable => formatter.write_str("stored fluid is not authored as drinkable"),
            Self::ZeroVolume => formatter.write_str("drink volume must be nonzero"),
            Self::InsufficientVolume {
                store,
                available,
                requested,
            } => write!(
                formatter,
                "drink source {} contains {} uL but {} uL was requested",
                store.value(),
                available.microliters(),
                requested.microliters()
            ),
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid revision space is exhausted")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::HydrationOverflow => {
                formatter.write_str("drink hydration calculation overflowed")
            }
            Self::NoHydrationGain { volume } => write!(
                formatter,
                "drink volume {} uL is too small to produce any hydration at the authored multiplier",
                volume.microliters()
            ),
            Self::HydrationCapacityExceeded {
                available,
                requested,
            } => write!(
                formatter,
                "drink provides {} uL of hydration but only {} uL of hydration capacity remains",
                requested.microliters(),
                available.microliters()
            ),
            Self::IngestedFluidOverflow => {
                formatter.write_str("ingested fluid accounting overflowed")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "drink withdrawal structural load failed: {error}"
            ),
        }
    }
}

impl Error for DrinkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::UnknownStore { store: _ }
            | Self::EmptyStore { store: _ }
            | Self::NotDrinkable
            | Self::ZeroVolume
            | Self::InsufficientVolume { .. }
            | Self::FluidRevisionExhausted
            | Self::SurvivalRevisionExhausted
            | Self::HydrationOverflow
            | Self::NoHydrationGain { .. }
            | Self::HydrationCapacityExceeded { .. }
            | Self::IngestedFluidOverflow => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrinkCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleFluidRevision { expected: u64, actual: u64 },
    FluidSourceChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
}

impl Display for DrinkCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated drinking expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "validated drinking expected fluid revision {expected} but current revision is {actual}"
            ),
            Self::FluidSourceChanged { store } => write!(
                formatter,
                "drink source store {} changed after validation",
                store.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated drinking structural commit failed: {error}"
            ),
        }
    }
}

impl Error for DrinkCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleSurvivalRevision { .. }
            | Self::StaleFluidRevision { .. }
            | Self::FluidSourceChanged { store: _ } => None,
        }
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrinkOutcome {
    store: FluidStoreId,
    volume: Volume,
    hydration_gained: Volume,
}

impl DrinkOutcome {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }
    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }
    #[must_use]
    pub const fn hydration_gained(self) -> Volume {
        self.hydration_gained
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedDrink {
    expected_survival_revision: u64,
    next_survival_revision: u64,
    egress: ValidatedFluidEgress,
    after: PlayerSurvivalRecord,
    fluid: crate::fluid::FluidDefinitionId,
    next_ingested_volume: AggregateVolume,
    outcome: DrinkOutcome,
}

pub fn validate_drink(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    volume: Volume,
) -> Result<ValidatedDrink, DrinkError> {
    let Some(player) = state.survival().player().copied() else {
        return Err(DrinkError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(DrinkError::PlayerDead);
    }
    let record = state
        .fluid()
        .get_store(store)
        .ok_or(DrinkError::UnknownStore { store })?;
    let contents = record.contents().ok_or(DrinkError::EmptyStore { store })?;
    let drink = registries
        .survival()
        .get_drink(contents.fluid())
        .copied()
        .ok_or(DrinkError::NotDrinkable)?;
    let egress =
        validate_fluid_egress(registries, state, store, volume).map_err(|error| match error {
            FluidEgressError::UnknownStore { store } => DrinkError::UnknownStore { store },
            FluidEgressError::EmptyStore { store } => DrinkError::EmptyStore { store },
            FluidEgressError::UnknownFluidDefinition { definition: _ } => DrinkError::NotDrinkable,
            FluidEgressError::ZeroVolume => DrinkError::ZeroVolume,
            FluidEgressError::InsufficientVolume {
                store,
                available,
                requested,
            } => DrinkError::InsufficientVolume {
                store,
                available,
                requested,
            },
            FluidEgressError::RevisionExhausted => DrinkError::FluidRevisionExhausted,
            FluidEgressError::StructuralLoad(error) => DrinkError::StructuralLoad(error),
        })?;
    let hydration_numerator = u128::from(egress.volume().microliters())
        .checked_mul(u128::from(drink.hydration_multiplier_ppm()))
        .ok_or(DrinkError::HydrationOverflow)?;
    let hydration_gain = u64::try_from(hydration_numerator / 1_000_000)
        .map_err(|_| DrinkError::HydrationOverflow)?;
    let hydration_gain = Volume::from_microliters(hydration_gain);
    if hydration_gain.is_zero() {
        return Err(DrinkError::NoHydrationGain { volume });
    }
    let physiology = registries.survival().physiology();
    let available_hydration = physiology
        .maximum_hydration()
        .checked_sub(player.hydration())
        .ok_or(DrinkError::HydrationOverflow)?;
    if hydration_gain > available_hydration {
        return Err(DrinkError::HydrationCapacityExceeded {
            available: available_hydration,
            requested: hydration_gain,
        });
    }
    let hydration_after = player
        .hydration()
        .checked_add(hydration_gain)
        .ok_or(DrinkError::HydrationOverflow)?;
    let hydration_gained = hydration_gain;
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(DrinkError::SurvivalRevisionExhausted)?;
    let next_ingested_volume = state
        .survival()
        .ingested_fluid_volume(contents.fluid())
        .checked_add(AggregateVolume::from_volume(volume))
        .ok_or(DrinkError::IngestedFluidOverflow)?;
    Ok(ValidatedDrink {
        expected_survival_revision,
        next_survival_revision,
        egress,
        after: player_record(
            player.metabolic_energy(),
            hydration_after,
            player.vitality(),
            player.nutrition(),
        ),
        fluid: contents.fluid(),
        next_ingested_volume,
        outcome: DrinkOutcome {
            store,
            volume,
            hydration_gained,
        },
    })
}

impl ValidatedDrink {
    pub fn commit(self, state: &mut AppState) -> Result<DrinkOutcome, DrinkCommitError> {
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(DrinkCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        self.egress.commit(state).map_err(|error| match error {
            FluidEgressCommitError::StaleRevision { expected, actual } => {
                DrinkCommitError::StaleFluidRevision { expected, actual }
            }
            FluidEgressCommitError::SourceChanged { store } => {
                DrinkCommitError::FluidSourceChanged { store }
            }
            FluidEgressCommitError::Structure(error) => DrinkCommitError::Structure(error),
        })?;
        state.survival_state_mut().apply_fluid_ingestion(
            self.expected_survival_revision,
            self.next_survival_revision,
            self.after,
            self.fluid,
            self.next_ingested_volume,
        );
        Ok(self.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN, MATERIAL_MEAT, build_registries,
    };
    use crate::core::quantity::{AggregateMass, AggregateVolume, Temperature};
    use crate::core::state::{apply_clock_advance, validate_loaded_state};
    use crate::core::time::{SimulationTick, WorldSeed};
    use crate::fluid::{
        add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting,
    };
    use crate::inventory::{
        StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile, deposit_lot_for_test,
        validate_material_transfer_for_test,
    };
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::survival::assess_survival;
    use crate::survival::{NUTRITION_PARTS_PER_MILLION, initialize_player_survival};

    fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
        initialize_player_survival(registries, state)
            .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
        for _ in 0..5 {
            advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
        }
    }

    #[test]
    fn eating_rejects_meal_beyond_remaining_metabolic_capacity_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0010));
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("full-reserve survival initialization failed: {error}"));
        let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
            .unwrap_or_else(|error| panic!("full-reserve food stockpile failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            Mass::from_milligrams(1),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("full-reserve food lot failed: {error}"));
        let before = state.clone();

        assert_eq!(
            validate_eat(
                &registries,
                &state,
                stockpile,
                &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
            )
            .err(),
            Some(EatError::MetabolicEnergyCapacityExceeded {
                available: Energy::ZERO,
                requested: Energy::from_nanojoules(14_000_000_000),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn drinking_rejects_intake_beyond_remaining_hydration_capacity_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0011));
        initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
            panic!("full-hydration survival initialization failed: {error}")
        });
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            Volume::from_microliters(1),
            FLUID_WATER,
            Volume::from_microliters(1),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("full-hydration water fixture failed: {error}"));
        let before = state.clone();

        assert_eq!(
            validate_drink(&registries, &state, store, Volume::from_microliters(1)).err(),
            Some(DrinkError::HydrationCapacityExceeded {
                available: Volume::ZERO,
                requested: Volume::from_microliters(1),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn eating_moves_exact_food_mass_into_metabolism_and_round_trips() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0001));
        initialize_and_spend_reserves(&registries, &mut state);
        let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("food stockpile fixture failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            Mass::from_milligrams(200),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("food lot fixture failed: {error}"));
        let matter_before = calculate_matter_accounting(&state).unwrap_or_else(|error| {
            panic!("food pre-consumption matter accounting failed: {error}")
        });
        let survival_before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("food fixture survival state is missing"));

        let token = validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
        )
        .unwrap_or_else(|error| panic!("food validation failed: {error}"));
        let outcome = token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("food commit failed: {error}"));

        let matter_after = calculate_matter_accounting(&state).unwrap_or_else(|error| {
            panic!("food post-consumption matter accounting failed: {error}")
        });
        let survival_after = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("food post-consumption survival state is missing"));
        assert_eq!(matter_before.total(), matter_after.total());
        assert_eq!(
            matter_after.metabolic(),
            AggregateMass::from_milligrams(100)
        );
        assert_eq!(
            state.inventory().get_lot(lot).map(|record| record.mass()),
            Some(Mass::from_milligrams(100))
        );
        assert_eq!(outcome.total_mass(), Mass::from_milligrams(100));
        assert_eq!(outcome.portions().len(), 1);
        assert_eq!(outcome.portions()[0].lot(), lot);
        assert_eq!(outcome.portions()[0].mass(), Mass::from_milligrams(100));
        assert_eq!(outcome.portions()[0].category(), FoodCategory::Grain);
        assert!(outcome.nutrition_gained().total_ppm() > 0);
        assert!(survival_after.metabolic_energy() > survival_before.metabolic_energy());
        assert_eq!(
            survival_after.nutrition().get(FoodCategory::Grain),
            NUTRITION_PARTS_PER_MILLION
        );
        assert_eq!(
            survival_after.nutrition().get(FoodCategory::Fruit),
            survival_before.nutrition().get(FoodCategory::Fruit)
        );
        assert_eq!(
            survival_after.nutrition().get(FoodCategory::Protein),
            survival_before.nutrition().get(FoodCategory::Protein)
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("food post-consumption audit failed: {error}"));

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("food save serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("food save decode failed: {error}"));
        let loaded = decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("food save validation failed: {error}"));
        assert_eq!(loaded, state);
    }

    #[test]
    fn varied_meal_consumes_multiple_foods_atomically_and_credits_each_category() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0004));
        initialize_and_spend_reserves(&registries, &mut state);
        let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("varied meal stockpile fixture failed: {error}"));
        let grain = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("varied meal grain fixture failed: {error}"));
        let berries = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("varied meal berry fixture failed: {error}"));
        let meat = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("varied meal meat fixture failed: {error}"));
        let before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("varied meal survival state is missing"));
        let matter_before = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("varied meal initial accounting failed: {error}"));
        let selections = [
            MaterialLotSelection::new(meat, Mass::from_milligrams(10)),
            MaterialLotSelection::new(grain, Mass::from_milligrams(10)),
            MaterialLotSelection::new(berries, Mass::from_milligrams(10)),
        ];

        let outcome = validate_eat(&registries, &state, stockpile, &selections)
            .unwrap_or_else(|error| panic!("varied meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("varied meal commit failed: {error}"));

        assert_eq!(outcome.total_mass(), Mass::from_milligrams(30));
        assert_eq!(outcome.portions().len(), 3);
        for category in [
            FoodCategory::Grain,
            FoodCategory::Fruit,
            FoodCategory::Protein,
        ] {
            assert!(outcome.nutrition_gained().get(category) > 0);
        }
        let after = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("varied meal survival state disappeared"));
        assert!(
            after.nutrition().get(FoodCategory::Grain)
                > before.nutrition().get(FoodCategory::Grain)
        );
        assert!(
            after.nutrition().get(FoodCategory::Fruit)
                > before.nutrition().get(FoodCategory::Fruit)
        );
        assert!(
            after.nutrition().get(FoodCategory::Protein)
                > before.nutrition().get(FoodCategory::Protein)
        );
        let matter_after = calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("varied meal final accounting failed: {error}"));
        assert_eq!(matter_after.total(), matter_before.total());
        assert_eq!(
            matter_before
                .metabolic()
                .checked_add(AggregateMass::from_milligrams(30)),
            Some(matter_after.metabolic())
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("varied meal final audit failed: {error}"));
    }

    #[test]
    fn meal_result_is_independent_of_selection_order() {
        let registries = build_registries();
        let mut base = AppState::new(WorldSeed::new(0x5A70_0006));
        initialize_and_spend_reserves(&registries, &mut base);
        let stockpile = add_solid_stockpile_for_test(&mut base, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("meal-order stockpile fixture failed: {error}"));
        let grain = deposit_lot_for_test(
            &registries,
            &mut base,
            stockpile,
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("meal-order grain fixture failed: {error}"));
        let berries = deposit_lot_for_test(
            &registries,
            &mut base,
            stockpile,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("meal-order berry fixture failed: {error}"));
        let meat = deposit_lot_for_test(
            &registries,
            &mut base,
            stockpile,
            CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("meal-order meat fixture failed: {error}"));
        let mut forward = base.clone();
        let mut reverse = base;
        let forward_selection = [
            MaterialLotSelection::new(grain, Mass::from_milligrams(7)),
            MaterialLotSelection::new(berries, Mass::from_milligrams(11)),
            MaterialLotSelection::new(meat, Mass::from_milligrams(13)),
        ];
        let reverse_selection = [
            MaterialLotSelection::new(meat, Mass::from_milligrams(13)),
            MaterialLotSelection::new(berries, Mass::from_milligrams(11)),
            MaterialLotSelection::new(grain, Mass::from_milligrams(7)),
        ];

        let forward_outcome = validate_eat(&registries, &forward, stockpile, &forward_selection)
            .unwrap_or_else(|error| panic!("forward meal-order validation failed: {error}"))
            .commit(&mut forward)
            .unwrap_or_else(|error| panic!("forward meal-order commit failed: {error}"));
        let reverse_outcome = validate_eat(&registries, &reverse, stockpile, &reverse_selection)
            .unwrap_or_else(|error| panic!("reverse meal-order validation failed: {error}"))
            .commit(&mut reverse)
            .unwrap_or_else(|error| panic!("reverse meal-order commit failed: {error}"));

        assert_eq!(forward_outcome, reverse_outcome);
        assert_eq!(forward, reverse);
    }

    #[test]
    fn meal_rejects_duplicate_lot_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0005));
        initialize_and_spend_reserves(&registries, &mut state);
        let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
            .unwrap_or_else(|error| panic!("duplicate meal stockpile fixture failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("duplicate meal food fixture failed: {error}"));
        let selection = MaterialLotSelection::new(lot, Mass::from_milligrams(5));
        let before = state.clone();

        assert_eq!(
            validate_eat(&registries, &state, stockpile, &[selection, selection]),
            Err(EatError::DuplicateLot { lot })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn preservation_multiplier_extends_food_shelf_life_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0002));
        let profile = StockpileStorageProfile::with_preservation(
            true,
            false,
            Temperature::from_millikelvin(350_000),
            3_000_000,
        )
        .unwrap_or_else(|error| panic!("preserved storage profile failed: {error}"));
        let stockpile = add_stockpile(&mut state, Mass::from_milligrams(1_000), profile)
            .unwrap_or_else(|error| panic!("preserved food stockpile failed: {error}"));
        let lot = deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("preserved berry lot failed: {error}"));

        assert_eq!(
            assess_food_freshness(&registries, &state, lot),
            Ok(FoodFreshness::Fresh {
                age: TickSpan::new(0),
                remaining: TickSpan::new(24_000 * 12),
            })
        );
    }

    #[test]
    fn preservation_transfer_slows_future_spoilage_without_rewriting_prior_age() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0007));
        let ambient = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("ambient food stockpile failed: {error}"));
        let preserved_profile = StockpileStorageProfile::with_preservation(
            true,
            false,
            Temperature::from_millikelvin(350_000),
            3_000_000,
        )
        .unwrap_or_else(|error| panic!("preserved food profile failed: {error}"));
        let preserved = add_stockpile(&mut state, Mass::from_milligrams(1_000), preserved_profile)
            .unwrap_or_else(|error| panic!("preserved food stockpile failed: {error}"));
        let berries = deposit_lot_for_test(
            &registries,
            &mut state,
            ambient,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("preservation-history berry fixture failed: {error}"));

        apply_clock_advance(&mut state, SimulationTick::new(72_000));
        assert_eq!(
            assess_food_freshness(&registries, &state, berries),
            Ok(FoodFreshness::Fresh {
                age: TickSpan::new(72_000),
                remaining: TickSpan::new(24_000),
            })
        );

        validate_material_transfer_for_test(
            &registries,
            &state,
            ambient,
            preserved,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(100),
        )
        .unwrap_or_else(|error| panic!("preservation-history transfer failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("preservation-history transfer commit failed: {error}"));

        assert_eq!(
            assess_food_freshness(&registries, &state, berries),
            Ok(FoodFreshness::Fresh {
                age: TickSpan::new(72_000),
                remaining: TickSpan::new(72_000),
            })
        );
        apply_clock_advance(&mut state, SimulationTick::new(144_000));
        assert_eq!(
            assess_food_freshness(&registries, &state, berries),
            Ok(FoodFreshness::Spoiled {
                age: TickSpan::new(96_000),
            })
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("preservation-history audit failed: {error}"));
    }

    #[test]
    fn partial_transfer_preserves_distinct_food_storage_age_cohorts() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0008));
        let ambient = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
            .unwrap_or_else(|error| panic!("merge-age ambient stockpile failed: {error}"));
        let preserved_profile = StockpileStorageProfile::with_preservation(
            true,
            false,
            Temperature::from_millikelvin(350_000),
            3_000_000,
        )
        .unwrap_or_else(|error| panic!("merge-age preservation profile failed: {error}"));
        let preserved = add_stockpile(&mut state, Mass::from_milligrams(1_000), preserved_profile)
            .unwrap_or_else(|error| panic!("merge-age preserved stockpile failed: {error}"));
        let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
        let old_lot = deposit_lot_for_test(
            &registries,
            &mut state,
            ambient,
            commodity,
            Mass::from_milligrams(100),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("merge-age old berry fixture failed: {error}"));

        apply_clock_advance(&mut state, SimulationTick::new(60_000));
        let destination_lot = deposit_lot_for_test(
            &registries,
            &mut state,
            preserved,
            commodity,
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("merge-age fresh berry fixture failed: {error}"));
        apply_clock_advance(&mut state, SimulationTick::new(72_000));

        validate_material_transfer_for_test(
            &registries,
            &state,
            ambient,
            preserved,
            commodity,
            Mass::from_milligrams(10),
        )
        .unwrap_or_else(|error| panic!("merge-age partial transfer failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("merge-age partial transfer commit failed: {error}"));

        assert_eq!(
            state.inventory().get_lot(old_lot).map(|lot| lot.mass()),
            Some(Mass::from_milligrams(90))
        );
        assert_eq!(
            state
                .inventory()
                .get_lot(destination_lot)
                .map(|lot| lot.mass()),
            Some(Mass::from_milligrams(20))
        );
        assert_eq!(
            assess_food_freshness(&registries, &state, destination_lot),
            Ok(FoodFreshness::Fresh {
                age: TickSpan::new(4_000),
                remaining: TickSpan::new(276_000),
            })
        );
        let transferred_lot = state
            .inventory()
            .lot_ids(preserved)
            .find(|lot| *lot != destination_lot)
            .unwrap_or_else(|| panic!("older transferred berry cohort disappeared"));
        assert_eq!(
            state
                .inventory()
                .get_lot(transferred_lot)
                .map(|lot| lot.mass()),
            Some(Mass::from_milligrams(10))
        );
        assert_eq!(
            assess_food_freshness(&registries, &state, transferred_lot),
            Ok(FoodFreshness::Fresh {
                age: TickSpan::new(72_000),
                remaining: TickSpan::new(72_000),
            })
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("merge-age state audit failed: {error}"));
    }

    #[test]
    fn drinking_moves_finite_water_volume_into_survival_owner() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0003));
        initialize_and_spend_reserves(&registries, &mut state);
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            Volume::from_microliters(10_000),
            FLUID_WATER,
            Volume::from_microliters(5_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("water store fixture failed: {error}"));
        let volume_before = calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("water pre-drink accounting failed: {error}"));
        let hydration_before = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("water fixture survival state is missing"))
            .hydration();

        let token = validate_drink(&registries, &state, store, Volume::from_microliters(625))
            .unwrap_or_else(|error| panic!("drink validation failed: {error}"));
        let outcome = token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("drink commit failed: {error}"));

        let volume_after = calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("water post-drink accounting failed: {error}"));
        let hydration_after = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("water post-drink survival state is missing"))
            .hydration();
        assert_eq!(volume_before.total(), volume_after.total());
        assert_eq!(
            volume_after.get_volume(FLUID_WATER),
            AggregateVolume::from_volume(Volume::from_microliters(5_000))
        );
        assert_eq!(
            state
                .fluid()
                .get_store(store)
                .map(|record| record.stored_volume()),
            Some(Volume::from_microliters(4_375))
        );
        assert_eq!(outcome.hydration_gained(), Volume::from_microliters(625));
        assert_eq!(
            hydration_after,
            hydration_before
                .checked_add(Volume::from_microliters(625))
                .unwrap_or_else(|| panic!("hydration expectation overflowed"))
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("water post-drink audit failed: {error}"));
    }
}
