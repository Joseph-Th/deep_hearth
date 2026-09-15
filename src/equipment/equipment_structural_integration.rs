//! Coordinates equipment support assignments with structure-owned equipment loads.

use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralElementId, StructuralLifecycle, StructuralLoadKind,
    StructuralMutationError, StructuralMutationOutcome, ValidatedStructuralLoadChange,
    calculate_aggregate_weight_force_ceiling, validate_owned_structural_load_change,
};

use super::EquipmentId;

mod availability;
mod errors;

use availability::{support_commit_error, support_validation_error};
pub use errors::{EquipmentSupportCommitError, EquipmentSupportError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentStructuralLoadConsistencyError {
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedEquipmentMassError {
    AggregateMassOverflow { element: StructuralElementId },
}

impl From<SupportedEquipmentMassError> for EquipmentSupportError {
    fn from(error: SupportedEquipmentMassError) -> Self {
        match error {
            SupportedEquipmentMassError::AggregateMassOverflow { element } => {
                Self::AggregateMassOverflow { element }
            }
        }
    }
}

/// Successful support change including any structural damage caused by the equipment load change.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct EquipmentSupportOutcome {
    structural: Option<StructuralMutationOutcome>,
}

impl EquipmentSupportOutcome {
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(StructuralMutationOutcome::analysis)
    }
}

/// Consumed proof that equipment ownership and the corresponding aggregate structural load agree.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedEquipmentSupportChange {
    equipment: EquipmentId,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    structural: ValidatedStructuralLoadChange,
}

fn validate_equipment_structural_change(
    registries: &Registries,
    state: &AppState,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedStructuralLoadChange, EquipmentSupportError> {
    validate_owned_structural_load_change(registries, state, StructuralLoadKind::Equipment, loads)
        .map_err(EquipmentSupportError::Structure)
}

impl ValidatedEquipmentSupportChange {
    /// Returns the precomputed structural consequence when the represented equipment load changes.
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural.analysis()
    }

    /// Commits structural consequences first after prechecking the equipment owner, then performs
    /// the infallible support-field update. Structural commit does not mutate equipment state, so
    /// the prechecked equipment record cannot change within this synchronous call.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentSupportOutcome, EquipmentSupportCommitError> {
        let actual_revision = state.equipment().revision();
        if actual_revision != self.expected_equipment_revision {
            return Err(EquipmentSupportCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_revision,
            });
        }
        let Some(record) = state.equipment().get_equipment(self.equipment) else {
            return Err(EquipmentSupportCommitError::UnknownEquipment {
                equipment: self.equipment,
            });
        };
        if record.supported_by() != self.before {
            return Err(EquipmentSupportCommitError::SupportChanged {
                equipment: self.equipment,
                expected: self.before,
                actual: record.supported_by(),
            });
        }
        if let Some(error) = support_commit_error(state, self.equipment) {
            return Err(error);
        }
        state.equipment().assert_support_change_available(
            self.equipment,
            self.before,
            self.after,
            self.next_equipment_revision,
        );

        let structural = self
            .structural
            .commit(state)
            .map_err(EquipmentSupportCommitError::Structure)?;

        state.equipment_state_mut().apply_support_change(
            self.equipment,
            self.before,
            self.after,
            self.next_equipment_revision,
        );
        Ok(EquipmentSupportOutcome { structural })
    }
}

fn supported_mass(
    state: &AppState,
    element: StructuralElementId,
    excluded: Option<EquipmentId>,
) -> Result<AggregateMass, SupportedEquipmentMassError> {
    let mut total = AggregateMass::ZERO;
    for equipment in state.equipment().supported_equipment(element) {
        if excluded == Some(equipment) {
            continue;
        }
        let record = match state.equipment().get_equipment(equipment) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: support index references missing equipment {}",
                equipment.value()
            ),
        };
        total = total
            .checked_add(AggregateMass::from_mass(record.embodied_mass()))
            .ok_or(SupportedEquipmentMassError::AggregateMassOverflow { element })?;
    }
    Ok(total)
}

fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass: AggregateMass,
) -> Result<Force, EquipmentSupportError> {
    calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(EquipmentSupportError::WeightForceOverflow { element })
}

fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<AggregateMass, EquipmentSupportError> {
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(EquipmentSupportError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::Equipment);
    validate_existing_equipment_structural_load(registries, state, element, stored).map_err(
        |error| match error {
            EquipmentStructuralLoadConsistencyError::AggregateMassOverflow { element } => {
                EquipmentSupportError::AggregateMassOverflow { element }
            }
            EquipmentStructuralLoadConsistencyError::WeightForceOverflow { element } => {
                EquipmentSupportError::WeightForceOverflow { element }
            }
            EquipmentStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => EquipmentSupportError::ExistingEquipmentLoadMismatch {
                element,
                stored,
                expected,
            },
        },
    )
}

pub(crate) fn validate_existing_equipment_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    stored: Force,
) -> Result<AggregateMass, EquipmentStructuralLoadConsistencyError> {
    let mass = supported_mass(state, element, None).map_err(|error| match error {
        SupportedEquipmentMassError::AggregateMassOverflow { element } => {
            EquipmentStructuralLoadConsistencyError::AggregateMassOverflow { element }
        }
    })?;
    let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(EquipmentStructuralLoadConsistencyError::WeightForceOverflow { element })?;
    if stored != expected {
        return Err(
            EquipmentStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            },
        );
    }
    Ok(mass)
}

