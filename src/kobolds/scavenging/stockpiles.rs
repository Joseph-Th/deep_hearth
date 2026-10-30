//! Scout raid assessment for exposed, unprotected stationary stockpiles.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{MaterialLotId, MaterialLotRecord, StockpileId};
use crate::material::MaterialPhase;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::{FoodFreshness, freshness_from_history};

use super::{SCOUT_SAFE_SCAVENGE_TEMPERATURE, is_servant_era_commodity};

/// A scavenger only touches hand-safe solids. Food is useful only when
/// it remains fresh and edible under the specific stockpile's storage history.
/// This is read-only eligibility; the custody owner repeats physical checks
/// when it validates the actual inventory-to-surface transfer.
fn scout_can_extract_lot(
    registries: &Registries,
    lot: &MaterialLotRecord,
    preservation_multiplier_ppm: u32,
    at: SimulationTick,
) -> bool {
    let commodity = lot.commodity();
    if lot.temperature() > SCOUT_SAFE_SCAVENGE_TEMPERATURE
        || !registries
            .materials()
            .get_form(commodity.form())
            .is_some_and(|form| form.phase() == MaterialPhase::Solid)
    {
        return false;
    }
    if commodity.form() != crate::content::FORM_FOOD {
        return true;
    }
    let Some(food) = registries.survival().get_food(commodity) else {
        return false;
    };
    lot.composition().pure_material() == Some(commodity.material())
        && food.consumption_temperature().contains(lot.temperature())
        && matches!(
            freshness_from_history(
                lot.storage_history(),
                preservation_multiplier_ppm,
                at,
                food.shelf_life()
            ),
            Ok(FoodFreshness::Fresh { .. })
        )
}

/// An informed scavenger can remove a finite portion from an exposed ground
/// cache, but not from a carried pack, enclosure, supported installation or
/// reserved store. Only the *current* scout cell is considered. Food must
/// remain fresh and at an edible temperature; shaped precision metal is the
/// next priority, followed by other useful stock in stable identity order.
/// No distant inventory or hidden cache can influence the choice.
pub(in crate::kobolds) fn local_unenclosed_stockpile_target(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> Option<(StockpileId, MaterialLotId)> {
    let mut best = None;
    for source in state.logistics().stockpiles_at(position) {
        let Some(store) = state.inventory().get_stockpile(source) else {
            continue;
        };
        if store.enclosure().is_some()
            || store.supported_by().is_some()
            || !store.reserved_inbound().is_zero()
        {
            continue;
        }
        for (commodity, _) in store.contents() {
            for lot_id in state.inventory().lot_ids_for_commodity(source, commodity) {
                let Some(lot) = state.inventory().get_lot(lot_id) else {
                    continue;
                };
                if !scout_can_extract_lot(
                    registries,
                    lot,
                    store.storage_profile().preservation_multiplier_ppm(),
                    at,
                ) {
                    continue;
                }
                let priority = (
                    commodity.form() != crate::content::FORM_FOOD,
                    !is_servant_era_commodity(commodity),
                    source,
                    lot_id,
                );
                if best.is_none_or(|(previous, _, _)| priority < previous) {
                    best = Some((priority, source, lot_id));
                }
            }
        }
    }
    best.map(|(_, source, lot)| (source, lot))
}
