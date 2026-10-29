//! Temperature-limited exact selection over the canonical inventory lot allocator.

use crate::core::quantity::Temperature;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::super::state::{InventoryState, StockpileId};
use super::ConsumptionSelection;
use super::implicit::{
    ConsumptionSelectionError, validate_consumption_selection,
    validate_consumption_selection_with_filter,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TemperatureLimitedSelectionError {
    Selection(ConsumptionSelectionError),
    TooHot {
        commodity: CommodityKey,
        temperature: Temperature,
        maximum: Temperature,
    },
}

/// Keep normal identity-ordered selection if safe; otherwise search the same
/// eligible lots in stable order with unsafe cohorts excluded. Only when enough
/// matter exists but not enough safe matter does temperature explain rejection.
pub(crate) fn validate_temperature_limited_consumption_selection(
    inventory: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
    maximum: Temperature,
) -> Result<ConsumptionSelection, TemperatureLimitedSelectionError> {
    let selection = validate_consumption_selection(inventory, source, inputs)
        .map_err(TemperatureLimitedSelectionError::Selection)?;
    let Some(unsafe_trace) = selection
        .consumed_inputs()
        .iter()
        .find(|trace| trace.profile().temperature() > maximum)
    else {
        return Ok(selection);
    };
    match validate_consumption_selection_with_filter(inventory, source, inputs, |lot| {
        lot.temperature() <= maximum
    }) {
        Ok(selection) => Ok(selection),
        Err(ConsumptionSelectionError::InsufficientMass { commodity, .. }) => {
            // Report the actual constrained ingredient, not merely the
            // earliest hot lot in the original successful allocation.
            let blocked_trace = selection
                .consumed_inputs()
                .iter()
                .find(|trace| {
                    trace.profile().commodity() == commodity
                        && trace.profile().temperature() > maximum
                })
                .unwrap_or(unsafe_trace);
            Err(TemperatureLimitedSelectionError::TooHot {
                commodity,
                temperature: blocked_trace.profile().temperature(),
                maximum,
            })
        }
        Err(error) => Err(TemperatureLimitedSelectionError::Selection(error)),
    }
}
