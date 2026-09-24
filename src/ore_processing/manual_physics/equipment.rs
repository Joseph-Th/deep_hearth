//! Optional durable equipment physics for direct-labor ore processing.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, MassFlow};
use crate::core::state::AppState;
use crate::core::throughput::MassFlowDurationError;
use crate::core::time::TickSpan;
use crate::equipment::{
    EquipmentId, EquipmentMassFlowResolutionError, EquipmentMassFlowScheduleError,
    EquipmentProviderError, ValidatedEquipmentUse, resolve_equipment_mass_flow_schedule,
    resolve_equipment_provider,
};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::ProcessId;
use crate::registry::Registries;

use crate::ore_processing::definitions::ManualOreProcessProfile;

/// Failure while binding optional durable equipment to direct-labor ore work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOreEquipmentError {
    EquipmentNotSupported {
        process: ProcessId,
        equipment: EquipmentId,
    },
    Provider(EquipmentProviderError),
    MissingCapability {
        equipment: EquipmentId,
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        equipment: EquipmentId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    Duration(MassFlowDurationError),
    Condition(ActiveConditionDurationError),
}

impl Display for ManualOreEquipmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EquipmentNotSupported { process, equipment } => write!(
                formatter,
                "manual ore process {} does not support equipment {}",
                process.value(),
                equipment.value()
            ),
            Self::Provider(error) => write!(formatter, "equipment provider failed: {error}"),
            Self::MissingCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "equipment {} lacks manual ore throughput capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "equipment {} capability {} has {found:?} rather than mass-flow semantics",
                equipment.value(),
                capability.value()
            ),
            Self::Duration(error) => {
                write!(formatter, "equipment throughput duration failed: {error}")
            }
            Self::Condition(error) => {
                write!(formatter, "equipment condition duration failed: {error}")
            }
        }
    }
}

impl Error for ManualOreEquipmentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::Condition(error) => Some(error),
            Self::EquipmentNotSupported { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::ore_processing) struct ResolvedManualOreEquipment {
    equipment_use: ValidatedEquipmentUse,
    processing_rate: MassFlow,
    duration: TickSpan,
    condition_after: Condition,
}

impl ResolvedManualOreEquipment {
    pub(in crate::ore_processing) const fn equipment_use(self) -> ValidatedEquipmentUse {
        self.equipment_use
    }

    pub(in crate::ore_processing) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    pub(in crate::ore_processing) const fn duration(self) -> TickSpan {
        self.duration
    }

    pub(in crate::ore_processing) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

fn map_equipment_physics_error(
    equipment: EquipmentId,
    error: EquipmentMassFlowResolutionError,
) -> ManualOreEquipmentError {
    match error {
        EquipmentMassFlowResolutionError::MissingCapability { capability } => {
            ManualOreEquipmentError::MissingCapability {
                equipment,
                capability,
            }
        }
        EquipmentMassFlowResolutionError::CapabilityKindMismatch { capability, found } => {
            ManualOreEquipmentError::CapabilityKindMismatch {
                equipment,
                capability,
                found,
            }
        }
        EquipmentMassFlowResolutionError::Schedule(error) => match error {
            EquipmentMassFlowScheduleError::Duration(error) => {
                ManualOreEquipmentError::Duration(error)
            }
            EquipmentMassFlowScheduleError::Condition(error) => {
                ManualOreEquipmentError::Condition(error)
            }
        },
    }
}

pub(in crate::ore_processing) fn resolve_manual_ore_equipment(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    profile: ManualOreProcessProfile,
    selected: Mass,
    equipment: EquipmentId,
) -> Result<ResolvedManualOreEquipment, ManualOreEquipmentError> {
    let equipment_profile = profile
        .equipment_profile()
        .ok_or(ManualOreEquipmentError::EquipmentNotSupported { process, equipment })?;
    let provider = resolve_equipment_provider(registries, state, equipment)
        .map_err(ManualOreEquipmentError::Provider)?;
    let schedule = resolve_equipment_mass_flow_schedule(
        provider.definition(),
        provider.condition(),
        equipment_profile.mass_flow_capability(),
        selected,
        registries.core().physical_tick_duration(),
        equipment_profile.condition_wear_ppm_per_active_tick(),
    )
    .map_err(|error| map_equipment_physics_error(equipment, error))?;
    Ok(ResolvedManualOreEquipment {
        equipment_use: provider.validated_use(),
        processing_rate: schedule.rate(),
        duration: schedule.duration(),
        condition_after: schedule.condition_after(),
    })
}
