//! Exact caller-selected lot binding for outcome-sensitive material operations.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::super::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialLotId, MaterialLotRecord, StockpileId,
};
use super::{ConsumptionSelection, MaterialLotSelection};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExplicitConsumptionSelectionError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    EmptySelection,
    ZeroMass {
        lot: MaterialLotId,
    },
    DuplicateLot {
        lot: MaterialLotId,
    },
    UnknownLot {
        lot: MaterialLotId,
    },
    LotOwnedElsewhere {
        lot: MaterialLotId,
        requested_source: StockpileId,
        actual_source: StockpileId,
    },
    InsufficientLotMass {
        lot: MaterialLotId,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
}

pub(crate) fn validate_explicit_consumption_selection(
    state: &InventoryState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ConsumptionSelection, ExplicitConsumptionSelectionError> {
    if state.get_stockpile(source).is_none() {
        return Err(ExplicitConsumptionSelectionError::UnknownStockpile { stockpile: source });
    }
    let ordered = order_explicit_selections(selections)?;

    let mut total_consumed = Mass::ZERO;
    let mut lot_slices = Vec::with_capacity(ordered.len());
    let mut consumed_inputs = Vec::with_capacity(ordered.len());
    let mut aggregate_inputs = BTreeMap::<CommodityKey, Mass>::new();
    for selection in ordered {
        let lot = validate_explicit_lot_selection(state, source, selection)?;
        total_consumed = total_consumed
            .checked_add(selection.mass)
            .ok_or(ExplicitConsumptionSelectionError::MassOverflow { stockpile: source })?;
        let current = aggregate_inputs
            .get(&lot.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        aggregate_inputs.insert(
            lot.commodity(),
            current
                .checked_add(selection.mass)
                .ok_or(ExplicitConsumptionSelectionError::MassOverflow { stockpile: source })?,
        );
        lot_slices.push(LotSlice {
            lot: selection.lot,
            mass: selection.mass,
        });
        consumed_inputs.push(ConsumedMaterialTrace {
            mass: selection.mass,
            profile: lot.profile.clone(),
            provenance: lot.provenance,
        });
    }

    let inputs = aggregate_inputs
        .into_iter()
        .map(|(commodity, mass)| MaterialInputSpec::new(commodity, mass))
        .collect();
    Ok(ConsumptionSelection {
        expected_revision: state.revision(),
        source,
        inputs,
        lot_slices,
        consumed_inputs,
    })
}

fn order_explicit_selections(
    selections: &[MaterialLotSelection],
) -> Result<Vec<MaterialLotSelection>, ExplicitConsumptionSelectionError> {
    if selections.is_empty() {
        return Err(ExplicitConsumptionSelectionError::EmptySelection);
    }
    let mut ordered = selections.to_vec();
    ordered.sort();
    if let Some(pair) = ordered.windows(2).find(|pair| pair[0].lot == pair[1].lot) {
        return Err(ExplicitConsumptionSelectionError::DuplicateLot { lot: pair[0].lot });
    }
    Ok(ordered)
}

fn validate_explicit_lot_selection(
    state: &InventoryState,
    source: StockpileId,
    selection: MaterialLotSelection,
) -> Result<&MaterialLotRecord, ExplicitConsumptionSelectionError> {
    if selection.mass.is_zero() {
        return Err(ExplicitConsumptionSelectionError::ZeroMass { lot: selection.lot });
    }
    let lot = state
        .get_lot(selection.lot)
        .ok_or(ExplicitConsumptionSelectionError::UnknownLot { lot: selection.lot })?;
    if lot.stockpile() != source {
        return Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
            lot: selection.lot,
            requested_source: source,
            actual_source: lot.stockpile(),
        });
    }
    if lot.mass() < selection.mass {
        return Err(ExplicitConsumptionSelectionError::InsufficientLotMass {
            lot: selection.lot,
            available: lot.mass(),
            requested: selection.mass,
        });
    }
    Ok(lot)
}
