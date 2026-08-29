//! Owns validated direct player-power work from admission through completion effects.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::{ManualPowerMethodId, PlayerWorkCommitError, PlayerWorkStartError};
use crate::capability::CapabilityValueKind;
use crate::core::quantity::{Energy, Power};
use crate::energy::{EnergyCarrier, EnergySinkError, EnergyStoreId};
use crate::equipment::{EquipmentId, EquipmentProviderError};
use crate::maintenance::ActiveConditionDurationError;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

/// Direct-labor request to place an exact quantity of generated work into one finite store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerRequest {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerRequest {
    #[must_use]
    pub const fn new(
        method: ManualPowerMethodId,
        equipment: EquipmentId,
        destination: EnergyStoreId,
        energy: Energy,
    ) -> Self {
        Self {
            method,
            equipment,
            destination,
            energy,
        }
    }
}

/// Failure while resolving one direct player-power work order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerError {
    UnknownMethod {
        method: ManualPowerMethodId,
    },
    Work(PlayerWorkStartError),
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    MissingPowerCapability {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    PowerCapabilityKindMismatch {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
        found: CapabilityValueKind,
    },
    ZeroEquipmentPower {
        equipment: EquipmentId,
        capability: crate::capability::CapabilityId,
    },
    EnergySink(EnergySinkError),
    WrongCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ZeroTransferPower {
        equipment: EquipmentId,
        destination: EnergyStoreId,
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
    ConditionDuration(ActiveConditionDurationError),
    CompletionTickOverflow {
        method: ManualPowerMethodId,
    },
}

impl Display for ManualPowerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown manual power method {}", method.value())
            }
            Self::Work(error) => write!(formatter, "manual power labor admission failed: {error}"),
            Self::Equipment(error) => write!(formatter, "manual power equipment failed: {error}"),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "manual power equipment {} is mounted and cannot be used for direct player-powered generation",
                equipment.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "manual power equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::MissingPowerCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} lacks authored power capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::PowerCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "manual power equipment {} capability {} has {found:?} value kind instead of Power",
                equipment.value(),
                capability.value()
            ),
            Self::ZeroEquipmentPower {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual power equipment {} capability {} currently resolves zero output power",
                equipment.value(),
                capability.value()
            ),
            Self::EnergySink(error) => {
                write!(formatter, "manual power destination failed: {error}")
            }
            Self::WrongCarrier { required, provided } => write!(
                formatter,
                "manual power method requires {required:?} storage but destination is {provided:?}"
            ),
            Self::ZeroTransferPower {
                equipment,
                destination,
            } => write!(
                formatter,
                "manual power equipment {} and destination store {} have no common transfer power",
                equipment.value(),
                destination.value()
            ),
            Self::PowerDuration { energy, power } => write!(
                formatter,
                "manual power output of {} nJ at {} pW cannot be transferred within the authoritative tick range",
                energy.nanojoules(),
                power.picowatts()
            ),
            Self::MetabolicConversionTooSmall { method } => write!(
                formatter,
                "manual power method {} metabolic conversion produces less than one nanojoule per active tick",
                method.value()
            ),
            Self::MetabolicDurationOverflow { method, energy } => write!(
                formatter,
                "manual power method {} requires more than the authoritative tick range to generate {} nJ",
                method.value(),
                energy.nanojoules()
            ),
            Self::ExertionResolution { method } => write!(
                formatter,
                "manual power method {} cannot resolve physiological effort for the requested output",
                method.value()
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "manual power work exceeds equipment condition lifetime: {error}"
            ),
            Self::CompletionTickOverflow { method } => write!(
                formatter,
                "manual power method {} completion exceeds the world clock range",
                method.value()
            ),
        }
    }
}

impl Error for ManualPowerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::UnknownMethod { method: _ }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::MissingPowerCapability { .. }
            | Self::PowerCapabilityKindMismatch { .. }
            | Self::ZeroEquipmentPower { .. }
            | Self::WrongCarrier { .. }
            | Self::ZeroTransferPower { .. }
            | Self::PowerDuration { .. }
            | Self::MetabolicConversionTooSmall { .. }
            | Self::MetabolicDurationOverflow { .. }
            | Self::ExertionResolution { .. }
            | Self::CompletionTickOverflow { .. } => None,
        }
    }
}

/// Commit-time conflict for a resolved direct player-power start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualPowerCommitError {
    Work(PlayerWorkCommitError),
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleEnergyRevision {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EnergyBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
}

impl Display for ManualPowerCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(
                formatter,
                "manual power labor changed after validation: {error}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "manual power expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "manual power expected energy revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by production job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "manual power equipment {} became occupied by mining job {} after validation",
                equipment.value(),
                job.value()
            ),
            Self::EnergyBusyProduction { store, job } => write!(
                formatter,
                "manual power destination store {} became occupied by production job {} after validation",
                store.value(),
                job.value()
            ),
        }
    }
}

impl Error for ManualPowerCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EnergyBusyProduction { .. } => None,
        }
    }
}

/// Observable completion of one direct player-powered generation work order.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManualPowerOutcome {
    method: ManualPowerMethodId,
    equipment: EquipmentId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl ManualPowerOutcome {
    #[must_use]
    pub const fn method(self) -> ManualPowerMethodId {
        self.method
    }
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }
}

mod start;
mod tick;

pub use start::{ValidatedManualPowerStart, validate_start_manual_power};
pub(crate) use tick::{ManualPowerTickError, apply_manual_power_tick, decide_manual_power_tick};

#[cfg(test)]
#[path = "power_execution_tests.rs"]
mod tests;
