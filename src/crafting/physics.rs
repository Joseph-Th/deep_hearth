//! Pure equipment resolution, timing, and wear physics shared by manual-craft planning,
//! registry operability, runtime admission, and trusted replay.

use std::num::NonZeroU64;

use crate::capability::CapabilityId;
use crate::core::quantity::Mass;
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::equipment::{
    EquipmentDefinition, EquipmentMassFlowResolutionError, EquipmentMassFlowSchedule,
    EquipmentMassFlowScheduleError, resolve_equipment_mass_flow_schedule,
};
use crate::maintenance::Condition;

pub(crate) type ManualCraftEquipmentResolutionError = EquipmentMassFlowResolutionError;
pub(crate) type ManualCraftEquipmentScheduleError = EquipmentMassFlowScheduleError;
pub(crate) type ManualCraftEquipmentSchedule = EquipmentMassFlowSchedule;

pub(super) fn resolve_manual_craft_hand_duration(
    duration_per_batch: TickSpan,
    batches: NonZeroU64,
) -> Option<TickSpan> {
    duration_per_batch
        .value()
        .checked_mul(batches.get())
        .map(TickSpan::new)
}

/// Resolves one authored equipment definition into the exact manual-craft schedule used by
/// planning, runtime admission, and trusted replay.
pub(crate) fn resolve_manual_craft_equipment_physics(
    equipment: &EquipmentDefinition,
    condition: Condition,
    capability: CapabilityId,
    mass: Mass,
    physical_tick_duration: PhysicalTickDuration,
    wear_ppm_per_active_tick: u32,
) -> Result<ManualCraftEquipmentSchedule, ManualCraftEquipmentResolutionError> {
    resolve_equipment_mass_flow_schedule(
        equipment,
        condition,
        capability,
        mass,
        physical_tick_duration,
        wear_ppm_per_active_tick,
    )
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
