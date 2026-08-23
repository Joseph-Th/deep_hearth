//! Persistent-state validation for root runtime; child audits reconcile private owners without
//! exposing mutation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Force, Mass};
use crate::core::rng::RandomStateValidationError;
use crate::core::time::{SimulationTick, WorldSeed};
use crate::crafting::ManualCraftJobValidationError;
use crate::energy::{EnergyValidationError, validate_loaded_energy};
use crate::equipment::{
    EquipmentDefinitionId, EquipmentId, EquipmentValidationError, validate_loaded_equipment,
};
use crate::fluid::{
    FluidStoreId, FluidStructuralLoadError, FluidValidationError, validate_loaded_fluid,
};
use crate::geology::{
    GeologicalKnowledgeValidationError, GeologyValidationError,
    validate_loaded_geological_knowledge, validate_loaded_geology,
};
use crate::inventory::{
    InventoryValidationError, MaterialLotId, StockpileId, StockpileStorageError,
    validate_loaded_inventory,
};
use crate::labor::{PlayerWorkValidationError, validate_loaded_player_work};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialId, ParticleSizeStateError};
use crate::mining::{
    MiningJobValidationError, MiningValidationError, validate_loaded_mining,
    validate_loaded_mining_jobs,
};
use crate::ore_processing::{
    ComminutionJobValidationError, ConstituentSeparationJobValidationError,
    ScreeningJobValidationError,
};
use crate::production::{
    ProcessId, ProductionJobId, ProductionValidationError, validate_loaded_production,
};
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysisError, StructuralDamageEvent, StructuralElementId, StructureValidationError,
    validate_loaded_structure,
};
use crate::survival::{SurvivalValidationError, validate_loaded_survival};
use crate::thermal::ThermalJobValidationError;

use super::AppState;

mod inventory;
mod production;
mod structural;

