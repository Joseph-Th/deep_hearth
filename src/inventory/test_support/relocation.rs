//! Unit-test composition over canonical inventory selection and exact relocation.

use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::registry::Registries;

use super::super::selection::{ConsumptionSelectionError, validate_consumption_selection};
use super::super::state::StockpileId;
use super::super::transactions::{
    MaterialRelocationError, ValidatedMaterialRelocation,
    validate_material_relocation_from_selection,
};

/// Test-only composition failure without duplicating canonical inventory error semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationTestError {
    ZeroMass,
    Selection(ConsumptionSelectionError),
    Relocation(MaterialRelocationError),
}

impl Display for MaterialRelocationTestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => formatter.write_str("test material relocation mass must be nonzero"),
            Self::Selection(error) => write!(formatter, "material selection failed: {error:?}"),
            Self::Relocation(error) => write!(formatter, "material relocation failed: {error}"),
        }
    }
}

/// Composes canonical selection and exact relocation for controlled custody moves in unit tests.
pub(crate) fn validate_material_relocation_for_test(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<ValidatedMaterialRelocation, MaterialRelocationTestError> {
    if mass.is_zero() {
        return Err(MaterialRelocationTestError::ZeroMass);
    }
    let selection = validate_consumption_selection(
        state.inventory(),
        source,
        &[MaterialInputSpec::new(commodity, mass)],
    )
    .map_err(MaterialRelocationTestError::Selection)?;
    validate_material_relocation_from_selection(registries, state, destination, selection)
        .map_err(MaterialRelocationTestError::Relocation)
}
