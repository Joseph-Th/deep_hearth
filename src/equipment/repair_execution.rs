//! Authored equipment maintenance resolution and conserved repair transaction boundary.
//!
//! Equipment definitions may author one replacement-material maintenance profile. Runtime resolution
//! selects that exact commodity from a requested source and binds the profile's restored condition and
//! spent-material form. The canonical transaction reforms the selected matter into that non-reusable
//! spent form while condition improves. Labor, tooling, access, and maintenance duration can extend
//! this physical resolver when those owners exist without reopening a free condition mutation path.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{
    ConsumptionSelection, ConsumptionSelectionError, MaterialReformCommitError,
    MaterialReformError, StockpileId, StockpileStorageError, StockpileStructuralLoadError,
    ValidatedMaterialReform, validate_consumption_selection,
    validate_material_reform_from_selection,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::mining::MiningJobId;
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
#[derive(Debug, PartialEq, Eq)]
pub struct EquipmentRepairResolution {
    equipment: EquipmentId,
    expected_equipment_revision: u64,
    condition_before: Condition,
    condition_after: Condition,
    material: ConsumptionSelection,
    spent: CommodityKey,
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
        spent: profile.spent(),
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
    pub const fn spent_commodity(&self) -> CommodityKey {
        self.spent
    }

    #[must_use]
    pub const fn material_mass(&self) -> Mass {
        self.material.total_consumed()
    }
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
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be repaired",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be repaired",
                equipment.value()
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
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before repair commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before repair commit",
                equipment.value()
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

/// Successful repair outcome after exact maintenance matter is reformed into its spent output.
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
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedEquipmentRepair {
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    material: ValidatedMaterialReform,
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
        if let Some(job) = state.mining().get_equipment_occupant(self.equipment) {
            return Err(EquipmentRepairCommitError::EquipmentBusyMining {
                equipment: self.equipment,
                job,
            });
        }
        if state
            .player_work()
            .get_manual_power_equipment_occupant(self.equipment)
            .is_some()
        {
            return Err(EquipmentRepairCommitError::EquipmentBusyManualPower {
                equipment: self.equipment,
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

fn map_material_error(error: MaterialReformError) -> EquipmentRepairMaterialError {
    match error {
        MaterialReformError::StaleSelection { expected, actual } => {
            EquipmentRepairMaterialError::StaleSelection { expected, actual }
        }
        MaterialReformError::UnknownSource { stockpile } => {
            EquipmentRepairMaterialError::UnknownSource { stockpile }
        }
        MaterialReformError::UnknownDestination { stockpile } => {
            EquipmentRepairMaterialError::UnknownSpentDestination { stockpile }
        }
        MaterialReformError::UnknownTargetMaterial { material } => {
            EquipmentRepairMaterialError::UnknownSpentMaterial { material }
        }
        MaterialReformError::UnknownTargetForm { form } => {
            EquipmentRepairMaterialError::UnknownSpentForm { form }
        }
        MaterialReformError::MaterialChanged { source, target } => {
            EquipmentRepairMaterialError::SpentMaterialChanged { source, target }
        }
        MaterialReformError::TargetUnchanged { commodity } => {
            EquipmentRepairMaterialError::SpentFormUnchanged { commodity }
        }
        MaterialReformError::DestinationStorage(error) => {
            EquipmentRepairMaterialError::SpentStorage(error)
        }
        MaterialReformError::DestinationMassOverflow { stockpile } => {
            EquipmentRepairMaterialError::SpentMassOverflow { stockpile }
        }
        MaterialReformError::DestinationCapacityExceeded {
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
        MaterialReformError::LotIdExhausted => EquipmentRepairMaterialError::LotIdExhausted,
        MaterialReformError::RevisionExhausted => {
            EquipmentRepairMaterialError::InventoryRevisionExhausted
        }
        MaterialReformError::StructuralLoad(error) => {
            EquipmentRepairMaterialError::StructuralLoad(error)
        }
    }
}

fn map_material_commit_error(error: MaterialReformCommitError) -> EquipmentRepairCommitError {
    match error {
        MaterialReformCommitError::StaleInventoryRevision { expected, actual } => {
            EquipmentRepairCommitError::StaleInventoryRevision { expected, actual }
        }
        MaterialReformCommitError::Structure(error) => EquipmentRepairCommitError::Structure(error),
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
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentRepairError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentRepairError::EquipmentBusyManualPower { equipment });
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
    let material = validate_material_reform_from_selection(
        registries,
        state,
        resolution.spent_destination,
        resolution.spent,
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
#[path = "repair_execution_tests.rs"]
mod tests;
