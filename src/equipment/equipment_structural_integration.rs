//! Coordinates equipment support assignments with structure-owned equipment loads.

use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralCommitError, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructuralMutationError, ValidatedStructuralLoadChange, analyze_structure,
    calculate_aggregate_weight_force_ceiling, validate_owned_structural_load_change,
};

use super::EquipmentId;

mod errors;

pub use errors::{EquipmentSupportCommitError, EquipmentSupportError};

/// Successful support change including any structural damage caused by the equipment load change.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSupportOutcome {
    structural: StructuralAnalysis,
}

impl EquipmentSupportOutcome {
    #[must_use]
    pub const fn structural_analysis(&self) -> &StructuralAnalysis {
        &self.structural
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
    structural: ValidatedEquipmentStructuralChange,
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
struct ValidatedEquipmentStructuralChange {
    structural: ValidatedStructuralLoadChange,
    analysis: StructuralAnalysis,
}

impl ValidatedEquipmentStructuralChange {
    fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }

    fn commit(self, state: &mut AppState) -> Result<StructuralAnalysis, StructuralCommitError> {
        let _ = self.structural.commit(state)?;
        Ok(self.analysis)
    }
}

fn validate_equipment_structural_change(
    registries: &Registries,
    state: &AppState,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedEquipmentStructuralChange, EquipmentSupportError> {
    let structural = validate_owned_structural_load_change(
        registries,
        state,
        StructuralLoadKind::Equipment,
        loads,
    )
    .map_err(EquipmentSupportError::Structure)?;
    let analysis = match structural.analysis() {
        Some(analysis) => analysis.clone(),
        None => analyze_structure(
            registries.structural(),
            registries.materials(),
            state.structures(),
        )
        .map_err(|error| {
            EquipmentSupportError::Structure(StructuralMutationError::Analysis(error))
        })?,
    };
    Ok(ValidatedEquipmentStructuralChange {
        structural,
        analysis,
    })
}

impl ValidatedEquipmentSupportChange {
    #[must_use]
    pub fn structural_analysis(&self) -> &StructuralAnalysis {
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
        if let Some(job) = state.mining().get_equipment_occupant(self.equipment) {
            return Err(EquipmentSupportCommitError::EquipmentBusyMining {
                equipment: self.equipment,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_equipment_occupant(self.equipment)
            .is_some()
        {
            return Err(EquipmentSupportCommitError::EquipmentBusyManualPower {
                equipment: self.equipment,
            });
        }
        if let Some(work) = state
            .player_work()
            .get_prospecting_equipment_occupant(self.equipment)
        {
            return Err(EquipmentSupportCommitError::EquipmentBusyProspecting {
                equipment: self.equipment,
                completes_at: work.completes_at(),
            });
        }
        if let Some(work) = state
            .player_work()
            .get_equipment_maintenance_occupant(self.equipment)
        {
            return Err(EquipmentSupportCommitError::EquipmentUnderMaintenance {
                equipment: self.equipment,
                completes_at: work.completes_at(),
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
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.equipment)
            .filter(|job| !job.is_suspended())
        {
            return Err(EquipmentSupportCommitError::EquipmentBusy {
                equipment: self.equipment,
                job: job.id(),
                completes_at: job.completes_at(),
            });
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
) -> Result<AggregateMass, EquipmentSupportError> {
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
            .ok_or(EquipmentSupportError::AggregateMassOverflow { element })?;
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
    let mass = supported_mass(state, element, None)?;
    let expected = support_force(registries, element, mass)?;
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(EquipmentSupportError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::Equipment);
    if stored != expected {
        return Err(EquipmentSupportError::ExistingEquipmentLoadMismatch {
            element,
            stored,
            expected,
        });
    }
    Ok(mass)
}

fn validate_not_busy(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), EquipmentSupportError> {
    if let Some(job) = state
        .production()
        .get_equipment_occupant(equipment)
        .filter(|job| !job.is_suspended())
    {
        return Err(EquipmentSupportError::EquipmentBusy {
            equipment,
            job: job.id(),
            completes_at: job.completes_at(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentSupportError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentSupportError::EquipmentBusyManualPower { equipment });
    }
    if let Some(work) = state
        .player_work()
        .get_prospecting_equipment_occupant(equipment)
    {
        return Err(EquipmentSupportError::EquipmentBusyProspecting {
            equipment,
            completes_at: work.completes_at(),
        });
    }
    if let Some(work) = state
        .player_work()
        .get_equipment_maintenance_occupant(equipment)
    {
        return Err(EquipmentSupportError::EquipmentUnderMaintenance {
            equipment,
            completes_at: work.completes_at(),
        });
    }
    Ok(())
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
