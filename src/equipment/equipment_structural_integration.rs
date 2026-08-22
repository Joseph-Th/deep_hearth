//! Equipment-to-structure support integration; equipment owns support assignment while structural state owns the resulting aggregate load and failure consequences.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysis, StructuralCommitError, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructuralMutationError, StructuralMutationOutcome,
    ValidatedStructuralLoadBatch, ValidatedStructuralMutation,
    calculate_aggregate_weight_force_ceiling, validate_set_owned_structural_load,
    validate_set_owned_structural_loads,
};

use super::{EquipmentDefinitionId, EquipmentId};

/// Failure while resolving one equipment support assignment before any owner mutates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSupportError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownEquipmentDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    AlreadyMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    NotMounted {
        equipment: EquipmentId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingEquipmentLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    EquipmentRevisionExhausted,
    Structure(StructuralMutationError),
}

impl Display for EquipmentSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownEquipmentDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {} while resolving structural support",
                equipment.value(),
                definition.value()
            ),
            Self::AlreadyMounted { equipment, element } => write!(
                formatter,
                "equipment {} is already supported by structural element {}",
                equipment.value(),
                element.value()
            ),
            Self::NotMounted { equipment } => write!(
                formatter,
                "equipment {} has no structural support assignment to remove",
                equipment.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive mounted equipment",
                element.value()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} until tick {} and cannot be moved",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be moved",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be moved",
                equipment.value()
            ),
            Self::AggregateMassOverflow { element } => write!(
                formatter,
                "mounted equipment mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "mounted equipment weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingEquipmentLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN equipment load but equipment ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::Structure(error) => {
                write!(formatter, "structural support change failed: {error}")
            }
        }
    }
}

impl Error for EquipmentSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownEquipment {
                equipment: _equipment,
            }
            | Self::NotMounted {
                equipment: _equipment,
            } => None,
            Self::UnknownEquipmentDefinition {
                equipment: _equipment,
                definition: _definition,
            } => None,
            Self::AlreadyMounted {
                equipment: _equipment,
                element: _element,
            } => None,
            Self::TargetNotActive {
                element: _element,
                lifecycle: _lifecycle,
            } => None,
            Self::EquipmentBusy {
                equipment: _equipment,
                job: _job,
                completes_at: _completes_at,
            } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
            Self::AggregateMassOverflow { element: _element }
            | Self::WeightForceOverflow { element: _element } => None,
            Self::ExistingEquipmentLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
            Self::EquipmentRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound equipment/support transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSupportCommitError {
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    SupportChanged {
        equipment: EquipmentId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated equipment support change expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => {
                write!(
                    formatter,
                    "equipment {} disappeared before support commit",
                    equipment.value()
                )
            }
            Self::SupportChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} support changed from expected {expected:?} to {actual:?} before commit",
                equipment.value()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by production job {} until tick {} before support commit",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before support commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before support commit",
                equipment.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "structural support commit failed: {error}")
            }
        }
    }
}

impl Error for EquipmentSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownEquipment {
                equipment: _equipment,
            } => None,
            Self::SupportChanged {
                equipment: _equipment,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::EquipmentBusy {
                equipment: _equipment,
                job: _job,
                completes_at: _completes_at,
            } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
        }
    }
}

/// Successful support change including any structural damage caused by the equipment load change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSupportOutcome {
    structural: StructuralMutationOutcome,
}

impl EquipmentSupportOutcome {
    #[must_use]
    pub const fn structural_analysis(&self) -> &StructuralAnalysis {
        self.structural.analysis()
    }
}

/// Consumed proof that equipment ownership and the corresponding aggregate structural load agree.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEquipmentSupportChange {
    equipment: EquipmentId,
    before: Option<StructuralElementId>,
    after: Option<StructuralElementId>,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    structural: ValidatedEquipmentStructuralChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValidatedEquipmentStructuralChange {
    Single(ValidatedStructuralMutation),
    Batch(ValidatedStructuralLoadBatch),
}

impl ValidatedEquipmentStructuralChange {
    fn analysis(&self) -> &StructuralAnalysis {
        match self {
            Self::Single(structural) => structural.analysis(),
            Self::Batch(structural) => structural.analysis(),
        }
    }

    fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralMutationOutcome, StructuralCommitError> {
        match self {
            Self::Single(structural) => structural.commit(state),
            Self::Batch(structural) => structural.commit(state),
        }
    }
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

fn resolve_definition_mass(
    registries: &Registries,
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
) -> Result<crate::core::quantity::Mass, EquipmentSupportError> {
    registries
        .equipment()
        .get_equipment(definition)
        .map(|entry| entry.mass())
        .ok_or(EquipmentSupportError::UnknownEquipmentDefinition {
            equipment,
            definition,
        })
}

fn supported_mass(
    registries: &Registries,
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
        let mass = resolve_definition_mass(registries, record.id(), record.definition())?;
        total = total
            .checked_add(AggregateMass::from_mass(mass))
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
    let mass = supported_mass(registries, state, element, None)?;
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
    let equipment_mass = resolve_definition_mass(registries, equipment, record.definition())?;
    let next_mass = current_mass
        .checked_add(AggregateMass::from_mass(equipment_mass))
        .ok_or(EquipmentSupportError::AggregateMassOverflow { element })?;
    let next_load = support_force(registries, element, next_mass)?;
    let structural = validate_set_owned_structural_load(
        registries,
        state,
        element,
        StructuralLoadKind::Equipment,
        next_load,
    )
    .map_err(EquipmentSupportError::Structure)?;
    let (expected_equipment_revision, next_equipment_revision) = next_equipment_revision(state)?;

    Ok(ValidatedEquipmentSupportChange {
        equipment,
        before: None,
        after: Some(element),
        expected_equipment_revision,
        next_equipment_revision,
        structural: ValidatedEquipmentStructuralChange::Single(structural),
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
    let remaining_mass = supported_mass(registries, state, element, Some(equipment))?;
    let next_load = support_force(registries, element, remaining_mass)?;
    let structural = validate_set_owned_structural_load(
        registries,
        state,
        element,
        StructuralLoadKind::Equipment,
        next_load,
    )
    .map_err(EquipmentSupportError::Structure)?;
    let (expected_equipment_revision, next_equipment_revision) = next_equipment_revision(state)?;

    Ok(ValidatedEquipmentSupportChange {
        equipment,
        before: Some(element),
        after: None,
        expected_equipment_revision,
        next_equipment_revision,
        structural: ValidatedEquipmentStructuralChange::Single(structural),
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
    let source_mass = supported_mass(registries, state, source, Some(equipment))?;
    let equipment_mass = resolve_definition_mass(registries, equipment, record.definition())?;
    let target_mass = target_mass
        .checked_add(AggregateMass::from_mass(equipment_mass))
        .ok_or(EquipmentSupportError::AggregateMassOverflow { element: target })?;

    let source_load = support_force(registries, source, source_mass)?;
    let target_load = support_force(registries, target, target_mass)?;
    let structural = validate_set_owned_structural_loads(
        registries,
        state,
        StructuralLoadKind::Equipment,
        BTreeMap::from([(source, source_load), (target, target_load)]),
    )
    .map_err(EquipmentSupportError::Structure)?;
    let structural = match structural {
        Some(structural) => ValidatedEquipmentStructuralChange::Batch(structural),
        None => ValidatedEquipmentStructuralChange::Single(
            validate_set_owned_structural_load(
                registries,
                state,
                target,
                StructuralLoadKind::Equipment,
                target_load,
            )
            .map_err(EquipmentSupportError::Structure)?,
        ),
    };
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
