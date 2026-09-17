//! Shared deterministic mining physics used by admission and persistence validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::MiningMethodDefinition;
use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow, Pressure};
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::equipment::{EquipmentDefinition, resolve_equipment_capability};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};

/// A physical capability, duration, or remaining tool-lifetime limit on mining effort.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningPhysicsError {
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    BatchTooLarge {
        maximum: Mass,
        requested: Mass,
    },
    DepositTooHard {
        hardness: Pressure,
        maximum: Pressure,
    },
    ZeroThroughput,
    Duration(MassFlowDurationError),
    ConditionDuration(ActiveConditionDurationError),
}

impl Display for MiningPhysicsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCapability { capability } => write!(
                formatter,
                "mining equipment lacks usable capability {}",
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "mining capability {} requires {expected:?}, found {found:?}",
                capability.value()
            ),
            Self::BatchTooLarge { maximum, requested } => write!(
                formatter,
                "mining batch requests {} mg, exceeding {} mg capacity",
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::DepositTooHard { hardness, maximum } => write!(
                formatter,
                "mining hardness {} Pa exceeds equipment limit {} Pa",
                hardness.pascals(),
                maximum.pascals()
            ),
            Self::ZeroThroughput => formatter.write_str("mining throughput must be nonzero"),
            Self::Duration(error) => Display::fmt(error, formatter),
            Self::ConditionDuration(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for MiningPhysicsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::DepositTooHard { .. }
            | Self::ZeroThroughput => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedMiningPhysics {
    duration: TickSpan,
    condition_after: Condition,
}

impl ResolvedMiningPhysics {
    #[must_use]
    pub(crate) const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub(crate) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves extraction throughput, batch capacity, excavation resistance, duration, and tool wear
/// from immutable method/equipment definitions plus the deposit's geological excavation hardness.
pub(crate) fn resolve_mining_physics(
    physical_tick_duration: PhysicalTickDuration,
    method: &MiningMethodDefinition,
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    excavation_hardness: Pressure,
    mass: Mass,
) -> Result<ResolvedMiningPhysics, MiningPhysicsError> {
    let flow_capability = method.mass_flow_capability();
    let flow_value = resolve_equipment_capability(equipment, condition_before, flow_capability)
        .ok_or(MiningPhysicsError::MissingCapability {
            capability: flow_capability,
        })?;
    let CapabilityValue::MassFlow(flow) = flow_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: flow_capability,
            expected: CapabilityValueKind::MassFlow,
            found: flow_value.kind(),
        });
    };

    let batch_capability = method.max_batch_mass_capability();
    let batch_value = resolve_equipment_capability(equipment, condition_before, batch_capability)
        .ok_or(MiningPhysicsError::MissingCapability {
        capability: batch_capability,
    })?;
    let CapabilityValue::Mass(maximum_batch) = batch_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: batch_capability,
            expected: CapabilityValueKind::Mass,
            found: batch_value.kind(),
        });
    };

    let hardness_capability = method.max_hardness_capability();
    let hardness_value =
        resolve_equipment_capability(equipment, condition_before, hardness_capability).ok_or(
            MiningPhysicsError::MissingCapability {
                capability: hardness_capability,
            },
        )?;
    let CapabilityValue::Pressure(maximum_hardness) = hardness_value else {
        return Err(MiningPhysicsError::CapabilityKindMismatch {
            capability: hardness_capability,
            expected: CapabilityValueKind::Pressure,
            found: hardness_value.kind(),
        });
    };

    if flow == MassFlow::ZERO {
        return Err(MiningPhysicsError::ZeroThroughput);
    }
    if mass > maximum_batch {
        return Err(MiningPhysicsError::BatchTooLarge {
            maximum: maximum_batch,
            requested: mass,
        });
    }

    if excavation_hardness > maximum_hardness {
        return Err(MiningPhysicsError::DepositTooHard {
            hardness: excavation_hardness,
            maximum: maximum_hardness,
        });
    }

    let duration = calculate_mass_flow_duration_ceiling(flow, mass, physical_tick_duration)
        .map_err(MiningPhysicsError::Duration)?;
    let condition_after = calculate_usable_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        condition_before,
        duration,
    )
    .map_err(MiningPhysicsError::ConditionDuration)?;
    Ok(ResolvedMiningPhysics {
        duration,
        condition_after,
    })
}
