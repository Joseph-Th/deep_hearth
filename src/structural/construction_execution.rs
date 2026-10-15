//! Geometry-constrained materialization of planned structural members from exact inventory matter.
//!
//! This owner moves rigid material into a planned member and establishes self-weight. Player labor,
//! locality, duration, and final activation are coordinated by the ordinary construction wrapper.

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::inventory::{
    ConsumedMaterialTrace, ConsumptionSelection, MaterialEgressError, StockpileStoredMassChange,
    ValidatedMaterialEgress, ValidatedStockpileStructuralLoad, apply_material_egress,
    validate_material_egress_from_selection, validate_stockpile_stored_mass_changes,
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

pub use binding::StructuralConstructionBindingError;
pub(crate) use binding::{
    StructuralConstructionResolution, bind_structural_construction_selection,
};
pub use errors::{StructuralConstructionCommitError, StructuralConstructionError};
pub(crate) use requirement::resolve_required_mass;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use requirement::resolve_structural_material_requirement;

/// Consumed proof that exact inventory matter can become one member's embodied matter atomically.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStructuralConstruction {
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

    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), StructuralConstructionCommitError> {
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
        Ok(())
    }

    /// Prechecks a construction token against the snapshot immediately before its planned member
    /// is inserted by the same atomic player action.
    ///
    /// This narrow path is only valid for carried material, so no structure-owned source-stockpile
    /// load mutation may accompany the egress. The member record and projected revision are supplied
    /// by the already-validated player planning token rather than being read from authoritative state.
    pub(crate) fn precheck_before_planned_member(
        &self,
        state: &AppState,
        record: &StructuralElementRecord,
        projected_structure_revision: u64,
    ) -> Result<(), StructuralConstructionCommitError> {
        let actual_projected_revision = state.structures().revision().checked_add(1).ok_or(
            StructuralConstructionCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: state.structures().revision(),
            },
        )?;
        if actual_projected_revision != projected_structure_revision
            || projected_structure_revision != self.expected_structure_revision
        {
            return Err(StructuralConstructionCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_projected_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.egress.expected_revision() {
            return Err(StructuralConstructionCommitError::StaleInventoryRevision {
                expected: self.egress.expected_revision(),
                actual: actual_inventory_revision,
            });
        }
        if record.id() != self.element
            || record.lifecycle() != StructuralLifecycle::Planned
            || !record.embodied_mass().is_zero()
        {
            return Err(StructuralConstructionCommitError::StateChanged {
                element: self.element,
            });
        }
        assert!(
            self.stockpile_load.is_none(),
            "atomic planned-member construction must consume player-carried, unsupported material"
        );
        self.egress.assert_matches_state(state.inventory());
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        if let Some(stockpile_load) = self.stockpile_load {
            stockpile_load.commit(state).unwrap_or_else(|error| {
                panic!("prechecked structural source load changed: {error}")
            });
        }
        apply_material_egress(state.inventory_state_mut(), self.egress);
        let structures = state.structure_state_mut();
        structures.set_embodied_matter(self.element, self.material, self.self_weight);
        structures.apply_revision(self.next_structure_revision);
    }

    /// Commits both owners only after rechecking both revisions and the target lifecycle.
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<(), StructuralConstructionCommitError> {
        self.precheck(state)?;
        self.apply(state);
        Ok(())
    }
}

/// Validates materialization for a member that will be inserted immediately before construction
/// starts in the same atomic player action.
///
/// The source must be the player's carried stockpile, which has no structural support load. That
/// lets this validator bind the post-plan structural revision without cloning or temporarily
/// mutating the world state.
pub(crate) fn validate_structural_construction_for_planned_member(
    registries: &Registries,
    state: &AppState,
    record: &StructuralElementRecord,
    expected_structure_revision: u64,
    resolution: StructuralConstructionResolution,
) -> Result<ValidatedStructuralConstruction, StructuralConstructionError> {
    let element = resolution.element();
    if record.id() != element {
        return Err(StructuralConstructionError::UnknownElement { element });
    }
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
    let selection = resolution.into_selection();
    let source = selection.source();
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            unreachable!("validated planned-member construction requires admitted player logistics")
        });
    assert_eq!(
        source, carried,
        "planned-member atomic construction must bind player-carried material"
    );
    assert!(
        state
            .inventory()
            .get_stockpile(carried)
            .is_some_and(|stockpile| stockpile.supported_by().is_none()),
        "player-carried stockpile cannot be structurally mounted"
    );
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
    if !state.can_spend_inventory_revisions(1) {
        return Err(StructuralConstructionError::InventoryRevisionExhausted);
    }
    let current_structure_revision = state.structures().revision();
    let projected = current_structure_revision
        .checked_add(1)
        .ok_or(StructuralConstructionError::StructureRevisionExhausted)?;
    if projected != expected_structure_revision || !state.can_spend_structure_revisions(2) {
        return Err(StructuralConstructionError::StructureRevisionExhausted);
    }
    let next_structure_revision = projected
        .checked_add(1)
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
        stockpile_load: None,
    })
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
    let source_after = egress.source_stored_mass_after(state.inventory());
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
    if !state.can_spend_structure_revisions(revision_steps) {
        return Err(StructuralConstructionError::StructureRevisionExhausted);
    }
    let next = expected.checked_add(revision_steps).unwrap_or_else(|| {
        unreachable!("structural headroom check includes construction revisions")
    });
    Ok((expected, next))
}

/// Validates a physically resolved material batch for one still-planned member.
pub(crate) fn validate_structural_construction(
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
        required_mass,
        resolution.into_selection(),
    )?;
    if !state.can_spend_inventory_revisions(1) {
        return Err(StructuralConstructionError::InventoryRevisionExhausted);
    }
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
