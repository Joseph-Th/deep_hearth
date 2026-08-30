//! Fixture-only geometry-constrained materialization of planned structural members.
//!
//! Member geometry and material density determine the exact conservative solid-mass requirement. This
//! module exists to create physically valid controlled test/gameplay-audit starting states. It is not a
//! player construction system and does not authorize labor, tools, joints, cutting/placement waste, or
//! build duration.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force, Mass, Volume};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, MaterialEgressError, StockpileId, StockpileStoredMassChange,
    StockpileStructuralLoadError, ValidatedMaterialEgress, ValidatedStockpileStructuralLoad,
    apply_material_egress, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialLotSelection,
    validate_explicit_consumption_selection,
};
use crate::material::MaterialId;
use crate::registry::Registries;

use super::geometry::{
    StructuralGeometryError, calculate_prismatic_material_mass_ceiling,
    calculate_prismatic_volume_ceiling,
};
use super::load::calculate_aggregate_weight_force_ceiling;
#[cfg(test)]
use super::state::StructuralLoadKind;
use super::state::{StructuralElementId, StructuralLifecycle};

/// Read-only physical material requirement for one prismatic structural member.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralMaterialRequirement {
    element: StructuralElementId,
    material: MaterialId,
    solid_volume_ceiling: Volume,
    required_mass: Mass,
}

impl StructuralMaterialRequirement {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn solid_volume_ceiling(self) -> Volume {
        self.solid_volume_ceiling
    }

    #[must_use]
    pub const fn required_mass(self) -> Mass {
        self.required_mass
    }
}

/// Failure while deriving a member's physical solid-material requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMaterialRequirementError {
    UnknownElement {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
}

impl Display for StructuralMaterialRequirementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} material requirement cannot be resolved: {error}",
                element.value()
            ),
        }
    }
}

impl Error for StructuralMaterialRequirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry {
                element: _element,
                error,
            } => Some(error),
            Self::UnknownElement { element: _element } => None,
        }
    }
}

/// Derives conservative solid volume and exact milligram ownership from member geometry and density.
pub fn resolve_structural_material_requirement(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<StructuralMaterialRequirement, StructuralMaterialRequirementError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralMaterialRequirementError::UnknownElement { element })?;
    let solid_volume_ceiling =
        calculate_prismatic_volume_ceiling(record.cross_section(), record.length())
            .map_err(|error| StructuralMaterialRequirementError::Geometry { element, error })?;
    let required_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        record.material(),
        record.cross_section(),
        record.length(),
    )
    .map_err(|error| StructuralMaterialRequirementError::Geometry { element, error })?;
    Ok(StructuralMaterialRequirement {
        element,
        material: record.material(),
        solid_volume_ceiling,
        required_mass,
    })
}

/// Immutable fixture materialization selection for a planned member.
///
/// There is no runtime/public constructor. Player construction is outside current production scope;
/// this setup-only binding intentionally omits joinery, wastage, tooling, labor, and duration.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct StructuralConstructionResolution {
    element: StructuralElementId,
    selection: crate::inventory::ConsumptionSelection,
}

impl StructuralConstructionResolution {
    #[must_use]
    pub fn mass(&self) -> Mass {
        self.selection.total_consumed()
    }
}

/// Harness-side binding failure for controlled fixture materialization.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructuralConstructionBindingError {
    Inventory(ExplicitConsumptionSelectionError),
}

#[cfg(any(test, feature = "test-gameplay"))]
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

mod errors;

pub use errors::{StructuralConstructionCommitError, StructuralConstructionError};

