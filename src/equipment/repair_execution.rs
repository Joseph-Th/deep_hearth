//! Authored equipment maintenance resolution and conserved repair transaction boundary.
//!
//! Equipment definitions may author one replacement-material maintenance profile. Runtime resolution
//! selects that exact commodity from a requested source and binds the profile's restored condition.
//! The canonical transaction then moves the selected matter into an explicit spent-material
//! destination while condition improves. Labor, tooling, access, and maintenance duration can extend
//! this physical resolver when those owners exist without reopening a free condition mutation path.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, MaterialRelocationCommitError,
    MaterialRelocationError, StockpileId, StockpileStorageError, StockpileStructuralLoadError,
    ValidatedMaterialRelocation, validate_consumption_selection,
    validate_material_relocation_from_selection,
};
#[cfg(test)]
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialLotSelection,
    validate_explicit_consumption_selection,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::definitions::EquipmentDefinitionId;
use super::state::EquipmentId;

/// Opaque result of physical maintenance resolution.
///
/// Production callers cannot construct this directly. The maintenance resolver binds exact authored
/// replacement material and resulting equipment condition before this transaction can be validated.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentRepairResolution {
    equipment: EquipmentId,
    expected_equipment_revision: u64,
    condition_before: Condition,
    condition_after: Condition,
    material: ConsumptionSelection,
    spent_destination: StockpileId,
}

/// Player/system request to service one idle equipment instance from explicit replacement stock.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentMaintenanceRequest {
    equipment: EquipmentId,
    material_source: StockpileId,
    spent_destination: StockpileId,
}

impl EquipmentMaintenanceRequest {
    pub const fn new(
        equipment: EquipmentId,
        material_source: StockpileId,
        spent_destination: StockpileId,
    ) -> Self {
        Self {
            equipment,
            material_source,
            spent_destination,
        }
    }
}

/// Failure while resolving an authored replacement-material maintenance action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentMaintenanceResolutionError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    NoMaintenanceProfile {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    ConditionAtOrAboveServiceTarget {
        equipment: EquipmentId,
        current: Condition,
        target: Condition,
    },
    UnknownMaterialSource {
        stockpile: StockpileId,
    },
    InsufficientReplacementMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    MaterialSelectionMassOverflow {
        stockpile: StockpileId,
    },
}

impl Display for EquipmentMaintenanceResolutionError {
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
                "equipment {} references unknown definition {} during maintenance resolution",
                equipment.value(),
                definition.value()
            ),
            Self::NoMaintenanceProfile {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} definition {} has no authored maintenance profile",
                equipment.value(),
                definition.value()
            ),
            Self::ConditionAtOrAboveServiceTarget {
                equipment,
                current,
                target,
            } => write!(
                formatter,
                "equipment {} condition {} ppm is already at or above maintenance target {} ppm",
                equipment.value(),
                current.parts_per_million(),
                target.parts_per_million()
            ),
            Self::UnknownMaterialSource { stockpile } => write!(
                formatter,
                "maintenance replacement source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InsufficientReplacementMaterial {
                stockpile,
                commodity,
                available,
                required,
            } => write!(
                formatter,
                "maintenance source stockpile {} has {} mg of commodity {} but {} mg is required",
                stockpile.value(),
                available.milligrams(),
                commodity.value(),
                required.milligrams()
            ),
            Self::MaterialSelectionMassOverflow { stockpile } => write!(
                formatter,
                "maintenance replacement selection overflows mass accounting in stockpile {}",
                stockpile.value()
            ),
        }
    }
}

impl Error for EquipmentMaintenanceResolutionError {}

/// Resolves the equipment definition's authored replacement-material service against current state.
pub fn resolve_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    request: EquipmentMaintenanceRequest,
) -> Result<EquipmentRepairResolution, EquipmentMaintenanceResolutionError> {
    let record = state.equipment().get_equipment(request.equipment).ok_or(
        EquipmentMaintenanceResolutionError::UnknownEquipment {
            equipment: request.equipment,
        },
    )?;
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .ok_or(EquipmentMaintenanceResolutionError::UnknownDefinition {
            equipment: request.equipment,
            definition: record.definition(),
        })?;
    let profile = definition.maintenance_profile().ok_or(
        EquipmentMaintenanceResolutionError::NoMaintenanceProfile {
            equipment: request.equipment,
            definition: record.definition(),
        },
    )?;
    let condition_before = record.condition();
    if condition_before >= profile.restored_condition() {
        return Err(
            EquipmentMaintenanceResolutionError::ConditionAtOrAboveServiceTarget {
                equipment: request.equipment,
                current: condition_before,
                target: profile.restored_condition(),
            },
        );
    }
    let material = validate_consumption_selection(
        state.inventory(),
        request.material_source,
        &[MaterialInputSpec::new(
            profile.replacement(),
            profile.replacement_mass(),
        )],
    )
    .map_err(|error| match error {
        ConsumptionSelectionError::UnknownStockpile { stockpile } => {
            EquipmentMaintenanceResolutionError::UnknownMaterialSource { stockpile }
        }
        ConsumptionSelectionError::InsufficientMass {
            stockpile,
            commodity,
            available,
            requested,
        } => EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
            stockpile,
            commodity,
            available,
            required: requested,
        },
        ConsumptionSelectionError::MassOverflow { stockpile } => {
            EquipmentMaintenanceResolutionError::MaterialSelectionMassOverflow { stockpile }
        }
    })?;

    Ok(EquipmentRepairResolution {
        equipment: request.equipment,
        expected_equipment_revision: state.equipment().revision(),
        condition_before,
        condition_after: profile.restored_condition(),
        material,
        spent_destination: request.spent_destination,
    })
}

