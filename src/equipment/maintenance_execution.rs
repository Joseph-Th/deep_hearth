//! Conserved equipment-maintenance transaction boundary.
//!
//! Maintenance resolution is read-only and lives in `maintenance_resolution`. This module consumes
//! that opaque result, validates every mutable owner, performs the authored exact material service,
//! and commits the material and condition changes atomically.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::maintenance::Condition;
use crate::registry::Registries;

use super::maintenance_resolution::{EquipmentMaintenanceResolution, impure_replacement_commodity};
use super::state::EquipmentId;

mod errors;
mod material;

use material::{ValidatedMaintenanceMaterial, validate_maintenance_material};

pub use errors::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceError, EquipmentMaintenanceMaterialError,
};

#[cfg(test)]
use super::maintenance_resolution::{
    EquipmentMaintenanceRequest, EquipmentMaintenanceResolutionError, resolve_equipment_maintenance,
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
    pub const fn material_mass(&self) -> Mass {
        self.material.material_mass()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentMaintenanceOutcome, EquipmentMaintenanceCommitError> {
        let actual_revision = state.equipment().revision();
        if actual_revision != self.expected_equipment_revision {
            return Err(EquipmentMaintenanceCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_revision,
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.equipment) {
            return Err(EquipmentMaintenanceCommitError::EquipmentBusyMining {
                equipment: self.equipment,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_equipment_occupant(self.equipment)
            .is_some()
        {
            return Err(EquipmentMaintenanceCommitError::EquipmentBusyManualPower {
                equipment: self.equipment,
            });
        }
        let Some(record) = state.equipment().get_equipment(self.equipment) else {
            return Err(EquipmentMaintenanceCommitError::UnknownEquipment {
                equipment: self.equipment,
            });
        };
        if record.condition() != self.condition_before {
            return Err(EquipmentMaintenanceCommitError::ConditionChanged {
                equipment: self.equipment,
                expected: self.condition_before,
                actual: record.condition(),
            });
        }
        if let Some(job) = state.production().get_equipment_occupant(self.equipment) {
            return Err(EquipmentMaintenanceCommitError::EquipmentBusy {
                equipment: self.equipment,
                job: job.id(),
                release: job.occupancy_release(),
            });
        }

        let material_mass = self.material.material_mass();
        self.material.commit(
            state,
            self.equipment,
            self.condition_before,
            self.condition_after,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        )?;

        Ok(EquipmentMaintenanceOutcome {
            equipment: self.equipment,
            condition_before: self.condition_before,
            condition_after: self.condition_after,
            material_mass,
        })
    }
}

/// Validates one already-resolved, resource-backed equipment maintenance without mutating any owner.
pub fn validate_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    resolution: EquipmentMaintenanceResolution,
) -> Result<ValidatedEquipmentMaintenance, EquipmentMaintenanceError> {
    let equipment = resolution.equipment;
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentMaintenanceError::UnknownEquipment { equipment })?;
    let actual_equipment_revision = state.equipment().revision();
    if actual_equipment_revision != resolution.expected_equipment_revision {
        return Err(EquipmentMaintenanceError::StaleEquipmentResolution {
            equipment,
            expected_revision: resolution.expected_equipment_revision,
            actual_revision: actual_equipment_revision,
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentMaintenanceError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentMaintenanceError::EquipmentBusyManualPower { equipment });
    }
    if record.condition() != resolution.condition_before {
        return Err(EquipmentMaintenanceError::ConditionChangedSinceResolution {
            equipment,
            expected: resolution.condition_before,
            actual: record.condition(),
        });
    }
    if registries
        .equipment()
        .get_equipment(record.definition())
        .is_none()
    {
        return Err(EquipmentMaintenanceError::UnknownDefinition {
            equipment,
            definition: record.definition(),
        });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentMaintenanceError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    let condition_before = resolution.condition_before;
    let condition_after = resolution.condition_after;
    if condition_after <= condition_before {
        return Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment,
            before: condition_before,
            after: condition_after,
        });
    }
    if let Some(commodity) = impure_replacement_commodity(&resolution.material) {
        return Err(EquipmentMaintenanceError::ImpureReplacementMaterial { commodity });
    }
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(EquipmentMaintenanceError::EquipmentRevisionExhausted)?;
    let expected_equipment_revision = state.equipment().revision();
    let material = validate_maintenance_material(registries, state, record, resolution)
        .map_err(EquipmentMaintenanceError::Material)?;

    Ok(ValidatedEquipmentMaintenance {
        equipment,
        condition_before,
        condition_after,
        expected_equipment_revision,
        next_equipment_revision,
        material,
    })
}

#[cfg(test)]
#[path = "maintenance_execution_tests.rs"]
mod tests;
