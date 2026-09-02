//! Public validation errors for durable process-start admission.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::material::{FormId, MaterialId};
use crate::mining::MiningJobId;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::super::definitions::ProcessId;
use super::super::super::resolution::ProcessOutputStreamId;
use super::super::super::state::{ProductionJobId, ProductionOccupancyRelease};

/// Failure while validating the start of one durable material-processing job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessError {
    UnknownProcess {
        process: ProcessId,
    },
    ManualProcessRequiresPlayerWork {
        process: ProcessId,
    },
    UnknownOutputMaterial {
        material: MaterialId,
    },
    UnknownOutputForm {
        form: FormId,
    },
    UnknownOutputCompositionMaterial {
        material: MaterialId,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    OutputDestinationBusyStorageDismantling {
        stockpile: StockpileId,
    },
    OutputRouteCountMismatch {
        streams: usize,
        routes: usize,
    },
    DuplicateOutputRoute {
        stream: ProcessOutputStreamId,
    },
    UnknownOutputRoute {
        stream: ProcessOutputStreamId,
    },
    MissingOutputRoute {
        stream: ProcessOutputStreamId,
    },
    DestinationStorage(StockpileStorageError),
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed_after_consumption: Mass,
        requested_inbound: Mass,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    InputStorageAgeOverflow {
        stockpile: StockpileId,
    },
    CompletionTickOverflow {
        current: SimulationTick,
        duration_ticks: u64,
    },
    JobIdExhausted,
    InventoryRevisionExhausted,
    ProductionRevisionExhausted,
    EnergyRevisionExhausted,
    ResolutionSourceMismatch {
        bound: StockpileId,
        requested: StockpileId,
    },
    StaleResolvedInputs {
        expected_inventory_revision: u64,
        actual_inventory_revision: u64,
    },
    StaleResolvedEnergy {
        expected_energy_revision: u64,
        actual_energy_revision: u64,
    },
    StaleResolvedEquipment {
        expected_equipment_revision: u64,
        actual_equipment_revision: u64,
    },
    StaleResolvedStructure {
        expected_structure_revision: u64,
        actual_structure_revision: u64,
    },
    ResolvedEnergyStoreMissing,
    ResolvedEnergyInsufficient,
    ResolvedEnergySinkMissing,
    ResolvedEnergySinkCapacity,
    EnergyStoreBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EnergyStoreBusyManualPower {
        store: EnergyStoreId,
    },
    ResolvedEquipmentMissing {
        equipment: EquipmentId,
    },
    ResolvedEquipmentDefinitionChanged {
        equipment: EquipmentId,
    },
    ResolvedEquipmentConditionChanged {
        equipment: EquipmentId,
    },
    ResolvedEquipmentSupportChanged {
        equipment: EquipmentId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    ResolvedEquipmentSupportMissing {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    ResolvedEquipmentSupportNotActive {
        equipment: EquipmentId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
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
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StartProcessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => {
                write!(formatter, "unknown process id {}", process.value())
            }
            Self::ManualProcessRequiresPlayerWork { process } => write!(
                formatter,
                "manual process {} must start through the player-work boundary",
                process.value()
            ),
            Self::UnknownOutputMaterial { material } => write!(
                formatter,
                "resolved output references unknown material {}",
                material.value()
            ),
            Self::UnknownOutputForm { form } => write!(
                formatter,
                "resolved output references unknown form {}",
                form.value()
            ),
            Self::UnknownOutputCompositionMaterial { material } => write!(
                formatter,
                "resolved output composition references unknown material {}",
                material.value()
            ),
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::OutputDestinationBusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} is being dismantled and cannot accept a new in-flight production output",
                stockpile.value()
            ),
            Self::OutputRouteCountMismatch { streams, routes } => write!(
                formatter,
                "resolved process has {streams} output streams but start supplied {routes} routes"
            ),
            Self::DuplicateOutputRoute { stream } => write!(
                formatter,
                "process start supplies output stream {} more than once",
                stream.value()
            ),
            Self::UnknownOutputRoute { stream } => write!(
                formatter,
                "process start routes unknown output stream {}",
                stream.value()
            ),
            Self::MissingOutputRoute { stream } => write!(
                formatter,
                "process start does not route output stream {}",
                stream.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "process destination rejects resolved output: {error}"
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed_after_consumption,
                requested_inbound,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg cannot reserve {} mg with {} mg already committed",
                stockpile.value(),
                capacity.milligrams(),
                requested_inbound.milligrams(),
                committed_after_consumption.milligrams()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "mass accounting overflow while scheduling against stockpile {}",
                stockpile.value()
            ),
            Self::InputStorageAgeOverflow { stockpile } => write!(
                formatter,
                "process input storage exposure from stockpile {} exceeds authoritative range",
                stockpile.value()
            ),
            Self::CompletionTickOverflow {
                current,
                duration_ticks,
            } => write!(
                formatter,
                "process duration {duration_ticks} cannot be added to simulation tick {}",
                current.value()
            ),
            Self::JobIdExhausted => {
                formatter.write_str("production job identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::ProductionRevisionExhausted => {
                formatter.write_str("production revision space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
            Self::ResolutionSourceMismatch { bound, requested } => write!(
                formatter,
                "resolved process is bound to source stockpile {} but start requested stockpile {}",
                bound.value(),
                requested.value()
            ),
            Self::StaleResolvedInputs {
                expected_inventory_revision,
                actual_inventory_revision,
            } => write!(
                formatter,
                "resolved process inputs expected inventory revision {expected_inventory_revision} but current revision is {actual_inventory_revision}"
            ),
            Self::StaleResolvedEnergy {
                expected_energy_revision,
                actual_energy_revision,
            } => write!(
                formatter,
                "resolved process energy expected revision {expected_energy_revision} but current energy revision is {actual_energy_revision}"
            ),
            Self::StaleResolvedEquipment {
                expected_equipment_revision,
                actual_equipment_revision,
            } => write!(
                formatter,
                "resolved process equipment expected revision {expected_equipment_revision} but current equipment revision is {actual_equipment_revision}"
            ),
            Self::StaleResolvedStructure {
                expected_structure_revision,
                actual_structure_revision,
            } => write!(
                formatter,
                "resolved process equipment support expected structural revision {expected_structure_revision} but current structural revision is {actual_structure_revision}"
            ),
            Self::ResolvedEnergyStoreMissing => {
                formatter.write_str("resolved process energy store no longer exists")
            }
            Self::ResolvedEnergyInsufficient => {
                formatter.write_str("resolved process energy amount is no longer available")
            }
            Self::ResolvedEnergySinkMissing => {
                formatter.write_str("resolved process energy sink no longer exists")
            }
            Self::ResolvedEnergySinkCapacity => {
                formatter.write_str("resolved process energy sink no longer has required capacity")
            }
            Self::EnergyStoreBusy {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is occupied by production job {} {release}",
                store.value(),
                job.value()
            ),
            Self::EnergyStoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is occupied by direct player-powered generation",
                store.value()
            ),
            Self::ResolvedEquipmentMissing { equipment } => write!(
                formatter,
                "resolved process equipment {} no longer exists",
                equipment.value()
            ),
            Self::ResolvedEquipmentDefinitionChanged { equipment } => write!(
                formatter,
                "resolved process equipment {} changed definition after resolution",
                equipment.value()
            ),
            Self::ResolvedEquipmentConditionChanged { equipment } => write!(
                formatter,
                "resolved process equipment {} changed condition after resolution",
                equipment.value()
            ),
            Self::ResolvedEquipmentSupportChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "resolved process equipment {} support changed from {expected:?} to {actual:?} after resolution",
                equipment.value()
            ),
            Self::ResolvedEquipmentSupportMissing { equipment, element } => write!(
                formatter,
                "resolved process equipment {} references missing structural support {}",
                equipment.value(),
                element.value()
            ),
            Self::ResolvedEquipmentSupportNotActive {
                equipment,
                element,
                lifecycle,
            } => write!(
                formatter,
                "resolved process equipment {} structural support {} is {lifecycle:?} and cannot authorize process start",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation",
                equipment.value()
            ),
            Self::StructuralLoad(error) => write!(
                formatter,
                "process start cannot update stored-matter load: {error}"
            ),
        }
    }
}

