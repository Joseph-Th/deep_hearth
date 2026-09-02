//! Maintenance transaction error taxonomy kept separate from validation and commit mechanics.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::StructuralCommitError;

use super::super::definitions::EquipmentDefinitionId;
use super::super::state::EquipmentId;

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
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
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
    CompletionTickOverflow {
        current: SimulationTick,
        duration: TickSpan,
    },
    PlayerWork(PlayerWorkStartError),
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
    EmbodiedComponentMismatch {
        equipment: EquipmentId,
        component: CommodityKey,
        embodied: Mass,
        required: Mass,
    },
    InvalidEmbodiedComponent {
        equipment: EquipmentId,
    },
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
            Self::EmbodiedComponentMismatch {
                equipment,
                component,
                embodied,
                required,
            } => write!(
                formatter,
                "equipment {} contains {} mg of service component {} but {} mg must be exchanged",
                equipment.value(),
                embodied.milligrams(),
                component.value(),
                required.milligrams()
            ),
            Self::InvalidEmbodiedComponent { equipment } => write!(
                formatter,
                "equipment {} contains an invalid embodied component trace for maintenance exchange",
                equipment.value()
            ),
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
            Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EmbodiedComponentMismatch { .. }
            | Self::InvalidEmbodiedComponent { .. } => None,
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
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by geological sampling until tick {} and cannot be serviced",
                equipment.value(),
                completes_at.value()
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
            Self::CompletionTickOverflow { current, duration } => write!(
                formatter,
                "equipment maintenance starting at tick {} cannot schedule {} active ticks",
                current.value(),
                duration.value()
            ),
            Self::PlayerWork(error) => write!(
                formatter,
                "equipment maintenance labor cannot start: {error}"
            ),
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
            Self::PlayerWork(error) => Some(error),
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
            Self::EquipmentBusyProspecting {
                equipment: _equipment,
                completes_at: _completes_at,
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
            Self::CompletionTickOverflow { .. } => None,
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
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    Structure(StructuralCommitError),
    PlayerWork(PlayerWorkCommitError),
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
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by geological sampling until tick {} before maintenance commit",
                equipment.value(),
                completes_at.value()
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated equipment maintenance expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "equipment maintenance material structural commit failed: {error}"
            ),
            Self::PlayerWork(error) => write!(
                formatter,
                "equipment maintenance labor commit failed: {error}"
            ),
        }
    }
}

impl Error for EquipmentMaintenanceCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
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
            Self::EquipmentBusyProspecting {
                equipment: _equipment,
                completes_at: _completes_at,
            } => None,
        }
    }
}
