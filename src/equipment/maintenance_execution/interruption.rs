//! Proportional service recovery when an admitted maintenance interval is interrupted.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::maintenance::{Condition, calculate_condition_after_partial_recovery};

use super::super::EquipmentId;

#[must_use]
pub(crate) struct EquipmentMaintenanceInterruptionPlan {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

impl EquipmentMaintenanceInterruptionPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_equipment_maintenance_interruption(
    state: &AppState,
    interrupted_at: SimulationTick,
) -> Option<EquipmentMaintenanceInterruptionPlan> {
    let work = state.player_work().active()?.equipment_maintenance()?;
    if work.completes_at() <= interrupted_at {
        return None;
    }
    let total = work
        .completes_at()
        .checked_duration_since(work.started_at())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: maintenance completion precedes start")
        });
    let elapsed = interrupted_at
        .checked_duration_since(work.started_at())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: maintenance interruption precedes start")
        });
    assert!(
        elapsed.value() != 0,
        "maintenance interruption must settle at least one active tick"
    );
    let condition_after = calculate_condition_after_partial_recovery(
        work.condition_before(),
        work.condition_after(),
        elapsed,
        total,
    );
    if condition_after == work.condition_before() {
        return None;
    }
    Some(EquipmentMaintenanceInterruptionPlan {
        equipment: work.equipment(),
        condition_before: work.condition_before(),
        condition_after,
    })
}

pub(crate) fn apply_equipment_maintenance_interruption(
    state: &mut AppState,
    plan: EquipmentMaintenanceInterruptionPlan,
) {
    let next_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted maintenance interruption revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.condition_before,
        plan.condition_after,
        next_revision,
    );
}
