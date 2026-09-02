//! Completion-stage equipment condition recovery for admitted maintenance work.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::PlayerWork;

use super::EquipmentMaintenanceOutcome;

#[must_use]
pub(crate) struct EquipmentMaintenanceTickPlan {
    equipment: crate::equipment::EquipmentId,
    condition_before: crate::maintenance::Condition,
    condition_after: crate::maintenance::Condition,
}

impl EquipmentMaintenanceTickPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_equipment_maintenance_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Option<EquipmentMaintenanceTickPlan> {
    let Some(PlayerWork::EquipmentMaintenance { work }) = state.player_work().active() else {
        return None;
    };
    if work.completes_at() != next_tick {
        return None;
    }
    let record = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active maintenance references missing equipment")
        });
    assert_eq!(record.definition(), work.equipment_trace().definition());
    assert_eq!(record.condition(), work.condition_before());
    Some(EquipmentMaintenanceTickPlan {
        equipment: work.equipment(),
        condition_before: work.condition_before(),
        condition_after: work.condition_after(),
    })
}

pub(crate) fn apply_equipment_maintenance_tick(
    state: &mut AppState,
    plan: Option<EquipmentMaintenanceTickPlan>,
) -> Option<EquipmentMaintenanceOutcome> {
    let plan = plan?;
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted maintenance equipment revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.condition_before,
        plan.condition_after,
        next_equipment_revision,
    );
    Some(EquipmentMaintenanceOutcome {
        equipment: plan.equipment,
        condition_before: plan.condition_before,
        condition_after: plan.condition_after,
    })
}