use inventory::validate_inventory_references;
use production::validate_production_references;
use structural::validate_structural_integrations;

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
    UnknownJobSource {
        job: ProductionJobId,
        stockpile: StockpileId,
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
    JobAlreadyDue {
        job: ProductionJobId,
        current: SimulationTick,
        due: SimulationTick,
    },
    JobSuspendedInFuture {
        job: ProductionJobId,
        current: SimulationTick,
        suspended_at: SimulationTick,
    },
    ReservedMassOverflow {
        stockpile: StockpileId,
    },
    UnknownJobOutputCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    UnknownJobOutputCompositionMaterial {
        job: ProductionJobId,
        material: MaterialId,
    },
    JobOutputStorage {
        job: ProductionJobId,
        error: StockpileStorageError,
    },
    UnknownJobConsumedCommodity {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    UnknownJobConsumedCompositionMaterial {
        job: ProductionJobId,
        material: MaterialId,
    },
    InvalidJobConsumedParticleSizeState {
        job: ProductionJobId,
        error: ParticleSizeStateError,
    },
    JobOutputMassOverflow {
        job: ProductionJobId,
    },
    ReservedInboundMismatch {
        stockpile: StockpileId,
        reserved: Mass,
        expected: Mass,
    },
}

impl Display for StateValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Random(error) => write!(formatter, "invalid random state: {error}"),
            Self::RandomWorldSeedMismatch {
                world_seed,
                random_seed,
            } => write!(
                formatter,
                "world seed {} disagrees with random-state root seed {}",
                world_seed.value(),
                random_seed.value()
            ),
            Self::Energy(error) => write!(formatter, "invalid energy state: {error}"),
            Self::Fluid(error) => write!(formatter, "invalid fluid state: {error}"),
            Self::Equipment(error) => write!(formatter, "invalid equipment state: {error}"),
            Self::Structure(error) => write!(formatter, "invalid structural state: {error}"),
            Self::StructureAnalysis(error) => {
                write!(formatter, "structural state cannot be analyzed: {error}")
            }
            Self::UnresolvedStructuralDamage { event } => write!(
                formatter,
                "structural element {} has unresolved canonical damage",
                event.element().value()
            ),
            Self::Geology(error) => write!(formatter, "invalid geology state: {error}"),
            Self::GeologicalKnowledge(error) => {
                write!(formatter, "invalid geological knowledge state: {error}")
            }
            Self::Inventory(error) => write!(formatter, "invalid inventory state: {error}"),
            Self::Production(error) => write!(formatter, "invalid production state: {error}"),
            Self::Mining(error) => write!(formatter, "invalid mining state: {error}"),
            Self::MiningJob(error) => write!(formatter, "invalid mining job: {error}"),
            Self::PlayerWork(error) => write!(formatter, "invalid player-work state: {error}"),
            Self::Survival(error) => write!(formatter, "invalid survival state: {error}"),
            Self::UnknownStoredCommodity {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} references unknown material {} or form {}",
                stockpile.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::LotCreatedInFuture {
                lot,
                created_at,
                current,
            } => write!(
                formatter,
                "material lot {} was created at tick {} after current tick {}",
                lot.value(),
                created_at.value(),
                current.value()
            ),
            Self::LotProvenanceInFuture {
                lot,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "material lot {} contains provenance through tick {} after current tick {}",
                lot.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::UnknownLotCompositionMaterial { lot, material } => write!(
                formatter,
                "material lot {} composition references unknown material {}",
                lot.value(),
                material.value()
            ),
            Self::UnknownJobConsumedCommodity { job, commodity } => write!(
                formatter,
                "production job {} consumed unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnknownJobConsumedCompositionMaterial { job, material } => write!(
                formatter,
                "production job {} consumed-input composition references unknown material {}",
                job.value(),
                material.value()
            ),
            Self::InvalidJobConsumedParticleSizeState { job, error } => write!(
                formatter,
                "production job {} consumed invalid particle-size state: {error}",
                job.value()
            ),
            Self::UnknownJobProcess { job, process } => write!(
                formatter,
                "production job {} references unknown process {}",
                job.value(),
                process.value()
            ),
            Self::UnknownJobSource { job, stockpile } => write!(
                formatter,
                "production job {} references missing source stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::UnknownJobDestination { job, stockpile } => write!(
                formatter,
                "production job {} references missing destination stockpile {}",
                job.value(),
                stockpile.value()
            ),
            Self::UnknownJobEnergySource { job, store } => write!(
                formatter,
                "production job {} traces missing energy store {}",
                job.value(),
                store.value()
            ),
            Self::JobEnergyDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces energy definition {} but source store references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobEnergyCarrierMismatch {
                job,
                traced,
                authored,
            } => write!(
                formatter,
                "production job {} traces {traced:?} energy but source definition is {authored:?}",
                job.value()
            ),
            Self::UnknownJobEnergySink { job, store } => write!(
                formatter,
                "production job {} traces missing released-energy sink {}",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces released-energy definition {} but sink store references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobReleasedEnergyCarrierMismatch {
                job,
                traced,
                authored,
            } => write!(
                formatter,
                "production job {} traces released {traced:?} energy but sink definition is {authored:?}",
                job.value()
            ),
            Self::JobReleasedEnergySinkHasNoInputPower { job, store } => write!(
                formatter,
                "production job {} reserves energy sink {} whose definition accepts no input power",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyCapacityOverflow { job, store } => write!(
                formatter,
                "production job {} released-energy reservation overflows sink {} accounting",
                job.value(),
                store.value()
            ),
            Self::JobReleasedEnergyCapacityExceeded {
                job,
                store,
                stored,
                released,
                capacity,
            } => write!(
                formatter,
                "production job {} reserves {} nJ into sink {} containing {} nJ above capacity {} nJ",
                job.value(),
                released.nanojoules(),
                store.value(),
                stored.nanojoules(),
                capacity.nanojoules()
            ),
            Self::UnknownJobEquipment { job, equipment } => write!(
                formatter,
                "production job {} references missing equipment {}",
                job.value(),
                equipment.value()
            ),
            Self::JobEquipmentDefinitionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces equipment definition {} but provider record references {}",
                job.value(),
                traced.value(),
                stored.value()
            ),
            Self::JobEquipmentConditionMismatch {
                job,
                traced,
                stored,
            } => write!(
                formatter,
                "production job {} traces equipment condition {} ppm but provider record is {} ppm",
                job.value(),
                traced.parts_per_million(),
                stored.parts_per_million()
            ),
            Self::UnknownEquipmentSupport { equipment, element } => write!(
                formatter,
                "equipment {} references missing structural support element {}",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentSupportedByPlannedElement { equipment, element } => write!(
                formatter,
                "equipment {} is assigned to planned structural element {} before activation",
                equipment.value(),
                element.value()
            ),
            Self::MountedEquipmentMassOverflow { element } => write!(
                formatter,
                "mounted equipment mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::MountedEquipmentWeightOverflow { element } => write!(
                formatter,
                "mounted equipment weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::EquipmentStructuralLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN equipment load but mounted equipment requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::UnknownFluidSupport { store, element } => write!(
                formatter,
                "fluid store {} references missing structural support element {}",
                store.value(),
                element.value()
            ),
            Self::FluidSupportedByPlannedElement { store, element } => write!(
                formatter,
                "fluid store {} is assigned to planned structural element {} before activation",
                store.value(),
                element.value()
            ),
            Self::FluidStructuralLoad(error) => {
                write!(
                    formatter,
                    "invalid supported-fluid structural load: {error}"
                )
            }
            Self::UnknownStockpileSupport { stockpile, element } => write!(
                formatter,
                "stockpile {} references missing structural support element {}",
                stockpile.value(),
                element.value()
            ),
            Self::StockpileSupportedByPlannedElement { stockpile, element } => write!(
                formatter,
                "stockpile {} is assigned to planned structural element {} before activation",
                stockpile.value(),
                element.value()
            ),
            Self::StoredMatterMassOverflow { element } => write!(
                formatter,
                "stored matter mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::StoredMatterWeightOverflow { element } => write!(
                formatter,
                "stored matter weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::StoredMatterStructuralLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN stored-matter load but supported stockpiles require {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::ComminutionJob(error) => {
                write!(formatter, "invalid comminution production job: {error}")
            }
            Self::ConstituentSeparationJob(error) => {
                write!(
                    formatter,
                    "invalid constituent-separation production job: {error}"
                )
            }
            Self::ScreeningJob(error) => {
                write!(formatter, "invalid screening production job: {error}")
            }
            Self::ThermalJob(error) => write!(formatter, "invalid thermal production job: {error}"),
            Self::ManualCraftJob(error) => {
                write!(formatter, "invalid manual crafting production job: {error}")
            }
            Self::JobAlreadyDue { job, current, due } => write!(
                formatter,
                "production job {} is due at tick {} but current tick is {}",
                job.value(),
                due.value(),
                current.value()
            ),
            Self::JobSuspendedInFuture {
                job,
                current,
                suspended_at,
            } => write!(
                formatter,
                "production job {} claims suspension at tick {} after current tick {}",
                job.value(),
                suspended_at.value(),
                current.value()
            ),
            Self::ReservedMassOverflow { stockpile } => write!(
                formatter,
                "expected inbound reservations overflow stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::UnknownJobOutputCommodity { job, commodity } => write!(
                formatter,
                "production job {} promises unknown material {} or form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnknownJobOutputCompositionMaterial { job, material } => write!(
                formatter,
                "production job {} output composition references unknown material {}",
                job.value(),
                material.value()
            ),
            Self::JobOutputStorage { job, error } => write!(
                formatter,
                "production job {} reserved output is incompatible with its destination: {error}",
                job.value()
            ),
            Self::JobOutputMassOverflow { job } => write!(
                formatter,
                "production job {} output mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::ReservedInboundMismatch {
                stockpile,
                reserved,
                expected,
            } => write!(
                formatter,
                "stockpile {} reserves {} mg inbound but active jobs require {} mg",
                stockpile.value(),
                reserved.milligrams(),
                expected.milligrams()
            ),
        }
    }
}

impl Error for StateValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Random(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Fluid(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Structure(error) => Some(error),
            Self::StructureAnalysis(error) => Some(error),
            Self::Geology(error) => Some(error),
            Self::GeologicalKnowledge(error) => Some(error),
            Self::Inventory(error) => Some(error),
            Self::Production(error) => Some(error),
            Self::Mining(error) => Some(error),
            Self::MiningJob(error) => Some(error),
            Self::PlayerWork(error) => Some(error),
            Self::Survival(error) => Some(error),
            Self::ComminutionJob(error) => Some(error),
            Self::ConstituentSeparationJob(error) => Some(error),
            Self::ScreeningJob(error) => Some(error),
            Self::ThermalJob(error) => Some(error),
            Self::ManualCraftJob(error) => Some(error),
            Self::JobOutputStorage { job: _job, error } => Some(error),
            Self::InvalidJobConsumedParticleSizeState { job: _job, error } => Some(error),
            Self::FluidStructuralLoad(error) => Some(error),
            Self::RandomWorldSeedMismatch {
                world_seed: _world_seed,
                random_seed: _random_seed,
            } => None,
            Self::UnresolvedStructuralDamage { event: _event } => None,
            Self::UnknownStoredCommodity {
                stockpile: _stockpile,
                commodity: _commodity,
            } => None,
            Self::LotCreatedInFuture {
                lot: _lot,
                created_at: _created_at,
                current: _current,
            } => None,
            Self::LotProvenanceInFuture {
                lot: _lot,
                latest_created_at: _latest_created_at,
                current: _current,
            } => None,
            Self::UnknownLotCompositionMaterial {
                lot: _lot,
                material: _material,
            } => None,
            Self::UnknownJobProcess {
                job: _job,
                process: _process,
            } => None,
            Self::UnknownJobSource {
                job: _job,
                stockpile: _stockpile,
            }
            | Self::UnknownJobDestination {
                job: _job,
                stockpile: _stockpile,
            } => None,
            Self::UnknownJobEnergySource {
                job: _job,
                store: _store,
            }
            | Self::UnknownJobEnergySink {
                job: _job,
                store: _store,
            }
            | Self::JobReleasedEnergySinkHasNoInputPower {
                job: _job,
                store: _store,
            }
            | Self::JobReleasedEnergyCapacityOverflow {
                job: _job,
                store: _store,
            } => None,
            Self::JobEnergyDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            }
            | Self::JobReleasedEnergyDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::JobEnergyCarrierMismatch {
                job: _job,
                traced: _traced,
                authored: _authored,
            }
            | Self::JobReleasedEnergyCarrierMismatch {
                job: _job,
                traced: _traced,
                authored: _authored,
            } => None,
            Self::JobReleasedEnergyCapacityExceeded {
                job: _job,
                store: _store,
                stored: _stored,
                released: _released,
                capacity: _capacity,
            } => None,
            Self::UnknownJobEquipment {
                job: _job,
                equipment: _equipment,
            } => None,
            Self::JobEquipmentDefinitionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::JobEquipmentConditionMismatch {
                job: _job,
                traced: _traced,
                stored: _stored,
            } => None,
            Self::UnknownEquipmentSupport {
                equipment: _equipment,
                element: _element,
            }
            | Self::EquipmentSupportedByPlannedElement {
                equipment: _equipment,
                element: _element,
            } => None,
            Self::MountedEquipmentMassOverflow { element: _element }
            | Self::MountedEquipmentWeightOverflow { element: _element }
            | Self::StoredMatterMassOverflow { element: _element }
            | Self::StoredMatterWeightOverflow { element: _element } => None,
            Self::EquipmentStructuralLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            }
            | Self::StoredMatterStructuralLoadMismatch {
                element: _element,
                stored: _stored,
                expected: _expected,
            } => None,
            Self::UnknownStockpileSupport {
                stockpile: _stockpile,
                element: _element,
            }
            | Self::StockpileSupportedByPlannedElement {
                stockpile: _stockpile,
                element: _element,
            } => None,
            Self::UnknownFluidSupport {
                store: _store,
                element: _element,
            }
            | Self::FluidSupportedByPlannedElement {
                store: _store,
                element: _element,
            } => None,
            Self::JobAlreadyDue {
                job: _job,
                current: _current,
                due: _due,
            } => None,
            Self::JobSuspendedInFuture {
                job: _job,
                current: _current,
                suspended_at: _suspended_at,
            } => None,
            Self::ReservedMassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::UnknownJobOutputCommodity {
                job: _job,
                commodity: _commodity,
            }
            | Self::UnknownJobConsumedCommodity {
                job: _job,
                commodity: _commodity,
            } => None,
            Self::UnknownJobOutputCompositionMaterial {
                job: _job,
                material: _material,
            }
            | Self::UnknownJobConsumedCompositionMaterial {
                job: _job,
                material: _material,
            } => None,
            Self::JobOutputMassOverflow { job: _job } => None,
            Self::ReservedInboundMismatch {
                stockpile: _stockpile,
                reserved: _reserved,
                expected: _expected,
            } => None,
        }
    }
}

