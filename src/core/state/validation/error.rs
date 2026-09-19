//! Root-state validation error taxonomy and diagnostics.

use crate::core::quantity::{Energy, Force, Mass};
use crate::core::rng::RandomStateValidationError;
use crate::core::time::{SimulationTick, WorldSeed};
use crate::crafting::ManualCraftJobValidationError;
use crate::energy::EnergyValidationError;
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentValidationError};
use crate::fluid::{FluidStoreId, FluidStructuralLoadError, FluidValidationError};
use crate::geology::{GeologicalKnowledgeValidationError, GeologyValidationError};
use crate::inventory::{
    InventoryValidationError, MaterialLotId, StockpileId, StockpileStorageError,
    StorageEnclosureValidationError,
};
use crate::labor::PlayerWorkValidationError;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialId, MaterialPhaseStateError, ParticleSizeStateError};
use crate::mining::{MiningJobValidationError, MiningValidationError};
use crate::ore_processing::{
    ComminutionJobValidationError, ConstituentSeparationJobValidationError,
    ScreeningJobValidationError,
};
use crate::production::{ProcessId, ProductionJobId, ProductionValidationError};
use crate::structural::{
    StructuralAnalysisError, StructuralDamageEvent, StructuralElementId, StructureValidationError,
};
use crate::survival::SurvivalValidationError;
use crate::thermal::ThermalJobValidationError;

