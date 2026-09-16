//! Fixture-only geometry-constrained materialization of planned structural members.
//!
//! Member geometry and material density determine the exact conservative solid-mass requirement. This
//! module exists to create physically valid controlled test/gameplay-audit starting states. It is not a
//! player construction system and does not authorize labor, tools, joints, cutting/placement waste, or
//! build duration.

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, MaterialEgressError, StockpileStoredMassChange,
    StockpileStructuralLoadError, ValidatedMaterialEgress, ValidatedStockpileStructuralLoad,
    apply_material_egress, validate_material_egress_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::material::MaterialId;
use crate::registry::Registries;

use super::load::calculate_aggregate_weight_force_ceiling;
#[cfg(test)]
use super::state::StructuralLoadKind;
use super::state::{StructuralElementId, StructuralElementRecord, StructuralLifecycle};

mod binding;
mod errors;
mod requirement;

use binding::StructuralConstructionResolution;
pub(crate) use binding::bind_structural_construction_selection;
pub use errors::{StructuralConstructionCommitError, StructuralConstructionError};
use requirement::resolve_required_mass;
pub(crate) use requirement::resolve_structural_material_requirement;

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
        self.egress.assert_matches_state(state.inventory());

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

fn validate_structural_construction_target(
    registries: &Registries,
    element: StructuralElementId,
    record: &StructuralElementRecord,
) -> Result<(), StructuralConstructionError> {
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
    Ok(())
}

fn validate_structural_construction_material(
    registries: &Registries,
    element: StructuralElementId,
    expected_material: MaterialId,
    traces: &[ConsumedMaterialTrace],
) -> Result<(), StructuralConstructionError> {
    for trace in traces {
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
        if found != expected_material {
            return Err(StructuralConstructionError::MaterialMismatch {
                element,
                expected: expected_material,
                found,
            });
        }
        if trace.profile().composition().pure_material() != Some(expected_material) {
            return Err(StructuralConstructionError::UnsupportedComposition {
                element,
                material: expected_material,
            });
        }
    }
    Ok(())
}

fn validate_structural_construction_egress(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    required_mass: Mass,
    selection: ConsumptionSelection,
) -> Result<
    (
        ValidatedMaterialEgress,
        Option<ValidatedStockpileStructuralLoad>,
    ),
    StructuralConstructionError,
> {
    let source = selection.source();
    let egress =
        validate_material_egress_from_selection(state.inventory(), selection).map_err(|error| {
            match error {
                MaterialEgressError::StaleSelection { expected, actual } => {
                    StructuralConstructionError::InventorySelectionStale { expected, actual }
                }
                MaterialEgressError::RevisionExhausted => {
                    StructuralConstructionError::InventoryRevisionExhausted
                }
            }
        })?;
    assert_eq!(
        egress.total_consumed(),
        required_mass,
        "validated structural construction must consume its exact geometry-derived material mass"
    );
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
    Ok((egress, stockpile_load))
}

fn resolve_structural_construction_revision(
    state: &AppState,
    stockpile_load: Option<&ValidatedStockpileStructuralLoad>,
) -> Result<(u64, u64), StructuralConstructionError> {
    let expected = state.structures().revision();
    let revision_steps =
        1_u64 + stockpile_load.map_or(0, ValidatedStockpileStructuralLoad::revision_delta);
    let next = expected
        .checked_add(revision_steps)
        .ok_or(StructuralConstructionError::StructureRevisionExhausted)?;
    Ok((expected, next))
}

/// Validates a physically resolved material batch for one still-planned member.
pub fn validate_structural_construction(
    registries: &Registries,
    state: &AppState,
    resolution: StructuralConstructionResolution,
) -> Result<ValidatedStructuralConstruction, StructuralConstructionError> {
    let element = resolution.element();
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralConstructionError::UnknownElement { element })?;
    validate_structural_construction_target(registries, element, record)?;
    validate_structural_construction_material(
        registries,
        element,
        record.material(),
        resolution.selection().consumed_inputs(),
    )?;

    let required_mass = resolve_required_mass(registries.materials(), record)
        .map_err(|error| StructuralConstructionError::Geometry { element, error })?;
    if resolution.mass() != required_mass {
        return Err(StructuralConstructionError::MaterialQuantityMismatch {
            element,
            required: required_mass,
            selected: resolution.mass(),
        });
    }

    let (egress, stockpile_load) = validate_structural_construction_egress(
        registries,
        state,
        element,
        required_mass,
        resolution.into_selection(),
    )?;
    let (expected_structure_revision, next_structure_revision) =
        resolve_structural_construction_revision(state, stockpile_load.as_ref())?;
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
#[path = "construction_execution_tests.rs"]
mod tests;
