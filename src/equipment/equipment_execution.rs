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
pub(crate) fn add_equipment(
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
    EquipmentBusyManualPower {
        equipment: EquipmentId,
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
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation",
                equipment.value()
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
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentConditionPlanError::EquipmentBusyManualPower { equipment });
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
    EquipmentBusyManualPower {
        equipment: EquipmentId,
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
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before condition commit",
                equipment.value()
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
    if state
        .player_work()
        .get_manual_power_equipment_occupant(plan.equipment)
        .is_some()
    {
        return Err(EquipmentConditionCommitError::EquipmentBusyManualPower {
            equipment: plan.equipment,
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
#[path = "equipment_execution_tests.rs"]
mod tests;
