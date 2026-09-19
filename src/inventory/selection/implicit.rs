//! Stable implicit allocation for fixed authored material requirements.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::super::state::{
    ConsumedMaterialTrace, InventoryState, LotSlice, MaterialLotId, StockpileId, StockpileRecord,
};
use super::ConsumptionSelection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConsumptionSelectionError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    InsufficientMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        requested: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
}

pub(crate) fn validate_consumption_selection(
    state: &InventoryState,
    source: StockpileId,
    inputs: &[MaterialInputSpec],
) -> Result<ConsumptionSelection, ConsumptionSelectionError> {
    let Some(source_record) = state.get_stockpile(source) else {
        return Err(ConsumptionSelectionError::UnknownStockpile { stockpile: source });
    };

    let mut total_consumed = Mass::ZERO;
    let mut lot_slices = Vec::new();
    let mut selected_by_lot = BTreeMap::<MaterialLotId, Mass>::new();
    for input in inputs {
        let selected = select_input_lot_slices(state, source_record, input, &mut selected_by_lot)
            .map_err(|available| ConsumptionSelectionError::InsufficientMass {
            stockpile: source,
            commodity: input.commodity(),
            available,
            requested: input.mass(),
        })?;
        total_consumed = total_consumed
            .checked_add(input.mass())
            .ok_or(ConsumptionSelectionError::MassOverflow { stockpile: source })?;
        lot_slices.extend(selected);
    }

    let consumed_inputs = lot_slices
        .iter()
        .map(|slice| {
            let lot = match state.get_lot(slice.lot) {
                Some(lot) => lot,
                None => panic!(
                    "validated input slice references missing material lot {}",
                    slice.lot.value()
                ),
            };
            ConsumedMaterialTrace {
                mass: slice.mass,
                profile: lot.profile.clone(),
                provenance: lot.provenance,
            }
        })
        .collect();

    Ok(ConsumptionSelection {
        expected_revision: state.revision(),
        source,
        inputs: inputs.to_vec(),
        lot_slices,
        consumed_inputs,
    })
}

fn select_input_lot_slices(
    inventories: &InventoryState,
    source: &StockpileRecord,
    input: &MaterialInputSpec,
    selected_by_lot: &mut BTreeMap<MaterialLotId, Mass>,
) -> Result<Vec<LotSlice>, Mass> {
    let mut remaining = input.mass();
    let mut available = Mass::ZERO;
    let mut slices = Vec::new();

    // Fixed-input recipes intentionally use stable persistent identity as their generic allocation
    // order. This is not a FIFO/FEFO policy: owners whose outcome depends on age or another local
    // lot property must require explicit lot selection, as direct food consumption does.
    for lot_id in inventories.lot_ids_for_commodity(source.id, input.commodity()) {
        let lot = inventories.get_lot(lot_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} indexes missing lot {}",
                source.id.value(),
                lot_id.value()
            )
        });
        if !input.is_satisfied_by(lot.composition()) {
            continue;
        }

        let already_selected = selected_by_lot.get(&lot_id).copied().unwrap_or(Mass::ZERO);
        let free = lot.mass.checked_sub(already_selected).unwrap_or_else(|| {
            panic!("input allocator selected more mass than material lot contains")
        });
        available = available
            .checked_add(free)
            .unwrap_or_else(|| panic!("eligible input mass overflowed stockpile mass accounting"));
        if free.is_zero() {
            continue;
        }

        let take = free.min(remaining);
        slices.push(LotSlice {
            lot: lot_id,
            mass: take,
        });
        let selected_after = already_selected
            .checked_add(take)
            .unwrap_or_else(|| panic!("input allocator selection overflowed material lot mass"));
        selected_by_lot.insert(lot_id, selected_after);
        remaining = remaining
            .checked_sub(take)
            .unwrap_or_else(|| panic!("input allocator underflowed remaining requested mass"));
        if remaining.is_zero() {
            return Ok(slices);
        }
    }

    Err(available)
}
