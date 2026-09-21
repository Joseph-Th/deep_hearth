//! Read-only physical projection for authored manual-power configurations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValue, CapabilityValueKind};
use crate::core::quantity::{Energy, Power};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::equipment::{EquipmentDefinitionId, project_equipment_capability};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

use super::power_physics::{
    ManualPowerMetabolicDurationError, ManualPowerScheduleError, resolve_manual_power_schedule,
};
use super::{
    ManualPowerMethodId, PlayerWorkResourceBudget, PlayerWorkResourceBudgetError,
    calculate_player_work_resource_budget,
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

/// Failure while projecting manual power from immutable authored definitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualPowerProjectionError {
    UnknownMethod {
        method: ManualPowerMethodId,
    },
    UnknownEquipmentDefinition {
        equipment: EquipmentDefinitionId,
    },
    UnknownStoreDefinition {
        store: EnergyStoreDefinitionId,
    },
    MissingPowerCapability {
        equipment: EquipmentDefinitionId,
        capability: CapabilityId,
    },
    PowerCapabilityKindMismatch {
        equipment: EquipmentDefinitionId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    ZeroEquipmentPower {
        equipment: EquipmentDefinitionId,
        capability: CapabilityId,
    },
    ZeroEnergy,
    EnergyExceedsStoreCapacity {
        store: EnergyStoreDefinitionId,
        requested: Energy,
        capacity: Energy,
    },
    WrongCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ZeroTransferPower {
        equipment: EquipmentDefinitionId,
        store: EnergyStoreDefinitionId,
    },
    PowerDuration {
        energy: Energy,
        power: Power,
    },
    MetabolicConversionTooSmall {
        method: ManualPowerMethodId,
    },
    MetabolicDurationOverflow {
        method: ManualPowerMethodId,
        energy: Energy,
    },
    ExertionResolution {
        method: ManualPowerMethodId,
    },
    ResourceBudgetOverflow,
    ConditionDuration(ActiveConditionDurationError),
}

impl Display for ManualPowerProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(
                    formatter,
                    "manual-power method {} is unknown",
                    method.value()
                )
            }
            Self::UnknownEquipmentDefinition { equipment } => write!(
                formatter,
                "manual-power equipment definition {} is unknown",
                equipment.value()
            ),
            Self::UnknownStoreDefinition { store } => {
                write!(
                    formatter,
                    "manual-power store definition {} is unknown",
                    store.value()
                )
            }
            Self::MissingPowerCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "equipment definition {} does not provide manual-power capability {} at the projected condition",
                equipment.value(),
                capability.value()
            ),
            Self::PowerCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "equipment definition {} capability {} has {found:?} value instead of power",
                equipment.value(),
                capability.value()
            ),
            Self::ZeroEquipmentPower {
                equipment,
                capability,
            } => write!(
                formatter,
                "equipment definition {} capability {} resolves to zero power",
                equipment.value(),
                capability.value()
            ),
            Self::ZeroEnergy => {
                write!(formatter, "manual-power projection requires nonzero energy")
            }
            Self::EnergyExceedsStoreCapacity {
                store,
                requested,
                capacity,
            } => write!(
                formatter,
                "manual-power projection requests {} nJ from empty store definition {} with {} nJ capacity",
                requested.nanojoules(),
                store.value(),
                capacity.nanojoules()
            ),
            Self::WrongCarrier { required, provided } => write!(
                formatter,
                "manual-power method requires {required:?} storage but projected store is {provided:?}"
            ),
            Self::ZeroTransferPower { equipment, store } => write!(
                formatter,
                "manual-power equipment {} and store {} have zero shared transfer power",
                equipment.value(),
                store.value()
            ),
            Self::PowerDuration { energy, power } => write!(
                formatter,
                "manual-power projection cannot schedule {} nJ at {} pW",
                energy.nanojoules(),
                power.picowatts()
            ),
            Self::MetabolicConversionTooSmall { method } => write!(
                formatter,
                "manual-power method {} metabolic conversion rounds to zero",
                method.value()
            ),
            Self::MetabolicDurationOverflow { method, energy } => write!(
                formatter,
                "manual-power method {} metabolic duration overflows for {} nJ",
                method.value(),
                energy.nanojoules()
            ),
            Self::ExertionResolution { method } => write!(
                formatter,
                "manual-power method {} cannot resolve bounded exertion",
                method.value()
            ),
            Self::ResourceBudgetOverflow => {
                write!(
                    formatter,
                    "manual-power projected physiological budget overflows"
                )
            }
            Self::ConditionDuration(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ManualPowerProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ConditionDuration(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownStoreDefinition { .. }
            | Self::MissingPowerCapability { .. }
            | Self::PowerCapabilityKindMismatch { .. }
            | Self::ZeroEquipmentPower { .. }
            | Self::ZeroEnergy
            | Self::EnergyExceedsStoreCapacity { .. }
            | Self::WrongCarrier { .. }
            | Self::ZeroTransferPower { .. }
            | Self::PowerDuration { .. }
            | Self::MetabolicConversionTooSmall { .. }
            | Self::MetabolicDurationOverflow { .. }
            | Self::ExertionResolution { .. }
            | Self::ResourceBudgetOverflow => None,
        }
    }
}

/// Projects one manual-power work order from authored definitions without runtime authorization.
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
        registries.core().physical_tick_duration(),
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
    let resource_budget = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        schedule.exertion(),
        duration,
    )
    .map_err(|error| match error {
        PlayerWorkResourceBudgetError::EnergyOverflow
        | PlayerWorkResourceBudgetError::HydrationOverflow => {
            ManualPowerProjectionError::ResourceBudgetOverflow
        }
    })?;
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