impl Error for StartProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownProcess { .. }
            | Self::ManualProcessRequiresPlayerWork { .. }
            | Self::UnknownOutputMaterial { .. }
            | Self::UnknownOutputForm { .. }
            | Self::UnknownOutputCompositionMaterial { .. }
            | Self::UnknownStockpile { .. }
            | Self::OutputDestinationBusyStorageDismantling { .. }
            | Self::OutputRouteCountMismatch { .. }
            | Self::DuplicateOutputRoute { .. }
            | Self::UnknownOutputRoute { .. }
            | Self::MissingOutputRoute { .. }
            | Self::CapacityExceeded { .. }
            | Self::MassOverflow { .. }
            | Self::InputStorageAgeOverflow { .. }
            | Self::CompletionTickOverflow { .. }
            | Self::JobIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::ProductionRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::ResolutionSourceMismatch { .. }
            | Self::StaleResolvedInputs { .. }
            | Self::StaleResolvedEnergy { .. }
            | Self::StaleResolvedEquipment { .. }
            | Self::StaleResolvedStructure { .. }
            | Self::ResolvedEnergyStoreMissing
            | Self::ResolvedEnergyInsufficient
            | Self::ResolvedEnergySinkMissing
            | Self::ResolvedEnergySinkCapacity
            | Self::EnergyStoreBusy { .. }
            | Self::EnergyStoreBusyManualPower { .. }
            | Self::ResolvedEquipmentMissing { .. }
            | Self::ResolvedEquipmentDefinitionChanged { .. }
            | Self::ResolvedEquipmentConditionChanged { .. }
            | Self::ResolvedEquipmentSupportChanged { .. }
            | Self::ResolvedEquipmentSupportMissing { .. }
            | Self::ResolvedEquipmentSupportNotActive { .. }
            | Self::EquipmentBusy { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}