/// Error returned when decoded runtime state violates a required persistent invariant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateValidationError {
    Random(RandomStateValidationError),
    RandomWorldSeedMismatch {
        world_seed: WorldSeed,
        random_seed: WorldSeed,
    },
    Energy(EnergyValidationError),
    Fluid(FluidValidationError),
    Equipment(EquipmentValidationError),
    Structure(StructureValidationError),
    StructureAnalysis(StructuralAnalysisError),
    UnresolvedStructuralDamage {
        event: StructuralDamageEvent,
    },
    Geology(GeologyValidationError),
    GeologicalKnowledge(GeologicalKnowledgeValidationError),
    Inventory(InventoryValidationError),
    StorageEnclosure(StorageEnclosureValidationError),
    Production(ProductionValidationError),
    Mining(MiningValidationError),
    MiningJob(MiningJobValidationError),
    PlayerWork(PlayerWorkValidationError),
    Survival(SurvivalValidationError),
    UnknownStoredCommodity {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    LotCreatedInFuture {
        lot: MaterialLotId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    LotProvenanceInFuture {
        lot: MaterialLotId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    UnknownLotCompositionMaterial {
        lot: MaterialLotId,
        material: MaterialId,
    },
    UnknownJobProcess {
        job: ProductionJobId,
        process: ProcessId,
    },
    MissingJobProcessTopology {
        job: ProductionJobId,
        process: ProcessId,
    },
    UnknownJobSource {
        job: ProductionJobId,
        stockpile: StockpileId,
    },
    JobEnergyTopologyMismatch {
        job: ProductionJobId,
        process: ProcessId,
    },
    JobEquipmentTopologyMismatch {
        job: ProductionJobId,
        process: ProcessId,
    },
    UnknownJobDestination {
        job: ProductionJobId,
        stockpile: StockpileId,
    },
    UnknownJobEnergySource {
        job: ProductionJobId,
        store: crate::energy::EnergyStoreId,
    },
    JobEnergyDefinitionMismatch {
        job: ProductionJobId,
        traced: crate::energy::EnergyStoreDefinitionId,
        stored: crate::energy::EnergyStoreDefinitionId,
    },
    JobEnergyCarrierMismatch {
        job: ProductionJobId,
        traced: crate::energy::EnergyCarrier,
        authored: crate::energy::EnergyCarrier,
    },
    UnknownJobEnergySink {
        job: ProductionJobId,
        store: crate::energy::EnergyStoreId,
    },
    JobReleasedEnergyDefinitionMismatch {
        job: ProductionJobId,
        traced: crate::energy::EnergyStoreDefinitionId,
        stored: crate::energy::EnergyStoreDefinitionId,
    },
    JobReleasedEnergyCarrierMismatch {
        job: ProductionJobId,
        traced: crate::energy::EnergyCarrier,
        authored: crate::energy::EnergyCarrier,
    },
    JobReleasedEnergySinkHasNoInputPower {
        job: ProductionJobId,
        store: crate::energy::EnergyStoreId,
    },
    JobReleasedEnergyCapacityOverflow {
        job: ProductionJobId,
        store: crate::energy::EnergyStoreId,
    },
    JobReleasedEnergyCapacityExceeded {
        job: ProductionJobId,
        store: crate::energy::EnergyStoreId,
        stored: Energy,
        released: Energy,
        capacity: Energy,
    },
    UnknownJobEquipment {
        job: ProductionJobId,
        equipment: EquipmentId,
    },
    JobEquipmentDefinitionMismatch {
        job: ProductionJobId,
        traced: EquipmentDefinitionId,
        stored: EquipmentDefinitionId,
    },
    JobEquipmentConditionMismatch {
        job: ProductionJobId,
        traced: Condition,
        stored: Condition,
    },
    JobEquipmentSupportRequirementMissing {
        job: ProductionJobId,
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    JobEquipmentSupportStateMismatch {
        job: ProductionJobId,
        equipment: EquipmentId,
        requires_active_support: bool,
        supported_by: Option<StructuralElementId>,
    },
    UnknownEquipmentSupport {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentSupportedByPlannedElement {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    MountedEquipmentMassOverflow {
        element: StructuralElementId,
    },
    MountedEquipmentWeightOverflow {
        element: StructuralElementId,
    },
    EquipmentStructuralLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    UnknownStockpileSupport {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    StockpileSupportedByPlannedElement {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    StoredMatterMassOverflow {
        element: StructuralElementId,
    },
    StoredMatterWeightOverflow {
        element: StructuralElementId,
    },
    StoredMatterStructuralLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    UnknownFluidSupport {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    FluidSupportedByPlannedElement {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    FluidStructuralLoad(FluidStructuralLoadError),
    ComminutionJob(ComminutionJobValidationError),
    ConstituentSeparationJob(ConstituentSeparationJobValidationError),
    ScreeningJob(ScreeningJobValidationError),
    ThermalJob(ThermalJobValidationError),
    ManualCraftJob(ManualCraftJobValidationError),
    NonManualJobSuspendedForPlayerLabor {
        job: ProductionJobId,
        process: ProcessId,
    },
    ReservedMassOverflow {
        stockpile: StockpileId,
    },
    UnknownJobOutputCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    JobOutputStorage {
        job: ProductionJobId,
        error: StockpileStorageError,
    },
    UnknownJobConsumedCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    InvalidJobConsumedParticleSizeState {
        job: ProductionJobId,
        error: ParticleSizeStateError,
    },
    InvalidJobConsumedPhaseState {
        job: ProductionJobId,
        error: MaterialPhaseStateError,
    },
    JobOutputMassOverflow {
        job: ProductionJobId,
    },
    ProductionInventoryRevisionCapacityExhausted {
        revision: u64,
        completion_buckets: u64,
    },
    ProductionEquipmentRevisionCapacityExhausted {
        revision: u64,
        completion_buckets: u64,
    },
    ProductionEnergyRevisionCapacityExhausted {
        revision: u64,
        completion_buckets: u64,
    },
    ProductionStructureRevisionCapacityExhausted {
        revision: u64,
        completion_buckets: u64,
    },
    FutureMaterialLotIdCapacityExhausted {
        next_lot_id: u64,
        required: u64,
    },
    FutureMaterialLotIdDemandOverflow,
    FutureInventoryRevisionCapacityExhausted {
        revision: u64,
        required: u64,
    },
    FutureInventoryRevisionDemandOverflow,
    FutureEnergyRevisionCapacityExhausted {
        revision: u64,
        required: u64,
    },
    FutureEnergyRevisionDemandOverflow,
    FutureEquipmentRevisionCapacityExhausted {
        revision: u64,
        required: u64,
    },
    FutureEquipmentRevisionDemandOverflow,
    FutureMiningRevisionCapacityExhausted {
        revision: u64,
        required: u64,
    },
    FutureMiningRevisionDemandOverflow,
    FutureStructureRevisionCapacityExhausted {
        revision: u64,
        required: u64,
    },
    FutureStructureRevisionDemandOverflow,
    ReservedInboundMismatch {
        stockpile: StockpileId,
        reserved: Mass,
        expected: Mass,
    },
}

mod display;
mod source;
