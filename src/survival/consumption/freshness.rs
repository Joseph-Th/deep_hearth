//! Read-only food freshness projection from lot storage history and preservation.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{
    MaterialLotId, MaterialStorageHistory, STORAGE_AGE_PARTS_PER_TICK, StockpileId,
    StorageDefinitionId,
};
use crate::material::CommodityKey;
use crate::registry::Registries;

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

/// Failure while projecting freshness through one prospective authored storage transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodFreshnessProjectionError {
    Freshness(FoodFreshnessError),
    UnknownStorageDefinition {
        definition: StorageDefinitionId,
    },
    TransitionBeforeCurrent {
        transition_at: SimulationTick,
        current: SimulationTick,
    },
    AssessmentBeforeTransition {
        assessment_at: SimulationTick,
        transition_at: SimulationTick,
    },
}

impl From<FoodFreshnessError> for FoodFreshnessProjectionError {
    fn from(error: FoodFreshnessError) -> Self {
        Self::Freshness(error)
    }
}

fn freshness_from_history(
    history: MaterialStorageHistory,
    preservation_multiplier_ppm: u32,
    at: SimulationTick,
    shelf_life: TickSpan,
) -> Result<FoodFreshness, FoodFreshnessError> {
    let age_parts = history
        .project(at, preservation_multiplier_ppm)
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
    let shelf_life_parts = u128::from(shelf_life.value())
        .checked_mul(STORAGE_AGE_PARTS_PER_TICK)
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
    let age_ticks = age_parts.div_ceil(STORAGE_AGE_PARTS_PER_TICK);
    let age =
        TickSpan::new(u64::try_from(age_ticks).map_err(|_| FoodFreshnessError::ShelfLifeOverflow)?);
    if age_parts >= shelf_life_parts {
        Ok(FoodFreshness::Spoiled { age })
    } else {
        let remaining = history
            .ticks_until_projected_age(at, preservation_multiplier_ppm, shelf_life_parts)
            .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
        Ok(FoodFreshness::Fresh { age, remaining })
    }
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
    freshness_from_history(
        record.storage_history(),
        stockpile.storage_profile().preservation_multiplier_ppm(),
        state.tick(),
        food.shelf_life(),
    )
}

/// Projects one food lot through a future transition into an authored storage enclosure.
///
/// The lot remains under its current stockpile preservation rate until `transition_at`, then uses
/// the selected enclosure's preservation rate through `assessment_at`. This is read-only planning
/// evidence, not construction authorization: capacity, material availability, containment, and
/// intervening state changes must still be resolved and validated by the inventory/crafting owners.
pub fn project_food_freshness_after_storage_transition(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
    transition_at: SimulationTick,
    definition: StorageDefinitionId,
    assessment_at: SimulationTick,
) -> Result<FoodFreshness, FoodFreshnessProjectionError> {
    if transition_at < state.tick() {
        return Err(FoodFreshnessProjectionError::TransitionBeforeCurrent {
            transition_at,
            current: state.tick(),
        });
    }
    if assessment_at < transition_at {
        return Err(FoodFreshnessProjectionError::AssessmentBeforeTransition {
            assessment_at,
            transition_at,
        });
    }
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
    let destination = registries
        .storage()
        .get(definition)
        .ok_or(FoodFreshnessProjectionError::UnknownStorageDefinition { definition })?;
    let source_preservation = stockpile.storage_profile().preservation_multiplier_ppm();
    let destination_preservation = destination.storage_profile().preservation_multiplier_ppm();
    let projected_history = record
        .storage_history()
        .transition_preservation(transition_at, source_preservation, destination_preservation)
        .ok_or(FoodFreshnessError::ShelfLifeOverflow)?;
    freshness_from_history(
        projected_history,
        destination_preservation,
        assessment_at,
        food.shelf_life(),
    )
    .map_err(Into::into)
}
