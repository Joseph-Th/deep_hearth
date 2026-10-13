//! Exact inventory binding for structural materialization.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ExplicitConsumptionSelectionError, MaterialLotSelection, StockpileId,
    validate_explicit_consumption_selection,
};

use super::super::state::StructuralElementId;

/// Immutable exact materialization selection for a planned member.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StructuralConstructionResolution {
    element: StructuralElementId,
    selection: ConsumptionSelection,
}

impl StructuralConstructionResolution {
    #[must_use]
    pub(crate) fn mass(&self) -> Mass {
        self.selection.total_consumed()
    }

    pub(super) const fn element(&self) -> StructuralElementId {
        self.element
    }

    pub(super) const fn selection(&self) -> &ConsumptionSelection {
        &self.selection
    }

    pub(super) fn into_selection(self) -> ConsumptionSelection {
        self.selection
    }
}

/// Failure while binding exact inventory matter to one planned member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralConstructionBindingError {
    Inventory(ExplicitConsumptionSelectionError),
}

pub(crate) fn bind_structural_construction_selection(
    state: &AppState,
    element: StructuralElementId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<StructuralConstructionResolution, StructuralConstructionBindingError> {
    let selection = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .map_err(StructuralConstructionBindingError::Inventory)?;
    Ok(StructuralConstructionResolution { element, selection })
}
