//! Read-only physical projection for authored manual-power configurations.

mod current;
mod errors;

pub use current::{
    ManualPowerDestinationTargetAssessment, ManualPowerDestinationTargetBlocker,
    ManualPowerDestinationTargetProjection, ManualPowerDestinationTargetRequest,
    ManualPowerEnergyEnvelope, ManualPowerEnergyEnvelopeRequest,
    assess_manual_power_destination_target, assess_manual_power_energy_envelope,
};
pub use errors::ManualPowerProjectionError;

use crate::capability::CapabilityValue;
use crate::core::quantity::{Energy, Power};
use crate::core::time::TickSpan;
use crate::energy::{EnergyStoreDefinition, EnergyStoreDefinitionId};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, project_equipment_capability};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::registry::{CoreDefinitions, Registries};
use crate::survival::{PhysiologyDefinition, SurvivalExertion};

use super::power_physics::{
    ManualPowerMetabolicDurationError, ManualPowerScheduleError, resolve_manual_power_schedule,
};
use super::{
    ManualPowerDefinition, ManualPowerMethodId, PlayerWorkResourceBudget,
    PlayerWorkResourceBudgetError, calculate_player_work_resource_budget,
};

/// Physical schedule for a future authored manual-power configuration.
///
/// This projection does not prove current ownership, occupancy, store fill, player survival reserve,
/// or stale-state validity. Runtime work must still be admitted through the normal manual-power
/// validation path.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerProjection {
    transfer_power: Power,
    duration: TickSpan,
    exertion: SurvivalExertion,
    resource_budget: PlayerWorkResourceBudget,
    condition_after: Condition,
}

impl ManualPowerProjection {
    #[must_use]
    pub const fn transfer_power(self) -> Power {
        self.transfer_power
    }

    #[must_use]
    pub const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub const fn exertion(self) -> SurvivalExertion {
        self.exertion
    }

    #[must_use]
    pub const fn resource_budget(self) -> PlayerWorkResourceBudget {
        self.resource_budget
    }

    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Projects one manual-power work order from authored definitions without runtime authorization.
///
/// Direct manual-power equipment is portable-only, so structurally installed definitions are
/// rejected here. Current mounting and occupancy still belong to runtime admission.
///
/// Energy is the intended addition to an empty or sufficiently free future store. Current store
/// fill and occupancy remain runtime state and are deliberately outside this projection.
pub fn project_manual_power(
    registries: &Registries,
    method: ManualPowerMethodId,
    equipment: EquipmentDefinitionId,
    condition: Condition,
    store: EnergyStoreDefinitionId,
    energy: Energy,
) -> Result<ManualPowerProjection, ManualPowerProjectionError> {
    let method_definition = registries
        .labor()
        .get_manual_power(method)
        .copied()
        .ok_or(ManualPowerProjectionError::UnknownMethod { method })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(equipment)
        .ok_or(ManualPowerProjectionError::UnknownEquipmentDefinition { equipment })?;
    let store_definition = registries
        .energy()
        .get_store(store)
        .ok_or(ManualPowerProjectionError::UnknownStoreDefinition { store })?;
    project_manual_power_configuration(
        registries.core(),
        registries.survival().physiology(),
        method_definition,
        equipment_definition,
        condition,
        store_definition,
        energy,
    )
}

/// Shared immutable-definition projection used by registry operability and public planning.
pub(crate) fn project_manual_power_configuration(
    core: &CoreDefinitions,
    physiology: PhysiologyDefinition,
    method_definition: ManualPowerDefinition,
    equipment_definition: &EquipmentDefinition,
    condition: Condition,
    store_definition: &EnergyStoreDefinition,
    energy: Energy,
) -> Result<ManualPowerProjection, ManualPowerProjectionError> {
    let method = method_definition.id();
    let equipment = equipment_definition.id();
    let store = store_definition.id();
    if equipment_definition.requires_structural_support() {
        return Err(ManualPowerProjectionError::EquipmentRequiresStructuralSupport { equipment });
    }
    if energy.is_zero() {
        return Err(ManualPowerProjectionError::ZeroEnergy);
    }
    if energy > store_definition.capacity() {
        return Err(ManualPowerProjectionError::EnergyExceedsStoreCapacity {
            store,
            requested: energy,
            capacity: store_definition.capacity(),
        });
    }
    if method_definition.carrier() != store_definition.carrier() {
        return Err(ManualPowerProjectionError::WrongCarrier {
            required: method_definition.carrier(),
            provided: store_definition.carrier(),
        });
    }
    let capability = method_definition.power_capability();
    let equipment_power =
        match project_equipment_capability(equipment_definition, condition, capability) {
            Some(CapabilityValue::Power(power)) => power,
            Some(
                value @ (CapabilityValue::Mass(_)
                | CapabilityValue::Temperature(_)
                | CapabilityValue::Pressure(_)
                | CapabilityValue::MassFlow(_)),
            ) => {
                return Err(ManualPowerProjectionError::PowerCapabilityKindMismatch {
                    equipment,
                    capability,
                    found: value.kind(),
                });
            }
            None => {
                return Err(ManualPowerProjectionError::MissingPowerCapability {
                    equipment,
                    capability,
                });
            }
        };
    if equipment_power.is_zero() {
        return Err(ManualPowerProjectionError::ZeroEquipmentPower {
            equipment,
            capability,
        });
    }
    let transfer_power = std::cmp::min(equipment_power, store_definition.max_input_power());
    if transfer_power.is_zero() {
        return Err(ManualPowerProjectionError::ZeroTransferPower { equipment, store });
    }
    let schedule = resolve_manual_power_schedule(
        energy,
        transfer_power,
        core.physical_tick_duration(),
        method_definition.maximum_exertion(),
        method_definition.metabolic_efficiency_ppm(),
    )
    .map_err(|error| match error {
        ManualPowerScheduleError::PowerDuration(_) => ManualPowerProjectionError::PowerDuration {
            energy,
            power: transfer_power,
        },
        ManualPowerScheduleError::MetabolicDuration(
            ManualPowerMetabolicDurationError::ZeroOutput,
        ) => ManualPowerProjectionError::MetabolicConversionTooSmall { method },
        ManualPowerScheduleError::MetabolicDuration(
            ManualPowerMetabolicDurationError::DurationOverflow,
        ) => ManualPowerProjectionError::MetabolicDurationOverflow { method, energy },
        ManualPowerScheduleError::Exertion(_) => {
            ManualPowerProjectionError::ExertionResolution { method }
        }
    })?;
    let duration = schedule.duration();
    let resource_budget =
        calculate_player_work_resource_budget(physiology, schedule.exertion(), duration).map_err(
            |error| match error {
                PlayerWorkResourceBudgetError::EnergyOverflow
                | PlayerWorkResourceBudgetError::HydrationOverflow => {
                    ManualPowerProjectionError::ResourceBudgetOverflow
                }
            },
        )?;
    let condition_after = calculate_usable_condition_after_active_ticks(
        method_definition.condition_wear_ppm_per_active_tick(),
        condition,
        duration,
    )
    .map_err(ManualPowerProjectionError::ConditionDuration)?;
    Ok(ManualPowerProjection {
        transfer_power,
        duration,
        exertion: schedule.exertion(),
        resource_budget,
        condition_after,
    })
}

#[cfg(test)]
#[path = "power_projection_tests.rs"]
mod tests;
