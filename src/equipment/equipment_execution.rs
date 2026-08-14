//! Canonical equipment creation and revision-checked condition mutation for sibling persistent state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::maintenance::{Condition, ConditionPlan, decide_repair, decide_wear};
use crate::production::ProductionJobId;
use crate::registry::Registries;

use super::definitions::EquipmentDefinitionId;
use super::state::{EquipmentId, EquipmentRecord};

/// Failure while allocating one persistent equipment instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddEquipmentError {
    UnknownDefinition { definition: EquipmentDefinitionId },
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddEquipmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown equipment definition {}",
                definition.value()
            ),
            Self::IdExhausted => formatter.write_str("equipment identifier space is exhausted"),
            Self::RevisionExhausted => formatter.write_str("equipment revision space is exhausted"),
        }
    }
}

impl Error for AddEquipmentError {}

/// Adds one equipment record after all identity and registry checks succeed.
pub fn add_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
    condition: Condition,
) -> Result<EquipmentId, AddEquipmentError> {
    if registries.equipment().get_equipment(definition).is_none() {
        return Err(AddEquipmentError::UnknownDefinition { definition });
    }

    let equipment_state = state.equipment_state();
    let id = EquipmentId::new(equipment_state.next_equipment_id);
    let next_equipment_id = equipment_state
        .next_equipment_id
        .checked_add(1)
        .ok_or(AddEquipmentError::IdExhausted)?;
    let next_revision = equipment_state
        .revision
        .checked_add(1)
        .ok_or(AddEquipmentError::RevisionExhausted)?;
    let record = EquipmentRecord {
        id,
        definition,
        condition,
        supported_by: None,
        created_at: state.tick(),
    };

    let equipment_state = state.equipment_state_mut();
    let replaced = equipment_state.records.insert(id, record);
    debug_assert!(
        replaced.is_none(),
        "Runtime Invariant 4 (Index Uniqueness): equipment allocation replaced an existing id"
    );
    equipment_state.next_equipment_id = next_equipment_id;
    equipment_state.revision = next_revision;
    Ok(id)
}

/// Planned equipment condition transition bound to the state revision it observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentConditionPlan {
    equipment: EquipmentId,
    expected_revision: u64,
    next_revision: u64,
    transition: ConditionPlan,
}

impl EquipmentConditionPlan {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn before(self) -> Condition {
        self.transition.before()
    }

    #[must_use]
    pub const fn after(self) -> Condition {
        self.transition.after()
    }
}

/// Failure while deciding a condition change against current authoritative state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentConditionPlanError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
    },
    RevisionExhausted,
}

impl Display for EquipmentConditionPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} until tick {}",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
            Self::RevisionExhausted => formatter.write_str("equipment revision space is exhausted"),
        }
    }
}

impl Error for EquipmentConditionPlanError {}

fn decide_condition_change(
    state: &AppState,
    equipment: EquipmentId,
    decide: impl FnOnce(Condition) -> ConditionPlan,
) -> Result<EquipmentConditionPlan, EquipmentConditionPlanError> {
    let equipment_state = state.equipment_state();
    let Some(record) = equipment_state.get_equipment(equipment) else {
        return Err(EquipmentConditionPlanError::UnknownEquipment { equipment });
    };
    if let Some(job) = state.production().jobs().find(|job| {
        job.equipment_provider()
            .is_some_and(|provider| provider.equipment() == equipment)
    }) {
        return Err(EquipmentConditionPlanError::EquipmentBusy {
            equipment,
            job: job.id(),
            completes_at: job.completes_at(),
        });
    }
    let next_revision = equipment_state
        .revision
        .checked_add(1)
        .ok_or(EquipmentConditionPlanError::RevisionExhausted)?;
    Ok(EquipmentConditionPlan {
        equipment,
        expected_revision: equipment_state.revision,
        next_revision,
        transition: decide(record.condition()),
    })
}

/// Decides deterministic wear without mutating the equipment record.
pub fn decide_equipment_wear(
    state: &AppState,
    equipment: EquipmentId,
    wear_ppm: u32,
) -> Result<EquipmentConditionPlan, EquipmentConditionPlanError> {
    decide_condition_change(state, equipment, |condition| {
        decide_wear(condition, wear_ppm)
    })
}

/// Decides deterministic repair without mutating the equipment record.
pub fn decide_equipment_repair(
    state: &AppState,
    equipment: EquipmentId,
    repair_ppm: u32,
) -> Result<EquipmentConditionPlan, EquipmentConditionPlanError> {
    decide_condition_change(state, equipment, |condition| {
        decide_repair(condition, repair_ppm)
    })
}

