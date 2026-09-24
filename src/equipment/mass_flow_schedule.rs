//! Shared condition-adjusted equipment throughput scheduling.

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow};
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};

use super::definitions::EquipmentDefinition;
use super::equipment_integration::resolve_equipment_capability;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentMassFlowResolutionError {
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    Schedule(EquipmentMassFlowScheduleError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentMassFlowScheduleError {
    Duration(MassFlowDurationError),
    Condition(ActiveConditionDurationError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EquipmentMassFlowSchedule {
    rate: MassFlow,
    duration: TickSpan,
    condition_after: Condition,
}

impl EquipmentMassFlowSchedule {
    pub(crate) const fn rate(self) -> MassFlow {
        self.rate
    }

    pub(crate) const fn duration(self) -> TickSpan {
        self.duration
    }

    pub(crate) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves one equipment definition into a condition-adjusted material-throughput schedule.
///
/// This is shared by direct-labor operations whose durable tool or workstation changes throughput
/// without changing material transformation semantics. Runtime ownership/occupancy remains with the
/// caller's normal equipment-provider and production boundaries.
pub(crate) fn resolve_equipment_mass_flow_schedule(
    equipment: &EquipmentDefinition,
    condition: Condition,
    capability: CapabilityId,
    mass: Mass,
    physical_tick_duration: PhysicalTickDuration,
    wear_ppm_per_active_tick: u32,
) -> Result<EquipmentMassFlowSchedule, EquipmentMassFlowResolutionError> {
    let rate = match resolve_equipment_capability(equipment, condition, capability) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(value) => {
            return Err(EquipmentMassFlowResolutionError::CapabilityKindMismatch {
                capability,
                found: value.kind(),
            });
        }
        None => {
            return Err(EquipmentMassFlowResolutionError::MissingCapability { capability });
        }
    };
    let duration = calculate_mass_flow_duration_ceiling(rate, mass, physical_tick_duration)
        .map_err(EquipmentMassFlowScheduleError::Duration)
        .map_err(EquipmentMassFlowResolutionError::Schedule)?;
    let condition_after = calculate_usable_condition_after_active_ticks(
        wear_ppm_per_active_tick,
        condition,
        duration,
    )
    .map_err(EquipmentMassFlowScheduleError::Condition)
    .map_err(EquipmentMassFlowResolutionError::Schedule)?;
    Ok(EquipmentMassFlowSchedule {
        rate,
        duration,
        condition_after,
    })
}
