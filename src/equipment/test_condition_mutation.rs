//! Provides test-only condition mutation while production wear remains the gameplay condition path.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::EquipmentId;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EquipmentConditionPlan {
    equipment: EquipmentId,
    expected_revision: u64,
    next_revision: u64,
    before: Condition,
    after: Condition,
}

impl EquipmentConditionPlan {
    #[must_use]
    pub(crate) const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub(crate) const fn before(&self) -> Condition {
        self.before
    }

    #[must_use]
    pub(crate) const fn after(&self) -> Condition {
        self.after
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentConditionPlanError {
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
    decide: impl FnOnce(Condition) -> Condition,
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
    let before = record.condition();
    Ok(EquipmentConditionPlan {
        equipment,
        expected_revision: equipment_state.revision(),
        next_revision,
        before,
        after: decide(before),
    })
}

pub(crate) fn decide_equipment_wear(
    state: &AppState,
    equipment: EquipmentId,
    wear_ppm: u32,
) -> Result<EquipmentConditionPlan, EquipmentConditionPlanError> {
    decide_condition_change(state, equipment, |condition| {
        calculate_condition_after_active_ticks(wear_ppm, condition, TickSpan::new(1))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentConditionCommitError {
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

pub(crate) fn apply_equipment_condition_plan(
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
    if record.condition() != plan.before {
        return Err(EquipmentConditionCommitError::ConditionChanged {
            equipment: plan.equipment,
            expected: plan.before,
            actual: record.condition(),
        });
    }

    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.before,
        plan.after,
        plan.next_revision,
    );
    Ok(())
}
