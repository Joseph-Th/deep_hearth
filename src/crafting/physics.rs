//! Pure timing and wear physics shared by manual-craft admission and trusted replay.

use std::num::NonZeroU64;

use crate::core::quantity::{Mass, MassFlow};
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManualCraftEquipmentScheduleError {
    Duration(MassFlowDurationError),
    Condition(ActiveConditionDurationError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ManualCraftEquipmentSchedule {
    duration: TickSpan,
    condition_after: Condition,
}

impl ManualCraftEquipmentSchedule {
    pub(crate) const fn duration(self) -> TickSpan {
        self.duration
    }

    pub(super) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

pub(super) fn resolve_manual_craft_hand_duration(
    duration_per_batch: TickSpan,
    batches: NonZeroU64,
) -> Option<TickSpan> {
    duration_per_batch
        .value()
        .checked_mul(batches.get())
        .map(TickSpan::new)
}

pub(crate) fn resolve_manual_craft_equipment_schedule(
    rate: MassFlow,
    mass: Mass,
    physical_tick_duration: PhysicalTickDuration,
    wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<ManualCraftEquipmentSchedule, ManualCraftEquipmentScheduleError> {
    let duration = calculate_mass_flow_duration_ceiling(rate, mass, physical_tick_duration)
        .map_err(ManualCraftEquipmentScheduleError::Duration)?;
    let condition_after = calculate_usable_condition_after_active_ticks(
        wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(ManualCraftEquipmentScheduleError::Condition)?;
    Ok(ManualCraftEquipmentSchedule {
        duration,
        condition_after,
    })
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
