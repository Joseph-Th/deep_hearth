//! Completion-time energy creation and equipment wear for direct manual power.

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{EnergyStoreRecord, apply_released_energy_outcomes};

use super::super::{ManualPowerWork, PlayerWork};
use super::ManualPowerOutcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualPowerTickError {
    EnergyRevisionExhausted,
    EquipmentRevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualPowerTickPlan {
    work: ManualPowerWork,
    stored_before: Energy,
}

impl ManualPowerTickPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }

    pub(crate) const fn energy_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_manual_power_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<ManualPowerTickPlan>, ManualPowerTickError> {
    let Some(PlayerWork::ManualPower { work }) = state.player_work().active() else {
        return Ok(None);
    };
    if work.completes_at() != next_tick {
        return Ok(None);
    }
    state
        .energy()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerTickError::EnergyRevisionExhausted)?;
    state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerTickError::EquipmentRevisionExhausted)?;
    let stored_before = state
        .energy()
        .get_store(work.destination())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power destination disappeared"))
        .stored();
    Ok(Some(ManualPowerTickPlan {
        work,
        stored_before,
    }))
}

pub(crate) fn apply_manual_power_tick(
    state: &mut AppState,
    plan: Option<ManualPowerTickPlan>,
) -> Option<ManualPowerOutcome> {
    let plan = plan?;
    let work = plan.work;
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| panic!("runtime invariant broken: manual power equipment disappeared"));
    assert_eq!(
        equipment.condition(),
        work.equipment_trace().condition(),
        "manual power occupancy must prevent equipment condition mutation while work is active"
    );
    assert_eq!(
        state
            .energy()
            .get_store(work.destination())
            .map(EnergyStoreRecord::stored),
        Some(plan.stored_before),
        "manual power occupancy must prevent destination mutation while work is active"
    );

    let energy_revision = state.energy().revision();
    let next_energy_revision = energy_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power energy revision exhausted"));
    apply_released_energy_outcomes(
        state.energy_state_mut(),
        energy_revision,
        next_energy_revision,
        &[work.output()],
    );

    let equipment_revision = state.equipment().revision();
    let next_equipment_revision = equipment_revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("prevalidated manual power equipment revision exhausted"));
    state.equipment_state_mut().apply_condition_change(
        work.equipment(),
        work.equipment_trace().condition(),
        work.condition_after(),
        next_equipment_revision,
    );

    Some(ManualPowerOutcome {
        method: work.method(),
        equipment: work.equipment(),
        destination: work.destination(),
        energy: work.output().energy(),
    })
}
