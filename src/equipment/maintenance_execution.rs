//! Conserved equipment-maintenance transaction boundary.
//!
//! Maintenance resolution is read-only and lives in `maintenance_resolution`. This module consumes
//! that opaque result, validates every mutable owner, performs the authored exact material service,
//! and admits the material exchange plus durable player work atomically. Condition recovery occurs
//! only when the authored service interval completes.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::labor::{EquipmentMaintenanceWork, ValidatedPlayerWorkStart};
use crate::maintenance::Condition;
#[cfg(test)]
use crate::{AppState, Registries};

use super::state::EquipmentId;

mod commit;
mod errors;
mod material;
mod tick;
mod validation;

use material::ValidatedMaintenanceMaterial;

pub use errors::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceError, EquipmentMaintenanceMaterialError,
};
pub(crate) use tick::{apply_equipment_maintenance_tick, decide_equipment_maintenance_tick};
pub use validation::validate_equipment_maintenance;

#[cfg(test)]
use super::maintenance_resolution::{
    EquipmentMaintenanceRequest, EquipmentMaintenanceResolution,
    EquipmentMaintenanceResolutionError, resolve_equipment_maintenance,
};

/// Successful admission of one exact material-backed maintenance interval.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceStartOutcome {
    equipment: EquipmentId,
    condition_before: Condition,
    target_condition: Condition,
    material_mass: Mass,
    completes_at: SimulationTick,
}

impl EquipmentMaintenanceStartOutcome {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn condition_before(self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn target_condition(self) -> Condition {
        self.target_condition
    }

    #[must_use]
    pub const fn material_mass(self) -> Mass {
        self.material_mass
    }

    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Condition recovery emitted only when the service interval reaches its scheduled completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceOutcome {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
}

impl EquipmentMaintenanceOutcome {
    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }
    #[must_use]
    pub const fn condition_before(self) -> Condition {
        self.condition_before
    }
    #[must_use]
    pub const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Consumed proof that equipment and exact maintenance material can change atomically.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedEquipmentMaintenance {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    material: ValidatedMaintenanceMaterial,
    work: EquipmentMaintenanceWork,
    player_work: ValidatedPlayerWorkStart,
}

impl ValidatedEquipmentMaintenance {
    #[must_use]
    pub fn material_mass(&self) -> Mass {
        self.material.material_mass()
    }

    #[must_use]
    pub const fn work(&self) -> EquipmentMaintenanceWork {
        self.work
    }
}

#[cfg(test)]
#[path = "maintenance_execution_tests.rs"]
mod tests;
