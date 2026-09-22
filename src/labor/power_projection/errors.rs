//! Typed failures for immutable manual-power planning.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Energy, Power};
use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::equipment::EquipmentDefinitionId;
use crate::maintenance::ActiveConditionDurationError;

use super::super::ManualPowerMethodId;

/// Failure while projecting manual power from immutable authored definitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualPowerProjectionError {
    UnknownMethod {
        method: ManualPowerMethodId,
    },
    UnknownEquipmentDefinition {
        equipment: EquipmentDefinitionId,
    },
    EquipmentRequiresStructuralSupport {
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
            Self::EquipmentRequiresStructuralSupport { equipment } => write!(
                formatter,
                "manual-power equipment definition {} requires structural installation but direct manual-power equipment must be portable",
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
            | Self::EquipmentRequiresStructuralSupport { .. }
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
