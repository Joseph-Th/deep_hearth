//! Physical instrument consequences of interrupted field prospecting.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::labor::PlayerWork;
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::registry::Registries;

#[must_use]
pub(crate) struct FieldProspectingInterruptionPlan {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

impl FieldProspectingInterruptionPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_field_prospecting_interruption(
    registries: &Registries,
    state: &AppState,
    interrupted_at: SimulationTick,
) -> Option<FieldProspectingInterruptionPlan> {
    let Some(PlayerWork::Prospecting { work }) = state.player_work().active() else {
        return None;
    };
    if work.completes_at() <= interrupted_at {
        return None;
    }
    let trace = work.equipment_trace()?;
    let method = registries
        .labor()
        .get_prospecting(work.method())
        .copied()
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: interrupted prospecting method disappeared")
        });
    let profile = method.equipment().unwrap_or_else(|| {
        panic!("runtime invariant broken: instrumented prospecting method has no equipment profile")
    });
    let wear = profile
        .condition_wear_ppm_per_active_tick(trace.definition())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: interrupted prospecting instrument is not accepted")
        });
    let elapsed = interrupted_at
        .checked_duration_since(work.started_at())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: prospecting interruption precedes start")
        });
    assert!(
        elapsed.value() != 0,
        "prospecting interruption must settle at least one active tick"
    );
    let condition_after = calculate_condition_after_active_ticks(wear, trace.condition(), elapsed);
    Some(FieldProspectingInterruptionPlan {
        equipment: trace.equipment(),
        condition_before: trace.condition(),
        condition_after,
    })
}

pub(crate) fn apply_field_prospecting_interruption(
    state: &mut AppState,
    plan: FieldProspectingInterruptionPlan,
) {
    let next_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted prospecting interruption revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.condition_before,
        plan.condition_after,
        next_revision,
    );
}
