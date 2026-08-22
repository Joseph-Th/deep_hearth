//! Read-only food freshness projection from lot storage history and preservation.

use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::inventory::{MaterialLotId, STORAGE_AGE_PARTS_PER_TICK, StockpileId};
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
