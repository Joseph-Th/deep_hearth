//! Canonical conserved food and drink consumption transactions.

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
    StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedMaterialEgress,
    ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_explicit_consumption_selection, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::material::{CommodityKey, MaterialComposition, MaterialId};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::{PlayerSurvivalRecord, player_record};
use super::{FoodCategory, SurvivalAssessment, Vitality, assess_survival};

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

fn effective_shelf_life(
    base: TickSpan,
    preservation_multiplier_ppm: u32,
) -> Result<TickSpan, FoodFreshnessError> {
    let scaled = u128::from(base.value())
        .checked_mul(u128::from(preservation_multiplier_ppm))
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?
        / 1_000_000;
    let ticks = u64::try_from(scaled).map_err(|_| FoodFreshnessError::ShelfLifeOverflow)?;
    Ok(TickSpan::new(ticks))
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
    let shelf_life = effective_shelf_life(
        food.shelf_life(),
        stockpile.storage_profile().preservation_multiplier_ppm(),
    )?;
    let age = TickSpan::new(state.tick().value() - record.created_at().value());
    if age.value() >= shelf_life.value() {
        Ok(FoodFreshness::Spoiled { age })
    } else {
        Ok(FoodFreshness::Fresh {
            age,
            remaining: TickSpan::new(shelf_life.value() - age.value()),
        })
    }
}

/// Failure while validating one exact eating action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatError {
    SurvivalNotInitialized,
    PlayerDead,
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
pub struct EatOutcome {
    lot: MaterialLotId,
    mass: Mass,
    category: FoodCategory,
    energy_gained: Energy,
    hydration_gained: Volume,
}

impl EatOutcome {
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
    #[must_use]
    pub const fn energy_gained(&self) -> Energy {
        self.energy_gained
    }
    #[must_use]
    pub const fn hydration_gained(&self) -> Volume {
        self.hydration_gained
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
    metabolic_material: MaterialId,
    next_metabolic_mass: AggregateMass,
    outcome: EatOutcome,
}

fn add_capped_energy(current: Energy, gain: Energy, maximum: Energy) -> (Energy, Energy) {
    let after = current.checked_add(gain).unwrap_or(maximum).min(maximum);
    let gained = after.checked_sub(current).unwrap_or(Energy::ZERO);
    (after, gained)
}

fn add_capped_volume(current: Volume, gain: Volume, maximum: Volume) -> (Volume, Volume) {
    let after = current.checked_add(gain).unwrap_or(maximum).min(maximum);
    let gained = after.checked_sub(current).unwrap_or(Volume::ZERO);
    (after, gained)
}

pub fn validate_eat(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selection: MaterialLotSelection,
) -> Result<ValidatedEat, EatError> {
    let Some(player) = state.survival().player().copied() else {
        return Err(EatError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(EatError::PlayerDead);
    }
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
    let exact_selection = validate_explicit_consumption_selection(
        state.inventory(),
        source,
        &[selection],
    )
    .map_err(|error| match error {
        ExplicitConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EatError::UnknownStockpile { stockpile }
        }
        ExplicitConsumptionSelectionError::EmptySelection => {
            unreachable!("single eating selection is never empty")
        }
        ExplicitConsumptionSelectionError::ZeroMass { lot } => EatError::ZeroMass { lot },
        ExplicitConsumptionSelectionError::DuplicateLot { lot: _ } => {
            unreachable!("single eating selection cannot contain a duplicate lot")
        }
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

    let energy_gain_nj = u128::from(selection.mass().milligrams())
        .checked_mul(u128::from(food.dietary_energy().nanojoules_per_milligram()))
        .ok_or(EatError::NutritionOverflow)?;
    let hydration_gain_ul = u128::from(selection.mass().milligrams())
        .checked_mul(u128::from(food.hydration_microliters_per_milligram()))
        .ok_or(EatError::NutritionOverflow)?;
    let hydration_gain_ul =
        u64::try_from(hydration_gain_ul).map_err(|_| EatError::NutritionOverflow)?;
    let physiology = registries.survival().physiology();
    let (energy_after, energy_gained) = add_capped_energy(
        player.metabolic_energy(),
        Energy::from_nanojoules(energy_gain_nj),
        physiology.maximum_metabolic_energy(),
    );
    let (hydration_after, hydration_gained) = add_capped_volume(
        player.hydration(),
        Volume::from_microliters(hydration_gain_ul),
        physiology.maximum_hydration(),
    );
    let expected_survival_revision = state.survival().revision();
    let next_survival_revision = expected_survival_revision
        .checked_add(1)
        .ok_or(EatError::SurvivalRevisionExhausted)?;
    let next_metabolic_mass = state
        .survival()
        .metabolic_mass(metabolic_material)
        .checked_add(AggregateMass::from_mass(selection.mass()))
        .ok_or(EatError::MetabolicMatterOverflow {
            material: metabolic_material,
        })?;
    Ok(ValidatedEat {
        expected_survival_revision,
        next_survival_revision,
        egress,
        structural,
        after: player_record(energy_after, hydration_after, player.vitality()),
        metabolic_material,
        next_metabolic_mass,
        outcome: EatOutcome {
            lot: selection.lot(),
            mass: selection.mass(),
            category: food.category(),
            energy_gained,
            hydration_gained,
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
            self.metabolic_material,
            self.next_metabolic_mass,
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
    let physiology = registries.survival().physiology();
    let (hydration_after, hydration_gained) = add_capped_volume(
        player.hydration(),
        Volume::from_microliters(hydration_gain),
        physiology.maximum_hydration(),
    );
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

/// Returns the post-action survival assessment for callers that need one consolidated projection.
#[must_use]
pub fn survival_after_action(
    registries: &Registries,
    state: &AppState,
) -> Option<SurvivalAssessment> {
    assess_survival(registries, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN, build_registries,
    };
    use crate::core::quantity::{AggregateMass, AggregateVolume, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::fluid::{add_fluid_store_with_contents_for_test, calculate_fluid_volume_accounting};
    use crate::inventory::{
        StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile, deposit_lot_for_test,
    };
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::survival::initialize_player_survival;

    fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
        initialize_player_survival(registries, state)
            .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
        for _ in 0..5 {
            advance_tick(registries, state)
                .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
        }
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
            MaterialLotSelection::new(lot, Mass::from_milligrams(100)),
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
        assert_eq!(outcome.mass(), Mass::from_milligrams(100));
        assert!(survival_after.metabolic_energy() > survival_before.metabolic_energy());
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
    fn preservation_multiplier_extends_food_shelf_life_without_mutation() {
        assert_eq!(
            effective_shelf_life(TickSpan::new(10), 2_000_000),
            Ok(TickSpan::new(20))
        );

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
    fn drinking_moves_finite_water_volume_into_survival_owner() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5A70_0003));
        initialize_and_spend_reserves(&registries, &mut state);
        let store = add_fluid_store_with_contents_for_test(
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

        let token = validate_drink(&registries, &state, store, Volume::from_microliters(1_000))
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
            Some(Volume::from_microliters(4_000))
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