impl EquipmentRepairResolution {
    #[must_use]
    pub const fn equipment(&self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn condition_before(&self) -> Condition {
        self.condition_before
    }

    #[must_use]
    pub const fn condition_after(&self) -> Condition {
        self.condition_after
    }

    #[must_use]
    pub const fn material_source(&self) -> StockpileId {
        self.material.source()
    }

    #[must_use]
    pub const fn spent_destination(&self) -> StockpileId {
        self.spent_destination
    }

    #[must_use]
    pub const fn material_mass(&self) -> Mass {
        self.material.total_consumed()
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentRepairBindingError {
    UnknownEquipment { equipment: EquipmentId },
    Inventory(ExplicitConsumptionSelectionError),
}

/// Test-side stand-in for custom repair resolutions not represented by an authored maintenance profile.
#[cfg(test)]
pub(crate) fn bind_equipment_repair_for_test(
    state: &AppState,
    equipment: EquipmentId,
    source: StockpileId,
    selections: &[MaterialLotSelection],
    spent_destination: StockpileId,
    condition_after: Condition,
) -> Result<EquipmentRepairResolution, EquipmentRepairBindingError> {
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentRepairBindingError::UnknownEquipment { equipment })?;
    let material = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .map_err(EquipmentRepairBindingError::Inventory)?;
    Ok(EquipmentRepairResolution {
        equipment,
        expected_equipment_revision: state.equipment().revision(),
        condition_before: record.condition(),
        condition_after,
        material,
        spent_destination,
    })
}

/// Failure while validating an already physically resolved equipment repair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentRepairError {
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
    ConditionNotImproved {
        equipment: EquipmentId,
        before: Condition,
        after: Condition,
    },
    EquipmentRevisionExhausted,
    Material(EquipmentRepairMaterialError),
}

/// Public repair-facing translation of the crate-private exact relocation boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentRepairMaterialError {
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
    SpentDestinationIsSource {
        stockpile: StockpileId,
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

impl Display for EquipmentRepairMaterialError {
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
            Self::SpentDestinationIsSource { stockpile } => write!(
                formatter,
                "spent maintenance material must leave source stockpile {}",
                stockpile.value()
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
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during equipment repair"),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during equipment repair")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "maintenance material movement cannot update stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EquipmentRepairMaterialError {
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
            | Self::SpentDestinationIsSource {
                stockpile: _stockpile,
            }
            | Self::SpentMassOverflow {
                stockpile: _stockpile,
            } => None,
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

impl Display for EquipmentRepairError {
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
                "equipment {} references unknown definition {} during repair validation",
                equipment.value(),
                definition.value()
            ),
            Self::StaleEquipmentResolution {
                equipment,
                expected_revision,
                actual_revision,
            } => write!(
                formatter,
                "equipment {} changed from repair-resolution revision {expected_revision} to {actual_revision} before transaction validation",
                equipment.value()
            ),
            Self::ConditionChangedSinceResolution {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from repair-resolution {} ppm to {} ppm before transaction validation",
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
                "equipment {} is occupied by production job {} {release} and cannot be repaired",
                equipment.value(),
                job.value()
            ),
            Self::ConditionNotImproved {
                equipment,
                before,
                after,
            } => write!(
                formatter,
                "equipment {} repair must improve condition above {} ppm; resolved outcome is {} ppm",
                equipment.value(),
                before.parts_per_million(),
                after.parts_per_million()
            ),
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted during repair")
            }
            Self::Material(error) => write!(
                formatter,
                "equipment repair material transaction is invalid: {error}"
            ),
        }
    }
}

impl Error for EquipmentRepairError {
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
            Self::ConditionNotImproved {
                equipment: _equipment,
                before: _before,
                after: _after,
            } => None,
            Self::EquipmentRevisionExhausted => None,
        }
    }
}

/// Commit failure after one or more repair owners changed since validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentRepairCommitError {
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
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentRepairCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated equipment repair expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before repair commit",
                equipment.value()
            ),
            Self::ConditionChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} condition changed from expected {} ppm to {} ppm before repair commit",
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
                "equipment {} became occupied by production job {} {release} before repair commit",
                equipment.value(),
                job.value()
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated equipment repair expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "equipment repair material structural commit failed: {error}"
            ),
        }
    }
}

impl Error for EquipmentRepairCommitError {
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
        }
    }
}

/// Successful repair outcome after exact maintenance matter is relocated to its spent destination.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentRepairOutcome {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    material_mass: Mass,
}