/// Validates decoded persistent state before it can re-enter the runtime.
pub fn validate_loaded_state(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    state
        .random
        .validate()
        .map_err(StateValidationError::Random)?;
    if state.random.root_seed() != state.world_seed {
        return Err(StateValidationError::RandomWorldSeedMismatch {
            world_seed: state.world_seed,
            random_seed: state.random.root_seed(),
        });
    }

    validate_loaded_energy(
        registries.energy(),
        registries.materials(),
        &state.systems.energy,
        state.tick(),
    )
    .map_err(StateValidationError::Energy)?;
    validate_loaded_fluid(registries.fluid(), &state.systems.fluid, state.tick())
        .map_err(StateValidationError::Fluid)?;
    validate_loaded_equipment(
        registries.equipment(),
        registries.materials(),
        &state.systems.equipment,
        state.tick(),
    )
    .map_err(StateValidationError::Equipment)?;
    validate_loaded_structure(
        registries.structural(),
        registries.materials(),
        &state.systems.structures,
        state.tick(),
        registries.core().gravity(),
    )
    .map_err(StateValidationError::Structure)?;
    validate_loaded_inventory(
        registries.materials(),
        &state.systems.inventory,
        state.tick(),
    )
    .map_err(StateValidationError::Inventory)?;

    validate_structural_integrations(registries, state)?;
    validate_loaded_geology(registries.materials(), &state.systems.geology, state.tick())
        .map_err(StateValidationError::Geology)?;
    validate_loaded_geological_knowledge(
        registries.materials(),
        &state.systems.geological_knowledge,
        state.tick(),
    )
    .map_err(StateValidationError::GeologicalKnowledge)?;
    validate_loaded_production(&state.systems.production, state.tick())
        .map_err(StateValidationError::Production)?;
    validate_loaded_mining(&state.systems.mining, state.tick())
        .map_err(StateValidationError::Mining)?;
    validate_loaded_survival(
        registries.survival(),
        registries.materials(),
        registries.fluid(),
        &state.systems.survival,
    )
    .map_err(StateValidationError::Survival)?;

    validate_inventory_references(registries, state)?;
    validate_production_references(registries, state)?;
    validate_loaded_mining_jobs(registries, state).map_err(StateValidationError::MiningJob)?;
    validate_loaded_player_work(registries, state, &state.systems.player_work)
        .map_err(StateValidationError::PlayerWork)?;

    Ok(())
}

