//! Conserved equipment-maintenance transaction boundary.
//!
//! Maintenance resolution is read-only and lives in `maintenance_resolution`. This module consumes
//! that opaque result, validates every mutable owner, performs the authored exact material service,
//! and commits the material and condition changes atomically.

use crate::core::quantity::Mass;
use crate::maintenance::Condition;
#[cfg(test)]
use crate::{AppState, Registries};

use super::state::EquipmentId;

mod commit;
mod errors;
mod material;
mod validation;

use material::ValidatedMaintenanceMaterial;

pub use errors::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceError, EquipmentMaintenanceMaterialError,
};
pub use validation::validate_equipment_maintenance;

#[cfg(test)]
use super::maintenance_resolution::{
    EquipmentMaintenanceRequest, EquipmentMaintenanceResolution,
    EquipmentMaintenanceResolutionError, resolve_equipment_maintenance,
};

/// Successful maintenance outcome after exact maintenance matter is consumed or exchanged.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceOutcome {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    material_mass: Mass,
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

    #[must_use]
    pub const fn material_mass(self) -> Mass {
        self.material_mass
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
}

impl ValidatedEquipmentMaintenance {
    #[must_use]
    pub fn material_mass(&self) -> Mass {
        self.material.material_mass()
    }
}

#[cfg(test)]
#[path = "maintenance_execution_tests.rs"]
mod tests;