impl EquipmentRepairOutcome {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEquipmentRepair {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    material: ValidatedMaterialRelocation,
}

impl ValidatedEquipmentRepair {
    #[must_use]
    pub const fn material_mass(&self) -> Mass {
        self.material.total_mass()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentRepairOutcome, EquipmentRepairCommitError> {
        let actual_revision = state.equipment().revision();
        if actual_revision != self.expected_equipment_revision {
            return Err(EquipmentRepairCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_revision,
            });
        }
        let Some(record) = state.equipment().get_equipment(self.equipment) else {
            return Err(EquipmentRepairCommitError::UnknownEquipment {
                equipment: self.equipment,
            });
        };
        if record.condition() != self.condition_before {
            return Err(EquipmentRepairCommitError::ConditionChanged {
                equipment: self.equipment,
                expected: self.condition_before,
                actual: record.condition(),
            });
        }
        if let Some(job) = state.production().get_equipment_occupant(self.equipment) {
            return Err(EquipmentRepairCommitError::EquipmentBusy {
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

        Ok(EquipmentRepairOutcome {
            equipment: self.equipment,
            condition_before: self.condition_before,
            condition_after: self.condition_after,
            material_mass,
        })
    }
}

fn map_material_error(error: MaterialRelocationError) -> EquipmentRepairMaterialError {
    match error {
        MaterialRelocationError::StaleSelection { expected, actual } => {
            EquipmentRepairMaterialError::StaleSelection { expected, actual }
        }
        MaterialRelocationError::UnknownSource { stockpile } => {
            EquipmentRepairMaterialError::UnknownSource { stockpile }
        }
        MaterialRelocationError::UnknownDestination { stockpile } => {
            EquipmentRepairMaterialError::UnknownSpentDestination { stockpile }
        }
        MaterialRelocationError::SameStockpile { stockpile } => {
            EquipmentRepairMaterialError::SpentDestinationIsSource { stockpile }
        }
        MaterialRelocationError::DestinationStorage(error) => {
            EquipmentRepairMaterialError::SpentStorage(error)
        }
        MaterialRelocationError::DestinationMassOverflow { stockpile } => {
            EquipmentRepairMaterialError::SpentMassOverflow { stockpile }
        }
        MaterialRelocationError::DestinationCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        } => EquipmentRepairMaterialError::SpentCapacityExceeded {
            stockpile,
            capacity,
            committed,
            requested,
        },
        MaterialRelocationError::LotIdExhausted => EquipmentRepairMaterialError::LotIdExhausted,
        MaterialRelocationError::RevisionExhausted => {
            EquipmentRepairMaterialError::InventoryRevisionExhausted
        }
        MaterialRelocationError::StructuralLoad(error) => {
            EquipmentRepairMaterialError::StructuralLoad(error)
        }
    }
}

fn map_material_commit_error(error: MaterialRelocationCommitError) -> EquipmentRepairCommitError {
    match error {
        MaterialRelocationCommitError::StaleInventoryRevision { expected, actual } => {
            EquipmentRepairCommitError::StaleInventoryRevision { expected, actual }
        }
        MaterialRelocationCommitError::Structure(error) => {
            EquipmentRepairCommitError::Structure(error)
        }
    }
}

/// Validates one already-resolved, resource-backed equipment repair without mutating any owner.
pub fn validate_equipment_repair(
    registries: &Registries,
    state: &AppState,
    resolution: EquipmentRepairResolution,
) -> Result<ValidatedEquipmentRepair, EquipmentRepairError> {
    let equipment = resolution.equipment;
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentRepairError::UnknownEquipment { equipment })?;
    let actual_equipment_revision = state.equipment().revision();
    if actual_equipment_revision != resolution.expected_equipment_revision {
        return Err(EquipmentRepairError::StaleEquipmentResolution {
            equipment,
            expected_revision: resolution.expected_equipment_revision,
            actual_revision: actual_equipment_revision,
        });
    }
    if record.condition() != resolution.condition_before {
        return Err(EquipmentRepairError::ConditionChangedSinceResolution {
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
        return Err(EquipmentRepairError::UnknownDefinition {
            equipment,
            definition: record.definition(),
        });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentRepairError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    let condition_before = resolution.condition_before;
    if resolution.condition_after <= condition_before {
        return Err(EquipmentRepairError::ConditionNotImproved {
            equipment,
            before: condition_before,
            after: resolution.condition_after,
        });
    }
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(EquipmentRepairError::EquipmentRevisionExhausted)?;
    let material = validate_material_relocation_from_selection(
        registries,
        state,
        resolution.spent_destination,
        resolution.material,
    )
    .map_err(map_material_error)
    .map_err(EquipmentRepairError::Material)?;

    Ok(ValidatedEquipmentRepair {
        equipment,
        condition_before,
        condition_after: resolution.condition_after,
        expected_equipment_revision: state.equipment().revision(),
        next_equipment_revision,
        material,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
    };
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        make_test_registries_with_equipment, make_test_registries_with_sensible_heating,
    };
    use crate::core::quantity::{AggregateMass, Area, Energy, Force, Length, Power, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId,
        ExplicitEnergyAccountingError, add_energy_store_with_initial_for_test,
        calculate_explicit_energy_accounting,
    };
    use crate::equipment::{
        EquipmentDefinition, EquipmentMaintenanceProfile, add_equipment,
        apply_equipment_condition_plan, decide_equipment_wear,
    };

    use crate::inventory::validate_transfer_bulk;
    use crate::inventory::{
        MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test,
        validate_mount_stockpile,
    };
    use crate::maintenance::MaintenanceThresholds;
    use crate::material::CommodityKey;
    use crate::matter::calculate_matter_accounting;
    use crate::production::{ProcessDefinition, ProcessId, validate_start_process};
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralLoadKind, add_structural_element, calculate_aggregate_weight_force_ceiling,
        materialize_structural_element_for_test, validate_activate_structural_element,
    };
    use crate::thermal::{
        SensibleHeatingProcessDefinition, SensibleHeatingRequest, resolve_sensible_heating_process,
    };

    const TEST_CAPABILITY: CapabilityId = CapabilityId::new(812_001);
    const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(812_001);
    const HEATING_POWER: CapabilityId = CapabilityId::new(812_002);
    const MAX_TEMPERATURE: CapabilityId = CapabilityId::new(812_003);
    const MAX_BATCH_MASS: CapabilityId = CapabilityId::new(812_004);
    const ENERGY_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(812_001);
    const HEATING_PROCESS: ProcessId = ProcessId::new(812_001);

    fn condition(parts_per_million: u32) -> Condition {
        match Condition::new(parts_per_million) {
            Ok(condition) => condition,
            Err(error) => panic!("repair condition fixture failed: {error}"),
        }
    }

    fn registries() -> Registries {
        let profile = match CapabilityProfile::new([(
            TEST_CAPABILITY,
            CapabilityValue::Mass(Mass::from_milligrams(50_000)),
        )]) {
            Ok(profile) => profile,
            Err(error) => panic!("repair capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("repair maintenance fixture failed: {error}"),
        };
        make_test_registries_with_equipment(
            CapabilityDefinition::new(
                TEST_CAPABILITY,
                "repair fixture supported mass",
                CapabilityValueKind::Mass,
            ),
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "repair fixture press",
                Mass::from_milligrams(40_000),
                profile,
                thresholds,
            )
            .with_maintenance_profile(EquipmentMaintenanceProfile::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(7),
                condition(700_000),
            )),
        )
    }

    fn occupied_registries() -> Registries {
        let profile = match CapabilityProfile::new([
            (
                HEATING_POWER,
                CapabilityValue::Power(Power::from_microwatts(1_000_000)),
            ),
            (
                MAX_TEMPERATURE,
                CapabilityValue::Temperature(Temperature::from_millikelvin(400_000)),
            ),
            (
                MAX_BATCH_MASS,
                CapabilityValue::Mass(Mass::from_milligrams(20)),
            ),
        ]) {
            Ok(profile) => profile,
            Err(error) => panic!("repair occupancy capability fixture failed: {error}"),
        };
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("repair occupancy maintenance fixture failed: {error}"),
        };
        make_test_registries_with_sensible_heating(
            vec![
                CapabilityDefinition::new(
                    HEATING_POWER,
                    "repair occupancy heating power",
                    CapabilityValueKind::Power,
                ),
                CapabilityDefinition::new(
                    MAX_TEMPERATURE,
                    "repair occupancy maximum temperature",
                    CapabilityValueKind::Temperature,
                ),
                CapabilityDefinition::new(
                    MAX_BATCH_MASS,
                    "repair occupancy maximum batch mass",
                    CapabilityValueKind::Mass,
                ),
            ],
            EquipmentDefinition::new(
                TEST_DEFINITION,
                "repair occupancy heater",
                Mass::from_milligrams(40_000),
                profile,
                thresholds,
            ),
            EnergyStoreDefinition::new(
                ENERGY_DEFINITION,
                "repair occupancy battery",
                EnergyCarrier::Electrical,
                Energy::from_nanojoules(1_000_000_000),
                Power::from_microwatts(500_000),
            ),
            ProcessDefinition::new_selected_batch(
                HEATING_PROCESS,
                "repair occupancy sensible heating",
                Vec::new(),
            ),
            SensibleHeatingProcessDefinition::new(
                HEATING_PROCESS,
                HEATING_POWER,
                MAX_TEMPERATURE,
                MAX_BATCH_MASS,
                EnergyCarrier::Electrical,
                1,
            ),
        )
    }

    fn explicit_energy(registries: &Registries, state: &AppState) -> Energy {
        match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
            accounting
                .total()
                .ok_or(ExplicitEnergyAccountingError::Overflow)
        }) {
            Ok(total) => total,
            Err(error) => panic!("repair energy accounting failed: {error}"),
        }
    }