/// Asserts every cheap runtime invariant in debug builds.
pub fn validate_invariants(registries: &Registries, state: &AppState) {
    debug_assert!(
        state.random.has_valid_core_stream(),
        "Runtime Invariant 11 (Serialization Completeness): core RNG stream must remain valid"
    );
    debug_assert!(
        state.systems.mining.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): mining job ID cursor must remain nonzero"
    );
    debug_assert!(
        state.systems.energy.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): energy store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.fluid.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): fluid store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.equipment.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): equipment ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.structures.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): structural ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.geology.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological deposit ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.geological_knowledge.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological observation ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.inventory.has_valid_id_cursors(),
        "Runtime Invariant 8 (No Lost Runtime State): inventory ID cursors must remain nonzero"
    );
    debug_assert!(
        state.systems.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production ID cursor must remain nonzero"
    );
    debug_assert!(
        state
            .systems
            .production
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no active production job may remain due"
    );
    debug_assert!(
        state
            .systems
            .mining
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no working mining job may remain due"
    );
    debug_assert!(
        state
            .systems
            .player_work
            .has_valid_inline_schedule(state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): active inline player work must have a current unfinished schedule"
    );
    debug_assert!(
        state
            .systems
            .survival
            .has_valid_player_bounds(registries.survival().physiology()),
        "Runtime Invariant 6 (Lifecycle Validity): player survival quantities must remain within authored bounds"
    );
}
