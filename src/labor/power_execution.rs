//! Owns validated direct player-power work from admission through completion effects.

use super::ManualPowerMethodId;
use crate::core::quantity::Energy;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;

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

mod errors;

pub use errors::{ManualPowerCommitError, ManualPowerError};

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