    fn add_material(
        registries: &Registries,
        state: &mut AppState,
        stockpile: StockpileId,
        mass: Mass,
    ) -> crate::inventory::MaterialLotId {
        match deposit_lot_for_test(
            registries,
            state,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            mass,
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("repair material fixture failed: {error}"),
        }
    }

    fn bind(
        state: &AppState,
        equipment: EquipmentId,
        source: StockpileId,
        lot: crate::inventory::MaterialLotId,
        mass: Mass,
        spent: StockpileId,
        after: Condition,
    ) -> EquipmentRepairResolution {
        match bind_equipment_repair_for_test(
            state,
            equipment,
            source,
            &[MaterialLotSelection::new(lot, mass)],
            spent,
            after,
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("repair binding fixture failed: {error:?}"),
        }
    }

    #[test]
    fn authored_maintenance_resolution_binds_exact_replacement_stock_and_service_target() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0000));
        let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
            .unwrap_or_else(|error| {
                panic!("maintenance resolver equipment fixture failed: {error}")
            });
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
            .unwrap_or_else(|error| panic!("maintenance resolver source fixture failed: {error}"));
        let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
            .unwrap_or_else(|error| panic!("maintenance resolver spent fixture failed: {error}"));
        add_material(&registries, &mut state, source, Mass::from_milligrams(20));

        let resolution = resolve_equipment_maintenance(
            &registries,
            &state,
            EquipmentMaintenanceRequest::new(equipment, source, spent),
        )
        .unwrap_or_else(|error| panic!("maintenance resolution failed: {error}"));
        assert_eq!(resolution.equipment(), equipment);
        assert_eq!(resolution.material_source(), source);
        assert_eq!(resolution.spent_destination(), spent);
        assert_eq!(resolution.material_mass(), Mass::from_milligrams(7));
        assert_eq!(resolution.condition_before(), condition(500_000));
        assert_eq!(resolution.condition_after(), condition(700_000));

        let outcome = validate_equipment_repair(&registries, &state, resolution)
            .unwrap_or_else(|error| panic!("maintenance transaction validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("maintenance transaction commit failed: {error}"));
        assert_eq!(outcome.condition_before(), condition(500_000));
        assert_eq!(outcome.condition_after(), condition(700_000));
        assert_eq!(outcome.material_mass(), Mass::from_milligrams(7));
        assert_eq!(
            state
                .inventory()
                .get_stockpile(source)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(13))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(spent)
                .map(|record| record.stored_mass()),
            Some(Mass::from_milligrams(7))
        );
    }

    #[test]
    fn authored_maintenance_resolution_rejects_unneeded_or_understocked_service() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_000A));
        let healthy = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(700_000))
            .unwrap_or_else(|error| {
                panic!("healthy maintenance equipment fixture failed: {error}")
            });
        let worn = add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000))
            .unwrap_or_else(|error| panic!("worn maintenance equipment fixture failed: {error}"));
        let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
            .unwrap_or_else(|error| panic!("maintenance stock fixture failed: {error}"));
        let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
            .unwrap_or_else(|error| panic!("maintenance spent fixture failed: {error}"));
        add_material(&registries, &mut state, source, Mass::from_milligrams(6));

        assert_eq!(
            resolve_equipment_maintenance(
                &registries,
                &state,
                EquipmentMaintenanceRequest::new(healthy, source, spent),
            ),
            Err(
                EquipmentMaintenanceResolutionError::ConditionAtOrAboveServiceTarget {
                    equipment: healthy,
                    current: condition(700_000),
                    target: condition(700_000),
                }
            )
        );
        assert_eq!(
            resolve_equipment_maintenance(
                &registries,
                &state,
                EquipmentMaintenanceRequest::new(worn, source, spent),
            ),
            Err(
                EquipmentMaintenanceResolutionError::InsufficientReplacementMaterial {
                    stockpile: source,
                    commodity: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                    available: Mass::from_milligrams(6),
                    required: Mass::from_milligrams(7),
                }
            )
        );
    }

    #[test]
    fn repair_moves_exact_material_to_spent_storage_and_preserves_conservation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0001));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(20));
        let source_lot = match state.inventory().get_lot(lot) {
            Some(record) => record,
            None => panic!("repair source lot disappeared"),
        };
        let commodity_before = source_lot.commodity();
        let temperature_before = source_lot.temperature();
        let composition_before = source_lot.composition().clone();
        let particle_size_before = source_lot.particle_size();
        let created_before = source_lot.created_at();
        let latest_before = source_lot.latest_created_at();
        let matter_before = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("repair initial matter accounting failed: {error}"),
        };
        let energy_before = explicit_energy(&registries, &state);
        let resolution = bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(7),
            spent,
            condition(700_000),
        );
        let token = match validate_equipment_repair(&registries, &state, resolution) {
            Ok(token) => token,
            Err(error) => panic!("repair validation failed: {error}"),
        };
        assert_eq!(token.material_mass(), Mass::from_milligrams(7));

        let outcome = match token.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("repair commit failed: {error}"),
        };

        assert_eq!(outcome.condition_before(), condition(500_000));
        assert_eq!(outcome.condition_after(), condition(700_000));
        assert_eq!(outcome.material_mass(), Mass::from_milligrams(7));
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(condition(700_000))
        );
        assert_eq!(
            state.inventory().get_lot(lot).map(|record| record.mass()),
            Some(Mass::from_milligrams(13))
        );
        let spent_lot = match state
            .inventory()
            .lots()
            .find(|record| record.stockpile() == spent)
        {
            Some(record) => record,
            None => panic!("repair spent material missing"),
        };
        assert_eq!(spent_lot.mass(), Mass::from_milligrams(7));
        assert_eq!(spent_lot.commodity(), commodity_before);
        assert_eq!(spent_lot.temperature(), temperature_before);
        assert_eq!(spent_lot.composition(), &composition_before);
        assert_eq!(spent_lot.particle_size(), particle_size_before);
        assert_eq!(spent_lot.created_at(), created_before);
        assert_eq!(spent_lot.latest_created_at(), latest_before);
        assert_eq!(
            calculate_matter_accounting(&state).map(|accounting| accounting.total()),
            Ok(matter_before)
        );
        assert_eq!(explicit_energy(&registries, &state), energy_before);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn repair_rejects_non_improvement_and_same_spent_destination_without_mutation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0002));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair rejection equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair rejection source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair rejection spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
        let before = state.clone();

        let no_improvement = bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(1),
            spent,
            condition(500_000),
        );
        assert_eq!(
            validate_equipment_repair(&registries, &state, no_improvement),
            Err(EquipmentRepairError::ConditionNotImproved {
                equipment,
                before: condition(500_000),
                after: condition(500_000),
            })
        );

        let same_destination = bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(1),
            source,
            condition(600_000),
        );
        assert_eq!(
            validate_equipment_repair(&registries, &state, same_destination),
            Err(EquipmentRepairError::Material(
                EquipmentRepairMaterialError::SpentDestinationIsSource { stockpile: source }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn repair_rechecks_inventory_and_equipment_before_any_partial_commit() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0003));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair stale equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair stale source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair stale spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));

        let inventory_stale = match validate_equipment_repair(
            &registries,
            &state,
            bind(
                &state,
                equipment,
                source,
                lot,
                Mass::from_milligrams(2),
                spent,
                condition(600_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("repair stale inventory validation failed: {error}"),
        };
        if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
            panic!("repair stale inventory mutation failed: {error}");
        }
        let condition_before = state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition());
        assert!(matches!(
            inventory_stale.commit(&mut state),
            Err(EquipmentRepairCommitError::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            })
        ));
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            condition_before
        );
        assert_eq!(
            state.inventory().get_lot(lot).map(|record| record.mass()),
            Some(Mass::from_milligrams(10))
        );

        let equipment_stale = match validate_equipment_repair(
            &registries,
            &state,
            bind(
                &state,
                equipment,
                source,
                lot,
                Mass::from_milligrams(2),
                spent,
                condition(600_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("repair stale equipment validation failed: {error}"),
        };
        let wear = match decide_equipment_wear(&state, equipment, 1_000) {
            Ok(plan) => plan,
            Err(error) => panic!("repair stale equipment wear failed: {error}"),
        };
        if let Err(error) = apply_equipment_condition_plan(&mut state, wear) {
            panic!("repair stale equipment wear commit failed: {error}");
        }
        let lot_mass_before = state.inventory().get_lot(lot).map(|record| record.mass());
        assert!(matches!(
            equipment_stale.commit(&mut state),
            Err(EquipmentRepairCommitError::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            })
        ));
        assert_eq!(
            state.inventory().get_lot(lot).map(|record| record.mass()),
            lot_mass_before
        );
    }

    #[test]
    fn repair_resolution_is_invalidated_by_equipment_change_before_validation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0009));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair stale-resolution equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair stale-resolution source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair stale-resolution spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(1));
        let expected_revision = state.equipment().revision();
        let resolution = bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(1),
            spent,
            condition(600_000),
        );
        assert_eq!(resolution.condition_before(), condition(500_000));
        let wear = match decide_equipment_wear(&state, equipment, 1_000) {
            Ok(plan) => plan,
            Err(error) => panic!("repair stale-resolution wear planning failed: {error}"),
        };
        if let Err(error) = apply_equipment_condition_plan(&mut state, wear) {
            panic!("repair stale-resolution wear commit failed: {error}");
        }
        let actual_revision = state.equipment().revision();
        let inventory_before = state.inventory().clone();

        assert_eq!(
            validate_equipment_repair(&registries, &state, resolution),
            Err(EquipmentRepairError::StaleEquipmentResolution {
                equipment,
                expected_revision,
                actual_revision,
            })
        );
        assert_eq!(state.inventory(), &inventory_before);
    }

    fn active_support(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
    ) -> crate::structural::StructuralElementId {
        let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        {
            Ok(bounds) => bounds,
            Err(error) => panic!("repair support bounds fixture failed: {error}"),
        };
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                bounds,
                Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            true,
        ) {
            Ok(element) => element,
            Err(error) => panic!("repair support fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        let activation = match validate_activate_structural_element(registries, state, element) {
            Ok(token) => token,
            Err(error) => panic!("repair support activation failed: {error}"),
        };
        if let Err(error) = activation.commit(state) {
            panic!("repair support activation commit failed: {error}");
        }
        element
    }

    #[test]
    fn repair_material_relocation_updates_supported_stockpile_loads_atomically() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0004));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair support equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair supported source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair supported spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
        let source_support = active_support(&registries, &mut state, 0);
        let spent_support = active_support(&registries, &mut state, 2);
        for (stockpile, support) in [(source, source_support), (spent, spent_support)] {
            let token = match validate_mount_stockpile(&registries, &state, stockpile, support) {
                Ok(token) => token,
                Err(error) => panic!("repair stockpile mount failed: {error}"),
            };
            if let Err(error) = token.commit(&mut state) {
                panic!("repair stockpile mount commit failed: {error}");
            }
        }
        let source_load_before = state
            .structures()
            .get_element(source_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter))
            .unwrap_or(Force::ZERO);
        assert!(source_load_before > Force::ZERO);
        assert_eq!(
            state
                .structures()
                .get_element(spent_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::ZERO)
        );

        let token = match validate_equipment_repair(
            &registries,
            &state,
            bind(
                &state,
                equipment,
                source,
                lot,
                Mass::from_milligrams(10),
                spent,
                condition(700_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("repair supported validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("repair supported commit failed: {error}");
        }

        assert_eq!(
            state
                .structures()
                .get_element(source_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::ZERO)
        );
        assert_eq!(
            state
                .structures()
                .get_element(spent_support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(source_load_before)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn repair_preserves_multiple_partial_lot_profiles_without_id_collision() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0005));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("multi-lot repair equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("multi-lot repair source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("multi-lot repair spent fixture failed: {error}"),
        };
        let first = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(5),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("multi-lot first fixture failed: {error}"),
        };
        let second = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(5),
            Temperature::from_millikelvin(310_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("multi-lot second fixture failed: {error}"),
        };
        let resolution = match bind_equipment_repair_for_test(
            &state,
            equipment,
            source,
            &[
                MaterialLotSelection::new(first, Mass::from_milligrams(2)),
                MaterialLotSelection::new(second, Mass::from_milligrams(2)),
            ],
            spent,
            condition(700_000),
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("multi-lot repair binding failed: {error:?}"),
        };
        let token = match validate_equipment_repair(&registries, &state, resolution) {
            Ok(token) => token,
            Err(error) => panic!("multi-lot repair validation failed: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("multi-lot repair commit failed: {error}");
        }

        assert_eq!(
            state.inventory().get_lot(first).map(|lot| lot.mass()),
            Some(Mass::from_milligrams(3))
        );
        assert_eq!(
            state.inventory().get_lot(second).map(|lot| lot.mass()),
            Some(Mass::from_milligrams(3))
        );
        let mut spent_lots: Vec<_> = state
            .inventory()
            .lots()
            .filter(|lot| lot.stockpile() == spent)
            .map(|lot| (lot.id(), lot.mass(), lot.temperature()))
            .collect();
        spent_lots.sort_by_key(|entry| entry.0);
        assert_eq!(spent_lots.len(), 2);
        assert_ne!(spent_lots[0].0, spent_lots[1].0);
        assert_eq!(spent_lots[0].1, Mass::from_milligrams(2));
        assert_eq!(spent_lots[1].1, Mass::from_milligrams(2));
        assert_eq!(
            spent_lots
                .iter()
                .map(|entry| entry.2)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                Temperature::from_millikelvin(300_000),
                Temperature::from_millikelvin(310_000),
            ])
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn repair_spent_capacity_failure_is_atomic() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0006));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair capacity equipment fixture failed: {error}"),
            };
        let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair capacity source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair capacity spent fixture failed: {error}"),
        };
        let lot = add_material(&registries, &mut state, source, Mass::from_milligrams(10));
        let resolution = bind(
            &state,
            equipment,
            source,
            lot,
            Mass::from_milligrams(2),
            spent,
            condition(700_000),
        );
        let before = state.clone();

        assert_eq!(
            validate_equipment_repair(&registries, &state, resolution),
            Err(EquipmentRepairError::Material(
                EquipmentRepairMaterialError::SpentCapacityExceeded {
                    stockpile: spent,
                    capacity: Mass::from_milligrams(1),
                    committed: Mass::ZERO,
                    requested: Mass::from_milligrams(2),
                }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    #[ignore = "long-horizon soak"]
    fn equipment_repair_soak_preserves_resource_conservation_and_replay() {
        let registries = registries();
        let mut first = AppState::new(WorldSeed::new(0x8120_0007));
        let equipment = match add_equipment(
            &registries,
            &mut first,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            Ok(equipment) => equipment,
            Err(error) => panic!("repair soak equipment fixture failed: {error}"),
        };
        let source = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(1)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair soak source fixture failed: {error}"),
        };
        let spent = match add_solid_stockpile_for_test(&mut first, Mass::from_milligrams(1)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair soak spent fixture failed: {error}"),
        };
        add_material(&registries, &mut first, source, Mass::from_milligrams(1));
        let initial_matter = match calculate_matter_accounting(&first) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("repair soak initial matter accounting failed: {error}"),
        };
        let initial_energy = explicit_energy(&registries, &first);
        let mut second = first.clone();

        for cycle in 0..500_u64 {
            for state in [&mut first, &mut second] {
                let wear = match decide_equipment_wear(state, equipment, 1_000) {
                    Ok(plan) => plan,
                    Err(error) => panic!("repair soak wear planning failed at {cycle}: {error}"),
                };
                if let Err(error) = apply_equipment_condition_plan(state, wear) {
                    panic!("repair soak wear commit failed at {cycle}: {error}");
                }
                let lot =
                    match state.inventory().lots().find(|lot| {
                        lot.stockpile() == source && lot.mass() >= Mass::from_milligrams(1)
                    }) {
                        Some(lot) => lot.id(),
                        None => panic!("repair soak source material missing at cycle {cycle}"),
                    };
                let resolution = bind(
                    state,
                    equipment,
                    source,
                    lot,
                    Mass::from_milligrams(1),
                    spent,
                    Condition::PRISTINE,
                );
                let repair = match validate_equipment_repair(&registries, state, resolution) {
                    Ok(token) => token,
                    Err(error) => panic!("repair soak validation failed at {cycle}: {error}"),
                };
                if let Err(error) = repair.commit(state) {
                    panic!("repair soak commit failed at {cycle}: {error}");
                }
                let return_material = match validate_transfer_bulk(
                    &registries,
                    state,
                    spent,
                    source,
                    CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                    Mass::from_milligrams(1),
                ) {
                    Ok(token) => token,
                    Err(error) => {
                        panic!("repair soak return validation failed at {cycle}: {error}")
                    }
                };
                if let Err(error) = return_material.commit(state) {
                    panic!("repair soak return commit failed at {cycle}: {error}");
                }
            }
            if cycle % 53 == 0 {
                assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
                assert_eq!(
                    calculate_matter_accounting(&first).map(|accounting| accounting.total()),
                    Ok(initial_matter)
                );
                assert_eq!(explicit_energy(&registries, &first), initial_energy);
            }
        }

        assert_eq!(first, second);
        assert_eq!(validate_loaded_state(&registries, &first), Ok(()));
        assert_eq!(
            calculate_matter_accounting(&first).map(|accounting| accounting.total()),
            Ok(initial_matter)
        );
        assert_eq!(explicit_energy(&registries, &first), initial_energy);
        assert_eq!(
            first
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            Some(Condition::PRISTINE)
        );
    }

    #[test]
    fn repair_commit_rechecks_late_production_occupancy_before_moving_material() {
        let registries = occupied_registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0008));
        let equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("repair occupancy equipment fixture failed: {error}"),
            };
        let process_source =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("repair occupancy process source failed: {error}"),
            };
        let process_destination =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("repair occupancy process destination failed: {error}"),
            };
        let maintenance_source =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("repair occupancy maintenance source failed: {error}"),
            };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("repair occupancy spent destination failed: {error}"),
        };
        let process_lot = add_material(
            &registries,
            &mut state,
            process_source,
            Mass::from_milligrams(10),
        );
        let maintenance_lot = add_material(
            &registries,
            &mut state,
            maintenance_source,
            Mass::from_milligrams(1),
        );
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_DEFINITION,
            Energy::from_nanojoules(1_000_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("repair occupancy energy fixture failed: {error}"),
        };
        let repair = match validate_equipment_repair(
            &registries,
            &state,
            bind(
                &state,
                equipment,
                maintenance_source,
                maintenance_lot,
                Mass::from_milligrams(1),
                spent,
                condition(600_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("repair occupancy validation failed: {error}"),
        };

        let selection = [MaterialLotSelection::new(
            process_lot,
            Mass::from_milligrams(10),
        )];
        let heating = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                HEATING_PROCESS,
                process_source,
                &selection,
                equipment,
                energy_store,
                Temperature::from_millikelvin(301_000),
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("repair occupancy heating resolution failed: {error}"),
        };
        let start = match validate_start_process(
            &registries,
            &state,
            heating.process_resolution(),
            process_source,
            process_destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("repair occupancy process validation failed: {error}"),
        };
        let job = match start.commit(&mut state) {
            Ok(job) => job,
            Err(error) => panic!("repair occupancy process commit failed: {error}"),
        };
        let job_record = match state.production().get_job(job) {
            Some(record) => record,
            None => panic!("repair occupancy process job missing after start"),
        };
        let expected_error = EquipmentRepairCommitError::EquipmentBusy {
            equipment,
            job,
            release: job_record.occupancy_release(),
        };
        let maintenance_mass_before = state
            .inventory()
            .get_lot(maintenance_lot)
            .map(|lot| lot.mass());
        let condition_before = state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition());

        assert_eq!(repair.commit(&mut state), Err(expected_error));
        assert_eq!(
            state
                .inventory()
                .get_lot(maintenance_lot)
                .map(|lot| lot.mass()),
            maintenance_mass_before
        );
        assert_eq!(
            state
                .equipment()
                .get_equipment(equipment)
                .map(|record| record.condition()),
            condition_before
        );
    }

    #[test]
    fn repair_counts_reserved_inbound_as_capacity_but_not_structural_weight() {
        let registries = occupied_registries();
        let mut state = AppState::new(WorldSeed::new(0x8120_0009));
        let process_equipment = match add_equipment(
            &registries,
            &mut state,
            TEST_DEFINITION,
            Condition::PRISTINE,
        ) {
            Ok(equipment) => equipment,
            Err(error) => panic!("reserved-weight process equipment fixture failed: {error}"),
        };
        let repair_equipment =
            match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
                Ok(equipment) => equipment,
                Err(error) => panic!("reserved-weight repair equipment fixture failed: {error}"),
            };
        let process_source =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("reserved-weight process source fixture failed: {error}"),
            };
        let maintenance_source =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("reserved-weight maintenance source fixture failed: {error}"),
            };
        let spent = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("reserved-weight spent fixture failed: {error}"),
        };
        let process_lot = add_material(
            &registries,
            &mut state,
            process_source,
            Mass::from_milligrams(5),
        );
        let maintenance_lot = add_material(
            &registries,
            &mut state,
            maintenance_source,
            Mass::from_milligrams(2),
        );
        let support = active_support(&registries, &mut state, 0);
        let mount = match validate_mount_stockpile(&registries, &state, spent, support) {
            Ok(token) => token,
            Err(error) => panic!("reserved-weight spent mount validation failed: {error}"),
        };
        if let Err(error) = mount.commit(&mut state) {
            panic!("reserved-weight spent mount commit failed: {error}");
        }
        let energy_store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ENERGY_DEFINITION,
            Energy::from_nanojoules(1_000_000_000),
        ) {
            Ok(store) => store,
            Err(error) => panic!("reserved-weight energy fixture failed: {error}"),
        };
        let process_selection = [MaterialLotSelection::new(
            process_lot,
            Mass::from_milligrams(5),
        )];
        let heating = match resolve_sensible_heating_process(
            &registries,
            &state,
            SensibleHeatingRequest::new(
                HEATING_PROCESS,
                process_source,
                &process_selection,
                process_equipment,
                energy_store,
                Temperature::from_millikelvin(301_000),
            ),
        ) {
            Ok(resolved) => resolved,
            Err(error) => panic!("reserved-weight heating resolution failed: {error}"),
        };
        let start = match validate_start_process(
            &registries,
            &state,
            heating.process_resolution(),
            process_source,
            spent,
        ) {
            Ok(token) => token,
            Err(error) => panic!("reserved-weight process validation failed: {error}"),
        };
        if let Err(error) = start.commit(&mut state) {
            panic!("reserved-weight process commit failed: {error}");
        }

        let spent_before = match state.inventory().get_stockpile(spent) {
            Some(record) => record,
            None => panic!("reserved-weight spent stockpile disappeared"),
        };
        assert_eq!(spent_before.reserved_inbound(), Mass::from_milligrams(5));
        assert_eq!(spent_before.stored_mass(), Mass::ZERO);
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(Force::ZERO)
        );

        let repair = match validate_equipment_repair(
            &registries,
            &state,
            bind(
                &state,
                repair_equipment,
                maintenance_source,
                maintenance_lot,
                Mass::from_milligrams(2),
                spent,
                condition(700_000),
            ),
        ) {
            Ok(token) => token,
            Err(error) => panic!("reserved-weight repair validation failed: {error}"),
        };
        if let Err(error) = repair.commit(&mut state) {
            panic!("reserved-weight repair commit failed: {error}");
        }

        let spent_after = match state.inventory().get_stockpile(spent) {
            Some(record) => record,
            None => panic!("reserved-weight spent stockpile disappeared after repair"),
        };
        assert_eq!(spent_after.reserved_inbound(), Mass::from_milligrams(5));
        assert_eq!(spent_after.stored_mass(), Mass::from_milligrams(2));
        let expected_weight = match calculate_aggregate_weight_force_ceiling(
            AggregateMass::from_mass(Mass::from_milligrams(2)),
            registries.core().gravity(),
        ) {
            Some(force) => force,
            None => panic!("reserved-weight expected load overflowed"),
        };
        assert_eq!(
            state
                .structures()
                .get_element(support)
                .map(|record| record.load(StructuralLoadKind::StoredMatter)),
            Some(expected_weight)
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}
