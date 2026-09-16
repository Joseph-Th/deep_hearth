//! Harness-side exact inventory binding for controlled structural materialization.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ExplicitConsumptionSelectionError, MaterialLotSelection, StockpileId,
    validate_explicit_consumption_selection,
};

use super::super::state::StructuralElementId;

/// Immutable fixture materialization selection for a planned member.
///
/// There is no runtime/public constructor. Player construction is outside current production scope;
/// this setup-only binding intentionally omits joinery, wastage, tooling, labor, and duration.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct StructuralConstructionResolution {
    element: StructuralElementId,
    selection: ConsumptionSelection,
}

impl StructuralConstructionResolution {
    #[must_use]
    pub fn mass(&self) -> Mass {
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

/// Harness-side binding failure for controlled fixture materialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructuralConstructionBindingError {
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
