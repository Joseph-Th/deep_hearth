//! Physical settlement of interrupted direct player-powered generation.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::registry::Registries;

use super::super::PlayerWork;

#[must_use]
pub(crate) struct ManualPowerInterruptionPlan {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

impl ManualPowerInterruptionPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_manual_power_interruption(
    registries: &Registries,
    state: &AppState,
    interrupted_at: SimulationTick,
) -> Option<ManualPowerInterruptionPlan> {
    let Some(PlayerWork::ManualPower { work }) = state.player_work().active() else {
        return None;
    };
    if work.completes_at() <= interrupted_at {
        return None;
    }
    let elapsed = interrupted_at
        .checked_duration_since(work.started_at())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: manual power interruption precedes start")
        });
    assert!(
        elapsed.value() != 0,
        "manual power interruption must settle at least one active tick"
    );
    let definition = registries
        .labor()
        .get_manual_power(work.method())
        .copied()
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: interrupted manual power method disappeared")
        });
    let condition_after = calculate_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        work.equipment_trace().condition(),
        elapsed,
    );
    Some(ManualPowerInterruptionPlan {
        equipment: work.equipment(),
        condition_before: work.equipment_trace().condition(),
        condition_after,
    })
}

pub(crate) fn apply_manual_power_interruption(
    state: &mut AppState,
    plan: ManualPowerInterruptionPlan,
) {
    let next_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted manual power interruption revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.condition_before,
        plan.condition_after,
        next_revision,
    );
}
