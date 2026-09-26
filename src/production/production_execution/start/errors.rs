//! Public validation errors for durable process-start admission.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::material::{FormId, MaterialId};
use crate::mining::MiningJobId;
use crate::spatial::VoxelCoord;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::super::ProductionSiteEndpoint;
use super::super::super::definitions::ProcessId;
use super::super::super::resolution::ProcessOutputStreamId;
use super::super::super::state::{ProductionJobId, ProductionOccupancyRelease};

mod display;
mod source;

/// Failure while validating the start of one durable material-processing job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessError {
    UnknownProcess {
        process: ProcessId,
    },
    ManualProcessRequiresPlayerWork {
        process: ProcessId,
    },
    ResolutionEquipmentTopologyMismatch {
        process: ProcessId,
    },
    ResolutionEnergyTopologyMismatch {
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
    SpatialEndpointMismatch {
        first: ProductionSiteEndpoint,
        first_position: VoxelCoord,
        second: ProductionSiteEndpoint,
        second_position: VoxelCoord,
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
    CompletionTickOverflow {
        current: SimulationTick,
        duration_ticks: u64,
    },
    JobIdExhausted,
    MaterialLotIdExhausted,
    InventoryRevisionExhausted,
    ProductionRevisionExhausted,
    EnergyRevisionExhausted,
    EquipmentRevisionExhausted,
    StructureRevisionExhausted,
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
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    StructuralLoad(StockpileStructuralLoadError),
}