/// Failure to commit a previously decided condition transition atomically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentConditionCommitError {
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    ConditionChanged {
        equipment: EquipmentId,
        expected: Condition,
        actual: Condition,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
    },
}

impl Display for EquipmentConditionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "equipment condition plan expected revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::ConditionChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from planned {} ppm to {} ppm",
                equipment.value(),
                expected.parts_per_million(),
                actual.parts_per_million()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by production job {} until tick {} before condition commit",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
        }
    }
}

impl Error for EquipmentConditionCommitError {}

/// Applies a condition plan exactly once if no equipment mutation occurred since it was decided.
pub fn apply_equipment_condition_plan(
    state: &mut AppState,
    plan: EquipmentConditionPlan,
) -> Result<(), EquipmentConditionCommitError> {
    let actual_revision = state.equipment().revision();
    if actual_revision != plan.expected_revision {
        return Err(EquipmentConditionCommitError::StaleRevision {
            expected: plan.expected_revision,
            actual: actual_revision,
        });
    }
    if let Some(job) = state.production().jobs().find(|job| {
        job.equipment_provider()
            .is_some_and(|provider| provider.equipment() == plan.equipment)
    }) {
        return Err(EquipmentConditionCommitError::EquipmentBusy {
            equipment: plan.equipment,
            job: job.id(),
            completes_at: job.completes_at(),
        });
    }

    let equipment_state = state.equipment_state_mut();
    let Some(record) = equipment_state.records.get_mut(&plan.equipment) else {
        return Err(EquipmentConditionCommitError::UnknownEquipment {
            equipment: plan.equipment,
        });
    };
    if record.condition != plan.transition.before() {
        return Err(EquipmentConditionCommitError::ConditionChanged {
            equipment: plan.equipment,
            expected: plan.transition.before(),
            actual: record.condition,
        });
    }

    record.condition = plan.transition.after();
    equipment_state.revision = plan.next_revision;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::make_test_registries_with_equipment;
    use crate::core::quantity::Mass;
    use crate::core::time::WorldSeed;
    use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId};
    use crate::maintenance::MaintenanceThresholds;

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(810_001);
    const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(810_001);

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("condition fixture failed: {error}"),
        }
    }

    fn make_registries() -> Registries {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(50_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("maintenance fixture failed: {error}"),
        };
        make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "test supported mass",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "test press",
                Mass::from_milligrams(40_000),
                profile,
                thresholds,
            ),
        )
    }

    #[test]
    fn creation_and_condition_changes_use_canonical_revisioned_state() {
        let registries = make_registries();
        let mut state = AppState::new(WorldSeed::new(17));
        let equipment = match add_equipment(
            &registries,
            &mut state,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            Ok(equipment) => equipment,
            Err(error) => panic!("equipment creation failed: {error}"),
        };
        let wear = match decide_equipment_wear(&state, equipment, 300_000) {
            Ok(plan) => plan,
            Err(error) => panic!("wear planning failed: {error}"),
        };
        assert_eq!(wear.before(), Condition::PRISTINE);
        assert_eq!(wear.after(), condition(700_000));
        if let Err(error) = apply_equipment_condition_plan(&mut state, wear) {
            panic!("wear commit failed: {error}");
        }

        let repair = match decide_equipment_repair(&state, equipment, 100_000) {
            Ok(plan) => plan,
            Err(error) => panic!("repair planning failed: {error}"),
        };
        if let Err(error) = apply_equipment_condition_plan(&mut state, repair) {
            panic!("repair commit failed: {error}");
        }

        let record = match state.equipment().get_equipment(equipment) {
            Some(record) => record,
            None => panic!("equipment disappeared after condition changes"),
        };
        assert_eq!(record.condition(), condition(800_000));
        assert_eq!(state.equipment().revision(), 3);
    }

    #[test]
    fn stale_condition_plan_leaves_equipment_unchanged() {
        let registries = make_registries();
        let mut state = AppState::new(WorldSeed::new(23));
        let equipment = match add_equipment(
            &registries,
            &mut state,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            Ok(equipment) => equipment,
            Err(error) => panic!("equipment creation failed: {error}"),
        };
        let stale = match decide_equipment_wear(&state, equipment, 200_000) {
            Ok(plan) => plan,
            Err(error) => panic!("wear planning failed: {error}"),
        };
        if let Err(error) = add_equipment(
            &registries,
            &mut state,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            panic!("second equipment creation failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            apply_equipment_condition_plan(&mut state, stale),
            Err(EquipmentConditionCommitError::StaleRevision {
                expected: 1,
                actual: 2,
            })
        );
        assert_eq!(state, before);
    }
}
