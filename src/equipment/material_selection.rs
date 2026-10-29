//! Shared hand-safe material selection for ordinary equipment assembly and additive upgrades.

use crate::core::quantity::Temperature;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, InventoryState, StockpileId,
    validate_consumption_selection, validate_consumption_selection_with_filter,
};
use crate::material::{CommodityKey, MaterialInputSpec};

const MAX_HAND_SAFE_TEMPERATURE: Temperature = Temperature::from_millikelvin(353_150);

pub(super) enum EquipmentMaterialSelectionError {
    Selection(ConsumptionSelectionError),
    TooHot {
        commodity: CommodityKey,
        temperature: Temperature,
        maximum: Temperature,
    },
}

fn first_unsafe_cohort(selection: &ConsumptionSelection) -> Option<(CommodityKey, Temperature)> {
    selection.consumed_inputs().iter().find_map(|trace| {
        let temperature = trace.profile().temperature();
        (temperature > MAX_HAND_SAFE_TEMPERATURE)
            .then_some((trace.profile().commodity(), temperature))
    })
}

/// Keep the ordinary stable-identity allocation when it is safe. If it selects hot stock,
/// retry the same inventory-owned allocator with unsafe cohorts excluded. This makes an
/// older hot lot neither a hidden recipe blocker nor a source of free heat handling.
pub(super) fn select_hand_safe_equipment_materials(
    inventory: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
) -> Result<ConsumptionSelection, EquipmentMaterialSelectionError> {
    let selection = validate_consumption_selection(inventory, source, inputs)
        .map_err(EquipmentMaterialSelectionError::Selection)?;
    let Some((commodity, temperature)) = first_unsafe_cohort(&selection) else {
        return Ok(selection);
    };
    match validate_consumption_selection_with_filter(inventory, source, inputs, |lot| {
        lot.temperature() <= MAX_HAND_SAFE_TEMPERATURE
    }) {
        Ok(selection) => Ok(selection),
        Err(ConsumptionSelectionError::InsufficientMass { .. }) => {
            Err(EquipmentMaterialSelectionError::TooHot {
                commodity,
                temperature,
                maximum: MAX_HAND_SAFE_TEMPERATURE,
            })
        }
        Err(error) => Err(EquipmentMaterialSelectionError::Selection(error)),
    }
}