fn validate_not_busy(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), EquipmentSupportError> {
    support_validation_error(state, equipment).map_or(Ok(()), Err)
}

fn next_equipment_revision(state: &AppState) -> Result<(u64, u64), EquipmentSupportError> {
    let current = state.equipment().revision();
    let next = current
        .checked_add(1)
        .ok_or(EquipmentSupportError::EquipmentRevisionExhausted)?;
    Ok((current, next))
}

/// Validates placing existing equipment on one active structural member and resolves the resulting
/// aggregate equipment load, including any crack or collapse cascade, without mutating either owner.
pub fn validate_mount_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    element: StructuralElementId,
) -> Result<ValidatedEquipmentSupportChange, EquipmentSupportError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentSupportError::UnknownEquipment { equipment })?;
    if let Some(existing) = record.supported_by() {
        return Err(EquipmentSupportError::AlreadyMounted {
            equipment,
            element: existing,
        });
    }
    validate_not_busy(state, equipment)?;

    let target =
        state
            .structures()
            .get_element(element)
            .ok_or(EquipmentSupportError::Structure(
                StructuralMutationError::UnknownElement { element },
            ))?;
    if target.lifecycle() != StructuralLifecycle::Active {
        return Err(EquipmentSupportError::TargetNotActive {
            element,
            lifecycle: target.lifecycle(),
        });
    }

    let current_mass = validate_existing_load(registries, state, element)?;
    let next_mass = current_mass
        .checked_add(AggregateMass::from_mass(record.embodied_mass()))
        .ok_or(EquipmentSupportError::AggregateMassOverflow { element })?;
    let next_load = support_force(registries, element, next_mass)?;
    let structural = validate_equipment_structural_change(
        registries,
        state,
        BTreeMap::from([(element, next_load)]),
    )?;
    let (expected_equipment_revision, next_equipment_revision) = next_equipment_revision(state)?;

    Ok(ValidatedEquipmentSupportChange {
        equipment,
        before: None,
        after: Some(element),
        expected_equipment_revision,
        next_equipment_revision,
        structural,
    })
}

/// Validates removing an equipment support assignment. Failed structural debris may be unloaded;
/// unloading never repairs already-persisted structural damage.
pub fn validate_unmount_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
) -> Result<ValidatedEquipmentSupportChange, EquipmentSupportError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentSupportError::UnknownEquipment { equipment })?;
    let element = record
        .supported_by()
        .ok_or(EquipmentSupportError::NotMounted { equipment })?;
    validate_not_busy(state, equipment)?;
    if state.structures().get_element(element).is_none() {
        return Err(EquipmentSupportError::Structure(
            StructuralMutationError::UnknownElement { element },
        ));
    }

    validate_existing_load(registries, state, element)?;
    let remaining_mass = supported_mass(state, element, Some(equipment))?;
    let next_load = support_force(registries, element, remaining_mass)?;
    let structural = validate_equipment_structural_change(
        registries,
        state,
        BTreeMap::from([(element, next_load)]),
    )?;
    let (expected_equipment_revision, next_equipment_revision) = next_equipment_revision(state)?;

    Ok(ValidatedEquipmentSupportChange {
        equipment,
        before: Some(element),
        after: None,
        expected_equipment_revision,
        next_equipment_revision,
        structural,
    })
}

/// Validates moving already-mounted equipment directly to another active structural member.
///
/// Source unloading and target loading are resolved as one structural batch, so callers can inspect
/// the real relocation consequence before committing and never need to unmount speculatively.
pub fn validate_relocate_equipment(
    registries: &Registries,
    state: &AppState,
    equipment: EquipmentId,
    target: StructuralElementId,
) -> Result<ValidatedEquipmentSupportChange, EquipmentSupportError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentSupportError::UnknownEquipment { equipment })?;
    let source = record
        .supported_by()
        .ok_or(EquipmentSupportError::NotMounted { equipment })?;
    if source == target {
        return Err(EquipmentSupportError::AlreadyMounted {
            equipment,
            element: source,
        });
    }
    validate_not_busy(state, equipment)?;

    let target_record =
        state
            .structures()
            .get_element(target)
            .ok_or(EquipmentSupportError::Structure(
                StructuralMutationError::UnknownElement { element: target },
            ))?;
    if target_record.lifecycle() != StructuralLifecycle::Active {
        return Err(EquipmentSupportError::TargetNotActive {
            element: target,
            lifecycle: target_record.lifecycle(),
        });
    }

    validate_existing_load(registries, state, source)?;
    let target_mass = validate_existing_load(registries, state, target)?;
    let source_mass = supported_mass(state, source, Some(equipment))?;
    let target_mass = target_mass
        .checked_add(AggregateMass::from_mass(record.embodied_mass()))
        .ok_or(EquipmentSupportError::AggregateMassOverflow { element: target })?;

    let source_load = support_force(registries, source, source_mass)?;
    let target_load = support_force(registries, target, target_mass)?;
    let structural = validate_equipment_structural_change(
        registries,
        state,
        BTreeMap::from([(source, source_load), (target, target_load)]),
    )?;
    let (expected_equipment_revision, next_equipment_revision) = next_equipment_revision(state)?;

    Ok(ValidatedEquipmentSupportChange {
        equipment,
        before: Some(source),
        after: Some(target),
        expected_equipment_revision,
        next_equipment_revision,
        structural,
    })
}

#[cfg(test)]
#[path = "equipment_structural_integration_tests.rs"]
mod tests;
