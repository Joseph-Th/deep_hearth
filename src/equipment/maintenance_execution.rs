//! Conserved equipment-maintenance transaction boundary.
//!
//! Maintenance resolution is read-only and lives in `maintenance_resolution`. This module consumes
//! that opaque result, validates every mutable owner, reforms exact replacement matter into the
//! authored spent form, and commits the material and condition changes atomically.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    MaterialReformCommitError, MaterialReformError, StockpileId, StockpileStorageError,
    StockpileStructuralLoadError, ValidatedMaterialReform, validate_material_reform_from_selection,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::definitions::EquipmentDefinitionId;
use super::maintenance_resolution::{EquipmentMaintenanceResolution, impure_replacement_commodity};
use super::state::EquipmentId;

#[cfg(test)]
use super::maintenance_resolution::{
    EquipmentMaintenanceRequest, EquipmentMaintenanceResolutionError, resolve_equipment_maintenance,
};

/// Failure while validating an already physically resolved equipment maintenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    StaleEquipmentResolution {
        equipment: EquipmentId,
        expected_revision: u64,
        actual_revision: u64,
    },
    ConditionChangedSinceResolution {
        equipment: EquipmentId,
        expected: Condition,
        actual: Condition,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    ConditionNotImproved {
        equipment: EquipmentId,
        before: Condition,
        after: Condition,
    },
    ImpureReplacementMaterial {
        commodity: CommodityKey,
    },
    EquipmentRevisionExhausted,
    Material(EquipmentMaintenanceMaterialError),
}

/// Public maintenance-facing translation of the crate-private exact material-reform boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceMaterialError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownSpentDestination {
        stockpile: StockpileId,
    },
    UnknownSpentMaterial {
        material: crate::material::MaterialId,
    },
    UnknownSpentForm {
        form: crate::material::FormId,
    },
    SpentMaterialChanged {
        source: crate::material::MaterialId,
        target: crate::material::MaterialId,
    },
    SpentPhaseChanged {
        replacement: crate::material::FormId,
        spent: crate::material::FormId,
    },
    SpentFormUnchanged {
        commodity: CommodityKey,
    },
    SpentStorage(StockpileStorageError),
    SpentMassOverflow {
        stockpile: StockpileId,
    },
    SpentCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    InventoryRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentMaintenanceMaterialError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSelection { expected, actual } => write!(
                formatter,
                "maintenance material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "maintenance material source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownSpentDestination { stockpile } => write!(
                formatter,
                "spent maintenance destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownSpentMaterial { material } => write!(
                formatter,
                "spent maintenance output references unknown material {}",
                material.value()
            ),
            Self::UnknownSpentForm { form } => write!(
                formatter,
                "spent maintenance output references unknown form {}",
                form.value()
            ),
            Self::SpentMaterialChanged { source, target } => write!(
                formatter,
                "equipment maintenance cannot change material identity from {} to {}",
                source.value(),
                target.value()
            ),
            Self::SpentPhaseChanged { replacement, spent } => write!(
                formatter,
                "equipment maintenance cannot change material phase from form {} to form {} without a thermal process",
                replacement.value(),
                spent.value()
            ),
            Self::SpentFormUnchanged { commodity } => write!(
                formatter,
                "equipment maintenance spent output must differ from replacement commodity {}",
                commodity.value()
            ),
            Self::SpentStorage(error) => {
                write!(
                    formatter,
                    "spent maintenance storage rejects material: {error}"
                )
            }
            Self::SpentMassOverflow { stockpile } => write!(
                formatter,
                "spent maintenance material overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::SpentCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "spent maintenance material exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter.write_str(
                "material lot identifier space is exhausted during equipment maintenance",
            ),
            Self::InventoryRevisionExhausted => formatter
                .write_str("inventory revision space is exhausted during equipment maintenance"),
            Self::StructuralLoad(error) => write!(
                formatter,
                "maintenance material movement cannot update stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceMaterialError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SpentStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StaleSelection {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownSource {
                stockpile: _stockpile,
            }
            | Self::UnknownSpentDestination {
                stockpile: _stockpile,
            }
            | Self::SpentMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownSpentMaterial {
                material: _material,
            }
            | Self::SpentMaterialChanged {
                source: _material,
                target: _,
            } => None,
            Self::SpentPhaseChanged {
                replacement: _replacement,
                spent: _spent,
            } => None,
            Self::SpentFormUnchanged {
                commodity: _commodity,
            } => None,
            Self::UnknownSpentForm { form: _form } => None,
            Self::SpentCapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            } => None,
            Self::LotIdExhausted | Self::InventoryRevisionExhausted => None,
        }
    }
}

impl Display for EquipmentMaintenanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {} during maintenance validation",
                equipment.value(),
                definition.value()
            ),
            Self::StaleEquipmentResolution {
                equipment,
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "equipment {} changed from maintenance-resolution revision {expected_revision} to {actual_revision} before transaction validation",
                equipment.value()
            ),
            Self::ConditionChangedSinceResolution {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from maintenance-resolution {} ppm to {} ppm before transaction validation",
                equipment.value(),
                expected.parts_per_million(),
                actual.parts_per_million()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be serviced",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be serviced",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be serviced",
                equipment.value()
            ),
            Self::ConditionNotImproved {
                equipment,
                before,
                after,
            } => write!(
                formatter,
                "equipment {} maintenance must improve condition above {} ppm; resolved outcome is {} ppm",
                equipment.value(),
                before.parts_per_million(),
                after.parts_per_million()
            ),
            Self::ImpureReplacementMaterial { commodity } => write!(
                formatter,
                "equipment maintenance replacement commodity {} must be pure authored material",
                commodity.value()
            ),
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted during maintenance")
            }
            Self::Material(error) => write!(
                formatter,
                "equipment maintenance material transaction is invalid: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Material(error) => Some(error),
            Self::UnknownEquipment {
                equipment: _equipment,
            } => None,
            Self::UnknownDefinition {
                equipment: _equipment,
                definition: _definition,
            } => None,
            Self::StaleEquipmentResolution {
                equipment: _equipment,
                expected_revision: _expected_revision,
                actual_revision: _actual_revision,
            } => None,
            Self::ConditionChangedSinceResolution {
                equipment: _equipment,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::EquipmentBusy {
                equipment: _equipment,
                job: _job,
                release: _release,
            } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
            Self::ConditionNotImproved {
                equipment: _equipment,
                before: _before,
                after: _after,
            } => None,
            Self::ImpureReplacementMaterial {
                commodity: _commodity,
            } => None,
            Self::EquipmentRevisionExhausted => None,
        }
    }
}