/// Consumed proof that exact inventory matter can become one member's embodied matter atomically.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedStructuralConstruction {
    element: StructuralElementId,
    expected_structure_revision: u64,
    next_structure_revision: u64,
    material: Vec<ConsumedMaterialTrace>,
    self_weight: Force,
    egress: ValidatedMaterialEgress,
    stockpile_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedStructuralConstruction {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn self_weight(&self) -> Force {
        self.self_weight
    }

    /// Commits both owners only after rechecking both revisions and the target lifecycle.
    pub fn commit(self, state: &mut AppState) -> Result<(), StructuralConstructionCommitError> {
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != self.expected_structure_revision {
            return Err(StructuralConstructionCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.egress.expected_revision() {
            return Err(StructuralConstructionCommitError::StaleInventoryRevision {
                expected: self.egress.expected_revision(),
                actual: actual_inventory_revision,
            });
        }
        let Some(record) = state.structures().get_element(self.element) else {
            return Err(StructuralConstructionCommitError::StateChanged {
                element: self.element,
            });
        };
        if record.lifecycle() != StructuralLifecycle::Planned || !record.embodied_mass().is_zero() {
            return Err(StructuralConstructionCommitError::StateChanged {
                element: self.element,
            });
        }
        if let Some(stockpile_load) = &self.stockpile_load {
            let expected = stockpile_load.expected_revision();
            if expected != self.expected_structure_revision {
                return Err(StructuralConstructionCommitError::StaleStructureRevision {
                    expected,
                    actual: self.expected_structure_revision,
                });
            }
        }

        if let Some(stockpile_load) = self.stockpile_load {
            stockpile_load
                .commit(state)
                .map_err(StructuralConstructionCommitError::Structure)?;
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        let structures = state.structure_state_mut();
        structures.set_embodied_matter(self.element, self.material, self.self_weight);
        structures.apply_revision(self.next_structure_revision);
        Ok(())
    }
}

/// Validates a physically resolved material batch for one still-planned member.
pub fn validate_structural_construction(
    registries: &Registries,
    state: &AppState,
    resolution: StructuralConstructionResolution,
) -> Result<ValidatedStructuralConstruction, StructuralConstructionError> {
    let element = resolution.element;
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralConstructionError::UnknownElement { element })?;
    if record.lifecycle() != StructuralLifecycle::Planned {
        return Err(StructuralConstructionError::ElementNotPlanned {
            element,
            lifecycle: record.lifecycle(),
        });
    }
    if !record.embodied_mass().is_zero() || !record.embodied_material().is_empty() {
        return Err(StructuralConstructionError::AlreadyMaterialized { element });
    }
    registries
        .structural()
        .get_profile(record.profile())
        .ok_or(StructuralConstructionError::UnknownProfile {
            element,
            profile: record.profile(),
        })?;
    for trace in resolution.selection.consumed_inputs() {
        let form_id = trace.profile().commodity().form();
        let Some(form) = registries.materials().get_form(form_id) else {
            return Err(StructuralConstructionError::UnknownMaterialForm {
                element,
                form: form_id,
            });
        };
        if !form.is_consolidated() {
            return Err(StructuralConstructionError::UnconsolidatedForm {
                element,
                form: form_id,
            });
        }
        let found = trace.profile().commodity().material();
        if found != record.material() {
            return Err(StructuralConstructionError::MaterialMismatch {
                element,
                expected: record.material(),
                found,
            });
        }
        if trace.profile().composition().pure_material() != Some(record.material()) {
            return Err(StructuralConstructionError::UnsupportedComposition {
                element,
                material: record.material(),
            });
        }
    }

    let required_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        record.material(),
        record.cross_section(),
        record.length(),
    )
    .map_err(|error| StructuralConstructionError::Geometry { element, error })?;
    if resolution.mass() != required_mass {
        return Err(StructuralConstructionError::MaterialQuantityMismatch {
            element,
            required: required_mass,
            selected: resolution.mass(),
        });
    }

    let source = resolution.selection.source();
    let egress = validate_material_egress_from_selection(state.inventory(), resolution.selection)
        .map_err(|error| match error {
        MaterialEgressError::StaleSelection { expected, actual } => {
            StructuralConstructionError::InventorySelectionStale { expected, actual }
        }
        MaterialEgressError::RevisionExhausted => {
            StructuralConstructionError::InventoryRevisionExhausted
        }
    })?;
    debug_assert_eq!(egress.total_consumed(), required_mass);
    let source_record = state.inventory().get_stockpile(source).ok_or(
        StructuralConstructionError::StructuralLoad(
            StockpileStructuralLoadError::UnknownStockpile { stockpile: source },
        ),
    )?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(required_mass)
        .ok_or(StructuralConstructionError::MaterialQuantityMismatch {
            element,
            required: required_mass,
            selected: source_record.stored_mass(),
        })?;
    let stockpile_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StructuralConstructionError::StructuralLoad)?;
    let expected_structure_revision = state.structures().revision();
    let revision_steps = 1_u64
        + stockpile_load
            .as_ref()
            .map_or(0, ValidatedStockpileStructuralLoad::revision_delta);
    let next_structure_revision = expected_structure_revision
        .checked_add(revision_steps)
        .ok_or(StructuralConstructionError::StructureRevisionExhausted)?;
    let self_weight = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(required_mass),
        registries.core().gravity(),
    )
    .ok_or(StructuralConstructionError::SelfWeightOverflow { element })?;
    Ok(ValidatedStructuralConstruction {
        element,
        expected_structure_revision,
        next_structure_revision,
        material: egress.consumed_inputs().to_vec(),
        self_weight,
        egress,
        stockpile_load,
    })
}

#[cfg(test)]
pub(crate) fn materialize_structural_element_for_test(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: crate::material::FormId,
) {
    use crate::core::quantity::Temperature;
    use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
    use crate::material::CommodityKey;

    let requirement = match resolve_structural_material_requirement(registries, state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("construction test material requirement failed: {error}"),
    };
    let material = requirement.material();
    let mass = requirement.required_mass();
    let source = match add_solid_stockpile_for_test(state, mass) {
        Ok(source) => source,
        Err(error) => panic!("construction test stockpile failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(material, form),
        mass,
        Temperature::from_millikelvin(293_150),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("construction test material deposit failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        state,
        element,
        source,
        &[MaterialLotSelection::new(lot, mass)],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("construction test material binding failed: {error:?}"),
    };
    let token = match validate_structural_construction(registries, state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("construction test validation failed: {error}"),
    };
    if let Err(error) = token.commit(state) {
        panic!("construction test commit failed: {error}");
    }
}

#[cfg(test)]
#[path = "construction_execution_tests.rs"]
mod tests;
