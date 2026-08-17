//! Equipment-to-structure support integration; equipment owns support assignment while structural state owns the resulting aggregate load and failure consequences.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{AggregateMass, Force};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
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
            Self::UnknownEquipment { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::AlreadyMounted { .. }
            | Self::NotMounted { .. }
            | Self::TargetNotActive { .. }
            | Self::EquipmentBusy { .. }
            | Self::AggregateMassOverflow { .. }
            | Self::WeightForceOverflow { .. }
            | Self::ExistingEquipmentLoadMismatch { .. }
            | Self::EquipmentRevisionExhausted => None,
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
            Self::StaleEquipmentRevision { .. }
            | Self::UnknownEquipment { .. }
            | Self::SupportChanged { .. }
            | Self::EquipmentBusy { .. } => None,
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
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        make_test_registries_with_equipment,
    };
    use crate::core::quantity::{Area, Mass};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::equipment::{EquipmentDefinition, add_equipment};
    use crate::maintenance::{Condition, MaintenanceThresholds};
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralDamageEvent, StructuralMutationError, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
        validate_remove_structural_element, validate_set_structural_load,
    };

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(830_001);
    const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(830_001);

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("equipment support condition fixture failed: {error}"),
        }
    }

    fn make_registries(equipment_mass: Mass) -> Registries {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(1)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("equipment support capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("equipment support thresholds fixture failed: {error}"),
        };
        make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "equipment support fixture capability",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "equipment support fixture",
                equipment_mass,
                profile,
                thresholds,
            ),
        )
    }

    fn make_bounds(x: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("equipment support bounds fixture failed: {error}"),
        }
    }

    fn add_member(registries: &Registries, state: &mut AppState, x: i64) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                make_bounds(x),
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("equipment support member fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        element
    }

    fn activate_member(
        registries: &Registries,
        state: &mut AppState,
        element: StructuralElementId,
    ) {
        let token = match validate_activate_structural_element(registries, state, element) {
            Ok(token) => token,
            Err(error) => panic!("equipment support activation fixture failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("equipment support activation commit failed: {error}");
        }
    }

    fn add_test_equipment(registries: &Registries, state: &mut AppState) -> EquipmentId {
        match add_equipment(registries, state, TEST_DEFINITION, Condition::PRISTINE) {
            Ok(equipment) => equipment,
            Err(error) => panic!("equipment support equipment fixture failed: {error}"),
        }
    }

    fn commit_support(
        token: ValidatedEquipmentSupportChange,
        state: &mut AppState,
    ) -> EquipmentSupportOutcome {
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("equipment support commit failed: {error}"),
        }
    }

    #[test]
    fn multiple_equipment_records_aggregate_one_structural_load_without_rounding_per_record() {
        let registries = make_registries(Mass::from_milligrams(1));
        let mut state = AppState::new(WorldSeed::new(0x8300_0001));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let first = add_test_equipment(&registries, &mut state);
        let second = add_test_equipment(&registries, &mut state);

        let first_mount = match validate_mount_equipment(&registries, &state, first, member) {
            Ok(token) => token,
            Err(error) => panic!("first equipment mount validation failed: {error}"),
        };
        commit_support(first_mount, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(member)
                .map(|record| { record.load(StructuralLoadKind::Equipment) }),
            Some(Force::from_millinewtons(1))
        );

        let second_mount = match validate_mount_equipment(&registries, &state, second, member) {
            Ok(token) => token,
            Err(error) => panic!("second equipment mount validation failed: {error}"),
        };
        commit_support(second_mount, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(member)
                .map(|record| { record.load(StructuralLoadKind::Equipment) }),
            Some(Force::from_millinewtons(1))
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        let first_unmount = match validate_unmount_equipment(&registries, &state, first) {
            Ok(token) => token,
            Err(error) => panic!("first equipment unmount validation failed: {error}"),
        };
        commit_support(first_unmount, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(member)
                .map(|record| { record.load(StructuralLoadKind::Equipment) }),
            Some(Force::from_millinewtons(1))
        );

        let second_unmount = match validate_unmount_equipment(&registries, &state, second) {
            Ok(token) => token,
            Err(error) => panic!("second equipment unmount validation failed: {error}"),
        };
        commit_support(second_unmount, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(member)
                .map(|record| { record.load(StructuralLoadKind::Equipment) }),
            Some(Force::ZERO)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn relocation_remains_revision_bound_when_force_rounding_hides_both_load_deltas() {
        let registries = make_registries(Mass::from_milligrams(1));
        let mut state = AppState::new(WorldSeed::new(0x8300_0012));
        let source = add_member(&registries, &mut state, 0);
        let target = add_member(&registries, &mut state, 2);
        activate_member(&registries, &mut state, source);
        activate_member(&registries, &mut state, target);
        let moved = add_test_equipment(&registries, &mut state);
        let source_peer = add_test_equipment(&registries, &mut state);
        let target_peer = add_test_equipment(&registries, &mut state);
        for (equipment, support) in [
            (moved, source),
            (source_peer, source),
            (target_peer, target),
        ] {
            let mount = validate_mount_equipment(&registries, &state, equipment, support)
                .unwrap_or_else(|error| panic!("rounding relocation mount failed: {error}"));
            commit_support(mount, &mut state);
        }
        let source_load = state
            .structures()
            .get_element(source)
            .map(|record| record.load(StructuralLoadKind::Equipment))
            .unwrap_or_else(|| panic!("rounding relocation source disappeared"));
        let target_load = state
            .structures()
            .get_element(target)
            .map(|record| record.load(StructuralLoadKind::Equipment))
            .unwrap_or_else(|| panic!("rounding relocation target disappeared"));
        assert_eq!(source_load, Force::from_millinewtons(1));
        assert_eq!(target_load, Force::from_millinewtons(1));
        let structural_revision = state.structures().revision();

        let relocation = validate_relocate_equipment(&registries, &state, moved, target)
            .unwrap_or_else(|error| panic!("rounding relocation validation failed: {error}"));
        commit_support(relocation, &mut state);

        assert_eq!(state.structures().revision(), structural_revision + 1);
        assert_eq!(
            state
                .structures()
                .get_element(source)
                .map(|record| record.load(StructuralLoadKind::Equipment)),
            Some(source_load)
        );
        assert_eq!(
            state
                .structures()
                .get_element(target)
                .map(|record| record.load(StructuralLoadKind::Equipment)),
            Some(target_load)
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(moved)
                .and_then(|record| record.supported_by()),
            Some(target)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn relocation_moves_equipment_and_structural_load_as_one_transaction() {
        let registries = make_registries(Mass::from_milligrams(3_600_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0010));
        let source = add_member(&registries, &mut state, 0);
        let target = add_member(&registries, &mut state, 2);
        activate_member(&registries, &mut state, source);
        activate_member(&registries, &mut state, target);
        let equipment = add_test_equipment(&registries, &mut state);
        let mount = validate_mount_equipment(&registries, &state, equipment, source)
            .unwrap_or_else(|error| panic!("relocation source mount failed: {error}"));
        commit_support(mount, &mut state);
        let source_load = state
            .structures()
            .get_element(source)
            .map(|record| record.load(StructuralLoadKind::Equipment))
            .unwrap_or_else(|| panic!("relocation source support disappeared"));

        let relocation = validate_relocate_equipment(&registries, &state, equipment, target)
            .unwrap_or_else(|error| panic!("equipment relocation validation failed: {error}"));
        assert!(
            relocation
                .structural_analysis()
                .assessments()
                .iter()
                .any(|assessment| assessment.element() == target)
        );
        commit_support(relocation, &mut state);

        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .and_then(|record| record.supported_by()),
            Some(target)
        );
        assert_eq!(
            state
                .structures()
                .get_element(source)
                .map(|record| record.load(StructuralLoadKind::Equipment)),
            Some(Force::ZERO)
        );
        assert_eq!(
            state
                .structures()
                .get_element(target)
                .map(|record| record.load(StructuralLoadKind::Equipment)),
            Some(source_load)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn stale_relocation_leaves_equipment_on_original_support() {
        let registries = make_registries(Mass::from_milligrams(3_600_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0011));
        let source = add_member(&registries, &mut state, 0);
        let target = add_member(&registries, &mut state, 2);
        activate_member(&registries, &mut state, source);
        activate_member(&registries, &mut state, target);
        let equipment = add_test_equipment(&registries, &mut state);
        let mount = validate_mount_equipment(&registries, &state, equipment, source)
            .unwrap_or_else(|error| panic!("stale relocation source mount failed: {error}"));
        commit_support(mount, &mut state);
        let relocation = validate_relocate_equipment(&registries, &state, equipment, target)
            .unwrap_or_else(|error| panic!("stale relocation validation failed: {error}"));

        let snow = validate_set_structural_load(
            &registries,
            &state,
            target,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        )
        .unwrap_or_else(|error| panic!("stale relocation structure mutation failed: {error}"));
        snow.commit(&mut state)
            .unwrap_or_else(|error| panic!("stale relocation structure commit failed: {error}"));

        assert!(matches!(
            relocation.commit(&mut state),
            Err(EquipmentSupportCommitError::Structure(
                StructuralCommitError::StaleRevision { .. }
            ))
        ));
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .and_then(|record| record.supported_by()),
            Some(source)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn heavy_equipment_cracks_support_and_unloading_does_not_repair_damage() {
        let registries = make_registries(Mass::from_milligrams(3_600_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0002));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let equipment = add_test_equipment(&registries, &mut state);

        let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
            Ok(token) => token,
            Err(error) => panic!("heavy equipment mount validation failed: {error}"),
        };
        assert!(matches!(
            mount.structural_analysis().damage_events(),
            [StructuralDamageEvent::Cracked { element, .. }] if *element == member
        ));
        let outcome = commit_support(mount, &mut state);
        assert!(matches!(
            outcome.structural_analysis().damage_events(),
            [StructuralDamageEvent::Cracked { element, .. }] if *element == member
        ));
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .and_then(|record| record.supported_by()),
            Some(member)
        );
        let member_record = match state.structures().get_element(member) {
            Some(record) => record,
            None => panic!("heavy equipment support disappeared"),
        };
        assert!(member_record.is_cracked());
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

        assert_eq!(
            validate_remove_structural_element(&registries, &state, member),
            Err(StructuralMutationError::ElementSupportsEquipment {
                element: member,
                equipment,
            })
        );

        let unmount = match validate_unmount_equipment(&registries, &state, equipment) {
            Ok(token) => token,
            Err(error) => panic!("heavy equipment unmount validation failed: {error}"),
        };
        let outcome = commit_support(unmount, &mut state);
        assert!(outcome.structural_analysis().damage_events().is_empty());
        let member_record = match state.structures().get_element(member) {
            Some(record) => record,
            None => panic!("unloaded cracked support disappeared"),
        };
        assert!(member_record.is_cracked());
        assert_eq!(
            member_record.load(StructuralLoadKind::Equipment),
            Force::ZERO
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn failed_support_can_be_unloaded_without_resurrecting_it() {
        let registries = make_registries(Mass::from_milligrams(4_100_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0003));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let equipment = add_test_equipment(&registries, &mut state);

        let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
            Ok(token) => token,
            Err(error) => panic!("failing equipment mount validation failed: {error}"),
        };
        commit_support(mount, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(member)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );

        let unmount = match validate_unmount_equipment(&registries, &state, equipment) {
            Ok(token) => token,
            Err(error) => panic!("failed-support unmount validation failed: {error}"),
        };
        commit_support(unmount, &mut state);
        let record = match state.structures().get_element(member) {
            Some(record) => record,
            None => panic!("failed support disappeared while unloading"),
        };
        assert_eq!(record.lifecycle(), StructuralLifecycle::Failed);
        assert!(record.is_cracked());
        assert_eq!(record.load(StructuralLoadKind::Equipment), Force::ZERO);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn stale_equipment_revision_rejects_mount_without_structural_mutation() {
        let registries = make_registries(Mass::from_milligrams(1_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0004));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let equipment = add_test_equipment(&registries, &mut state);
        let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
            Ok(token) => token,
            Err(error) => panic!("stale mount validation failed: {error}"),
        };
        let expected_revision = state.equipment().revision();
        add_test_equipment(&registries, &mut state);
        let before = state.clone();

        assert_eq!(
            mount.commit(&mut state),
            Err(EquipmentSupportCommitError::StaleEquipmentRevision {
                expected: expected_revision,
                actual: expected_revision + 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn stale_structural_revision_rejects_mount_without_equipment_mutation() {
        let registries = make_registries(Mass::from_milligrams(1_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0006));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let equipment = add_test_equipment(&registries, &mut state);
        let expected_structure_revision = state.structures().revision();
        let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
            Ok(token) => token,
            Err(error) => panic!("stale-structure mount validation failed: {error}"),
        };
        let snow = match validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(1),
        ) {
            Ok(token) => token,
            Err(error) => panic!("intervening structural mutation validation failed: {error}"),
        };
        if let Err(error) = snow.commit(&mut state) {
            panic!("intervening structural mutation commit failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            mount.commit(&mut state),
            Err(EquipmentSupportCommitError::Structure(
                StructuralCommitError::StaleRevision {
                    expected: expected_structure_revision,
                    actual: expected_structure_revision + 1,
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn equipment_load_channel_rejects_direct_structural_writes() {
        let registries = make_registries(Mass::from_milligrams(1_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0007));
        let member = add_member(&registries, &mut state, 0);
        activate_member(&registries, &mut state, member);
        let before = state.clone();

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                member,
                StructuralLoadKind::Equipment,
                Force::from_millinewtons(1),
            ),
            Err(StructuralMutationError::LoadOwnedBySubsystem {
                kind: StructuralLoadKind::Equipment,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn mounting_requires_an_active_structural_target() {
        let registries = make_registries(Mass::from_milligrams(1_000_000));
        let mut state = AppState::new(WorldSeed::new(0x8300_0005));
        let planned = add_member(&registries, &mut state, 0);
        let equipment = add_test_equipment(&registries, &mut state);
        let before = state.clone();

        assert_eq!(
            validate_mount_equipment(&registries, &state, equipment, planned),
            Err(EquipmentSupportError::TargetNotActive {
                element: planned,
                lifecycle: StructuralLifecycle::Planned,
            })
        );
        assert_eq!(state, before);
    }
}