/// Commit failure after one or more maintenance owners changed since validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceCommitError {
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    ConditionChanged {
        equipment: EquipmentId,
        expected: Condition,
        actual: Condition,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentMaintenanceCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated equipment maintenance expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before maintenance commit",
                equipment.value()
            ),
            Self::ConditionChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from expected {} ppm to {} ppm before maintenance commit",
                equipment.value(),
                expected.parts_per_million(),
                actual.parts_per_million()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} became occupied by production job {} {release} before maintenance commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before maintenance commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before maintenance commit",
                equipment.value()
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated equipment maintenance expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "equipment maintenance material structural commit failed: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::UnknownEquipment {
                equipment: _equipment,
            } => None,
            Self::ConditionChanged {
                equipment: _equipment,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::EquipmentBusy {
                equipment: _equipment,
                job: _job,
                release: _release,
            } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
        }
    }
}

/// Successful maintenance outcome after exact maintenance matter is reformed into its spent output.
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
    material: ValidatedMaterialReform,
}

impl ValidatedEquipmentMaintenance {
    #[must_use]
    pub const fn material_mass(&self) -> Mass {
        self.material.total_mass()
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

        let material_mass = self.material.total_mass();
        self.material
            .commit(state)
            .map_err(map_material_commit_error)?;

        state.equipment_state_mut().apply_condition_change(
            self.equipment,
            self.condition_before,
            self.condition_after,
            self.next_equipment_revision,
        );

        Ok(EquipmentMaintenanceOutcome {
            equipment: self.equipment,
            condition_before: self.condition_before,
            condition_after: self.condition_after,
            material_mass,
        })
    }
}

fn map_material_error(error: MaterialReformError) -> EquipmentMaintenanceMaterialError {
    match error {
        MaterialReformError::StaleSelection { expected, actual } => {
            EquipmentMaintenanceMaterialError::StaleSelection { expected, actual }
        }
        MaterialReformError::UnknownSource { stockpile } => {
            EquipmentMaintenanceMaterialError::UnknownSource { stockpile }
        }
        MaterialReformError::UnknownDestination { stockpile } => {
            EquipmentMaintenanceMaterialError::UnknownSpentDestination { stockpile }
        }
        MaterialReformError::UnknownTargetMaterial { material } => {
            EquipmentMaintenanceMaterialError::UnknownSpentMaterial { material }
        }
        MaterialReformError::UnknownTargetForm { form } => {
            EquipmentMaintenanceMaterialError::UnknownSpentForm { form }
        }
        MaterialReformError::MaterialChanged { source, target } => {
            EquipmentMaintenanceMaterialError::SpentMaterialChanged { source, target }
        }
        MaterialReformError::PhaseChanged { source, target } => {
            EquipmentMaintenanceMaterialError::SpentPhaseChanged {
                replacement: source,
                spent: target,
            }
        }
        MaterialReformError::TargetUnchanged { commodity } => {
            EquipmentMaintenanceMaterialError::SpentFormUnchanged { commodity }
        }
        MaterialReformError::DestinationStorage(error) => {
            EquipmentMaintenanceMaterialError::SpentStorage(error)
        }
        MaterialReformError::DestinationMassOverflow { stockpile } => {
            EquipmentMaintenanceMaterialError::SpentMassOverflow { stockpile }
        }
        MaterialReformError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EquipmentMaintenanceMaterialError::SpentCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialReformError::LotIdExhausted => EquipmentMaintenanceMaterialError::LotIdExhausted,
        MaterialReformError::RevisionExhausted => {
            EquipmentMaintenanceMaterialError::InventoryRevisionExhausted
        }
        MaterialReformError::StructuralLoad(error) => {
            EquipmentMaintenanceMaterialError::StructuralLoad(error)
        }
    }
}

fn map_material_commit_error(error: MaterialReformCommitError) -> EquipmentMaintenanceCommitError {
    match error {
        MaterialReformCommitError::StaleInventoryRevision { expected, actual } => {
            EquipmentMaintenanceCommitError::StaleInventoryRevision { expected, actual }
        }
        MaterialReformCommitError::Structure(error) => {
            EquipmentMaintenanceCommitError::Structure(error)
        }
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
    if resolution.condition_after <= condition_before {
        return Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment,
            before: condition_before,
            after: resolution.condition_after,
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
    let material = validate_material_reform_from_selection(
        registries,
        state,
        resolution.spent_destination,
        resolution.spent,
        resolution.material,
    )
    .map_err(map_material_error)
    .map_err(EquipmentMaintenanceError::Material)?;

    Ok(ValidatedEquipmentMaintenance {
        equipment,
        condition_before,
        condition_after: resolution.condition_after,
        expected_equipment_revision: state.equipment().revision(),
        next_equipment_revision,
        material,
    })
}

#[cfg(test)]
#[path = "maintenance_execution_tests.rs"]
mod tests;
