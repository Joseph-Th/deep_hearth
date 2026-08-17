//! Canonical equipment creation and revision-checked condition mutation for sibling persistent state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::maintenance::{Condition, ConditionPlan, decide_wear};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::registry::Registries;

#[cfg(any(test, feature = "test-gameplay"))]
use super::definitions::EquipmentDefinitionId;
use super::state::EquipmentId;
#[cfg(any(test, feature = "test-gameplay"))]
use super::state::EquipmentRecord;

/// Failure while allocating one persistent equipment instance.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddEquipmentError {
    UnknownDefinition { definition: EquipmentDefinitionId },
    RequiresAssembly { definition: EquipmentDefinitionId },
    IdExhausted,
    RevisionExhausted,
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Display for AddEquipmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown equipment definition {}",
                definition.value()
            ),
            Self::RequiresAssembly { definition } => write!(
                formatter,
                "equipment definition {} requires conserved gameplay assembly",
                definition.value()
            ),
            Self::IdExhausted => formatter.write_str("equipment identifier space is exhausted"),
            Self::RevisionExhausted => formatter.write_str("equipment revision space is exhausted"),
        }
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for AddEquipmentError {}

/// Adds one equipment record for tests and gameplay harness bootstrap fixtures.
#[cfg(any(test, feature = "test-gameplay"))]
pub fn add_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
    condition: Condition,
) -> Result<EquipmentId, AddEquipmentError> {
    let Some(definition_record) = registries.equipment().get_equipment(definition) else {
        return Err(AddEquipmentError::UnknownDefinition { definition });
    };
    if definition_record.assembly_profile().is_some() {
        return Err(AddEquipmentError::RequiresAssembly { definition });
    }

    let equipment_state = state.equipment();
    let id = EquipmentId::new(equipment_state.next_equipment_id());
    let next_equipment_id = equipment_state
        .next_equipment_id()
        .checked_add(1)
        .ok_or(AddEquipmentError::IdExhausted)?;
    let next_revision = equipment_state
        .revision()
        .checked_add(1)
        .ok_or(AddEquipmentError::RevisionExhausted)?;
    let record = EquipmentRecord {
        id,
        definition,
        condition,
        embodied_mass: definition_record.mass(),
        embodied_material: Vec::new(),
        supported_by: None,
        created_at: state.tick(),
    };

    let equipment_state = state.equipment_state_mut();
    equipment_state.insert_equipment(record, next_equipment_id, next_revision);
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
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
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
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
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
    let equipment_state = state.equipment();
    let Some(record) = equipment_state.get_equipment(equipment) else {
        return Err(EquipmentConditionPlanError::UnknownEquipment { equipment });
    };
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentConditionPlanError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentConditionPlanError::EquipmentBusyMining { equipment, job });
    }
    let next_revision = equipment_state
        .revision()
        .checked_add(1)
        .ok_or(EquipmentConditionPlanError::RevisionExhausted)?;
    Ok(EquipmentConditionPlan {
        equipment,
        expected_revision: equipment_state.revision(),
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
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
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
                release,
            } => write!(
                formatter,
                "equipment {} became occupied by production job {} {release} before condition commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before condition commit",
                equipment.value(),
                job.value()
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
    if let Some(job) = state.production().get_equipment_occupant(plan.equipment) {
        return Err(EquipmentConditionCommitError::EquipmentBusy {
            equipment: plan.equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(plan.equipment) {
        return Err(EquipmentConditionCommitError::EquipmentBusyMining {
            equipment: plan.equipment,
            job,
        });
    }

    let Some(record) = state.equipment().get_equipment(plan.equipment) else {
        return Err(EquipmentConditionCommitError::UnknownEquipment {
            equipment: plan.equipment,
        });
    };
    if record.condition() != plan.transition.before() {
        return Err(EquipmentConditionCommitError::ConditionChanged {
            equipment: plan.equipment,
            expected: plan.transition.before(),
            actual: record.condition(),
        });
    }

    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.transition.before(),
        plan.transition.after(),
        plan.next_revision,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::make_test_registries_with_equipment;
    use crate::content::{EQUIPMENT_STONE_PICK, build_registries};
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

    #[test]
    fn bootstrap_creation_cannot_bypass_authored_assembly() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(19));
        let before = state.clone();

        assert_eq!(
            add_equipment(
                &registries,
                &mut state,
                EQUIPMENT_STONE_PICK,
                Condition::PRISTINE,
            ),
            Err(AddEquipmentError::RequiresAssembly {
                definition: EQUIPMENT_STONE_PICK,
            })
        );
        assert_eq!(state, before);
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
    fn creation_and_wear_use_canonical_revisioned_state() {
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

        let record = match state.equipment().get_equipment(equipment) {
            Some(record) => record,
            None => panic!("equipment disappeared after condition changes"),
        };
        assert_eq!(record.condition(), condition(700_000));
        assert_eq!(state.equipment().revision(), 2);
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
