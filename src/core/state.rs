//! Root serializable runtime state and cheap invariant enforcement for the simulation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{AggregateMass, Energy, Force, Mass};
use crate::energy::{EnergyState, EnergyValidationError, validate_loaded_energy};
use crate::equipment::{
    EquipmentDefinitionId, EquipmentId, EquipmentState, EquipmentValidationError,
    validate_loaded_equipment,
};
use crate::fluid::{FluidState, FluidValidationError, validate_loaded_fluid};
use crate::geology::{
    GeologicalKnowledgeState, GeologicalKnowledgeValidationError, GeologyState,
    GeologyValidationError, validate_loaded_geological_knowledge, validate_loaded_geology,
};
use crate::inventory::{
    InventoryState, InventoryValidationError, MaterialLotId, StockpileId, StockpileStorageError,
    validate_loaded_inventory, validate_stockpile_storage,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialId};
use crate::ore_processing::{ComminutionJobValidationError, validate_loaded_comminution_job};
use crate::production::{
    ProcessId, ProductionJobId, ProductionState, ProductionValidationError, sum_lot_spec_mass,
    validate_loaded_production,
};
use crate::registry::Registries;
use crate::structural::{
    StructuralAnalysisError, StructuralDamageEvent, StructuralElementId, StructuralLifecycle,
    StructuralLoadKind, StructureState, StructureValidationError, analyze_structure,
    calculate_aggregate_weight_force_ceiling, validate_loaded_structure,
};
use crate::thermal::{ThermalJobValidationError, validate_loaded_thermal_job};

use super::rng::{RandomState, RandomStateValidationError, RngStreamId};
use super::time::{SimulationTick, WorldSeed};

/// Mutable runtime state that must survive execution and restart boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    world_seed: WorldSeed,
    clock: ClockState,
    random: RandomState,
    energy: EnergyState,
    fluid: FluidState,
    equipment: EquipmentState,
    structures: StructureState,
    geology: GeologyState,
    geological_knowledge: GeologicalKnowledgeState,
    inventory: InventoryState,
    production: ProductionState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ClockState {
    tick: SimulationTick,
}

impl AppState {
    /// Builds a fresh deterministic runtime state for one world.
    #[must_use]
    pub fn new(world_seed: WorldSeed) -> Self {
        Self {
            world_seed,
            clock: ClockState {
                tick: SimulationTick::ZERO,
            },
            random: RandomState::new(world_seed),
            energy: EnergyState::new(),
            fluid: FluidState::new(),
            equipment: EquipmentState::new(),
            structures: StructureState::new(),
            geology: GeologyState::new(),
            geological_knowledge: GeologicalKnowledgeState::new(),
            inventory: InventoryState::new(),
            production: ProductionState::new(),
        }
    }

    /// Returns the immutable world seed.
    #[must_use]
    pub const fn world_seed(&self) -> WorldSeed {
        self.world_seed
    }

    /// Returns the current authoritative simulation tick.
    #[must_use]
    pub const fn tick(&self) -> SimulationTick {
        self.clock.tick
    }

    /// Returns the persisted random algorithm identity without exposing mutable PRNG state.
    #[must_use]
    pub fn rng_algorithm(&self) -> super::rng::RngAlgorithm {
        match self.random.stream_algorithm(RngStreamId::CORE) {
            Some(algorithm) => algorithm,
            None => panic!("runtime invariant broken: core random stream is missing"),
        }
    }

    /// Returns read-only authoritative finite-energy state.
    #[must_use]
    pub const fn energy(&self) -> &EnergyState {
        &self.energy
    }

    pub(crate) const fn energy_state(&self) -> &EnergyState {
        &self.energy
    }

    pub(crate) fn energy_state_mut(&mut self) -> &mut EnergyState {
        &mut self.energy
    }

    /// Returns read-only authoritative finite fluid state.
    #[must_use]
    pub const fn fluid(&self) -> &FluidState {
        &self.fluid
    }

    pub(crate) const fn fluid_state(&self) -> &FluidState {
        &self.fluid
    }

    pub(crate) fn fluid_state_mut(&mut self) -> &mut FluidState {
        &mut self.fluid
    }

    /// Returns read-only authoritative equipment state.
    #[must_use]
    pub const fn equipment(&self) -> &EquipmentState {
        &self.equipment
    }

    pub(crate) const fn equipment_state(&self) -> &EquipmentState {
        &self.equipment
    }

    pub(crate) fn equipment_state_mut(&mut self) -> &mut EquipmentState {
        &mut self.equipment
    }

    /// Returns read-only authoritative structural state.
    #[must_use]
    pub const fn structures(&self) -> &StructureState {
        &self.structures
    }

    pub(crate) const fn structure_state(&self) -> &StructureState {
        &self.structures
    }

    pub(crate) fn structure_state_mut(&mut self) -> &mut StructureState {
        &mut self.structures
    }

    /// Returns authoritative geological truth to owning core systems only.
    ///
    /// Player-facing adapters must use `geological_knowledge()` rather than enumerating hidden
    /// deposit records directly.
    #[must_use]
    pub(crate) const fn geology(&self) -> &GeologyState {
        &self.geology
    }

    pub(crate) fn geology_state_mut(&mut self) -> &mut GeologyState {
        &mut self.geology
    }

    /// Returns acquired geological evidence without exposing it as authoritative world truth.
    #[must_use]
    pub const fn geological_knowledge(&self) -> &GeologicalKnowledgeState {
        &self.geological_knowledge
    }

    pub(crate) fn geological_knowledge_state_mut(&mut self) -> &mut GeologicalKnowledgeState {
        &mut self.geological_knowledge
    }

    /// Returns read-only authoritative stockpile state.
    #[must_use]
    pub const fn inventory(&self) -> &InventoryState {
        &self.inventory
    }

    pub(crate) const fn inventory_state(&self) -> &InventoryState {
        &self.inventory
    }

    pub(crate) fn inventory_state_mut(&mut self) -> &mut InventoryState {
        &mut self.inventory
    }

    /// Returns read-only authoritative production scheduling state.
    #[must_use]
    pub const fn production(&self) -> &ProductionState {
        &self.production
    }

    pub(crate) const fn production_state(&self) -> &ProductionState {
        &self.production
    }

    pub(crate) fn production_state_mut(&mut self) -> &mut ProductionState {
        &mut self.production
    }

    pub(crate) fn random_state_mut(&mut self) -> &mut RandomState {
        &mut self.random
    }
}

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
    EnergyStoreDoubleBooked {
        store: crate::energy::EnergyStoreId,
        first: ProductionJobId,
        second: ProductionJobId,
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
    EquipmentDoubleBooked {
        equipment: EquipmentId,
        first: ProductionJobId,
        second: ProductionJobId,
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
    ComminutionJob(ComminutionJobValidationError),
    ThermalJob(ThermalJobValidationError),
    JobAlreadyDue {
        job: ProductionJobId,
        current: SimulationTick,
        due: SimulationTick,
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
            Self::EnergyStoreDoubleBooked {
                store,
                first,
                second,
            } => write!(
                formatter,
                "energy store {} is simultaneously reserved by production jobs {} and {}",
                store.value(),
                first.value(),
                second.value()
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
            Self::EquipmentDoubleBooked {
                equipment,
                first,
                second,
            } => write!(
                formatter,
                "equipment {} is simultaneously assigned to production jobs {} and {}",
                equipment.value(),
                first.value(),
                second.value()
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
            Self::ThermalJob(error) => write!(formatter, "invalid thermal production job: {error}"),
            Self::JobAlreadyDue { job, current, due } => write!(
                formatter,
                "production job {} is due at tick {} but current tick is {}",
                job.value(),
                due.value(),
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
            Self::ComminutionJob(error) => Some(error),
            Self::ThermalJob(error) => Some(error),
            Self::JobOutputStorage { error, .. } => Some(error),
            Self::RandomWorldSeedMismatch { .. }
            | Self::UnresolvedStructuralDamage { .. }
            | Self::UnknownStoredCommodity { .. }
            | Self::LotCreatedInFuture { .. }
            | Self::LotProvenanceInFuture { .. }
            | Self::UnknownLotCompositionMaterial { .. }
            | Self::UnknownJobProcess { .. }
            | Self::UnknownJobSource { .. }
            | Self::UnknownJobDestination { .. }
            | Self::UnknownJobEnergySource { .. }
            | Self::JobEnergyDefinitionMismatch { .. }
            | Self::JobEnergyCarrierMismatch { .. }
            | Self::UnknownJobEnergySink { .. }
            | Self::JobReleasedEnergyDefinitionMismatch { .. }
            | Self::JobReleasedEnergyCarrierMismatch { .. }
            | Self::JobReleasedEnergySinkHasNoInputPower { .. }
            | Self::JobReleasedEnergyCapacityOverflow { .. }
            | Self::JobReleasedEnergyCapacityExceeded { .. }
            | Self::EnergyStoreDoubleBooked { .. }
            | Self::UnknownJobEquipment { .. }
            | Self::JobEquipmentDefinitionMismatch { .. }
            | Self::JobEquipmentConditionMismatch { .. }
            | Self::EquipmentDoubleBooked { .. }
            | Self::UnknownEquipmentSupport { .. }
            | Self::EquipmentSupportedByPlannedElement { .. }
            | Self::MountedEquipmentMassOverflow { .. }
            | Self::MountedEquipmentWeightOverflow { .. }
            | Self::EquipmentStructuralLoadMismatch { .. }
            | Self::UnknownStockpileSupport { .. }
            | Self::StockpileSupportedByPlannedElement { .. }
            | Self::StoredMatterMassOverflow { .. }
            | Self::StoredMatterWeightOverflow { .. }
            | Self::StoredMatterStructuralLoadMismatch { .. }
            | Self::JobAlreadyDue { .. }
            | Self::ReservedMassOverflow { .. }
            | Self::UnknownJobOutputCommodity { .. }
            | Self::UnknownJobOutputCompositionMaterial { .. }
            | Self::UnknownJobConsumedCommodity { .. }
            | Self::UnknownJobConsumedCompositionMaterial { .. }
            | Self::JobOutputMassOverflow { .. }
            | Self::ReservedInboundMismatch { .. } => None,
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

    validate_loaded_energy(registries.energy(), &state.energy, state.tick())
        .map_err(StateValidationError::Energy)?;
    validate_loaded_fluid(registries.fluid(), &state.fluid, state.tick())
        .map_err(StateValidationError::Fluid)?;
    validate_loaded_equipment(registries.equipment(), &state.equipment, state.tick())
        .map_err(StateValidationError::Equipment)?;
    validate_loaded_structure(
        registries.structural(),
        registries.materials(),
        &state.structures,
        state.tick(),
        registries.core().gravity(),
    )
    .map_err(StateValidationError::Structure)?;
    validate_loaded_inventory(registries.materials(), &state.inventory)
        .map_err(StateValidationError::Inventory)?;

    let mut mounted_mass_by_element = BTreeMap::<StructuralElementId, AggregateMass>::new();
    for equipment in state.equipment.equipment() {
        let Some(element) = equipment.supported_by() else {
            continue;
        };
        let Some(structural) = state.structures.get_element(element) else {
            return Err(StateValidationError::UnknownEquipmentSupport {
                equipment: equipment.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::EquipmentSupportedByPlannedElement {
                equipment: equipment.id(),
                element,
            });
        }
        let Some(definition) = registries.equipment().get_equipment(equipment.definition()) else {
            return Err(StateValidationError::Equipment(
                EquipmentValidationError::UnknownDefinition {
                    equipment: equipment.id(),
                    definition: equipment.definition(),
                },
            ));
        };
        let current = mounted_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let next = current
            .checked_add(AggregateMass::from_mass(definition.mass()))
            .ok_or(StateValidationError::MountedEquipmentMassOverflow { element })?;
        mounted_mass_by_element.insert(element, next);
    }
    for structural in state.structures.elements() {
        let element = structural.id();
        let mass = mounted_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
            .ok_or(StateValidationError::MountedEquipmentWeightOverflow { element })?;
        let stored = structural.load(StructuralLoadKind::Equipment);
        if stored != expected {
            return Err(StateValidationError::EquipmentStructuralLoadMismatch {
                element,
                stored,
                expected,
            });
        }
    }

    let mut stored_mass_by_element = BTreeMap::<StructuralElementId, AggregateMass>::new();
    for stockpile in state.inventory.stockpiles() {
        let Some(element) = stockpile.supported_by() else {
            continue;
        };
        let Some(structural) = state.structures.get_element(element) else {
            return Err(StateValidationError::UnknownStockpileSupport {
                stockpile: stockpile.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::StockpileSupportedByPlannedElement {
                stockpile: stockpile.id(),
                element,
            });
        }
        let current = stored_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let next = current
            .checked_add(AggregateMass::from_mass(stockpile.stored_mass()))
            .ok_or(StateValidationError::StoredMatterMassOverflow { element })?;
        stored_mass_by_element.insert(element, next);
    }
    for structural in state.structures.elements() {
        let element = structural.id();
        let mass = stored_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
            .ok_or(StateValidationError::StoredMatterWeightOverflow { element })?;
        let stored = structural.load(StructuralLoadKind::StoredMatter);
        if stored != expected {
            return Err(StateValidationError::StoredMatterStructuralLoadMismatch {
                element,
                stored,
                expected,
            });
        }
    }

    let structural_analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        &state.structures,
    )
    .map_err(StateValidationError::StructureAnalysis)?;
    if let Some(event) = structural_analysis.damage_events().first().copied() {
        return Err(StateValidationError::UnresolvedStructuralDamage { event });
    }
    validate_loaded_geology(registries.materials(), &state.geology, state.tick())
        .map_err(StateValidationError::Geology)?;
    validate_loaded_geological_knowledge(
        registries.materials(),
        &state.geological_knowledge,
        state.tick(),
    )
    .map_err(StateValidationError::GeologicalKnowledge)?;
    validate_loaded_production(&state.production).map_err(StateValidationError::Production)?;

    for stockpile in state.inventory.stockpiles() {
        for (commodity, _) in stockpile.contents() {
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownStoredCommodity {
                    stockpile: stockpile.id(),
                    commodity,
                });
            }
        }
    }
    for lot in state.inventory.lots() {
        if lot.created_at() > state.tick() {
            return Err(StateValidationError::LotCreatedInFuture {
                lot: lot.id(),
                created_at: lot.created_at(),
                current: state.tick(),
            });
        }
        if lot.latest_created_at() > state.tick() {
            return Err(StateValidationError::LotProvenanceInFuture {
                lot: lot.id(),
                latest_created_at: lot.latest_created_at(),
                current: state.tick(),
            });
        }
        for component in lot.composition().components() {
            if registries
                .materials()
                .get_material(component.material())
                .is_none()
            {
                return Err(StateValidationError::UnknownLotCompositionMaterial {
                    lot: lot.id(),
                    material: component.material(),
                });
            }
        }
    }

    let mut expected_reservations = BTreeMap::<StockpileId, Mass>::new();
    let mut occupied_energy = BTreeMap::<crate::energy::EnergyStoreId, ProductionJobId>::new();
    let mut occupied_equipment = BTreeMap::<EquipmentId, ProductionJobId>::new();
    for job in state.production.jobs() {
        if registries.production().get_process(job.process()).is_none() {
            return Err(StateValidationError::UnknownJobProcess {
                job: job.id(),
                process: job.process(),
            });
        }
        if state.inventory.get_stockpile(job.source()).is_none() {
            return Err(StateValidationError::UnknownJobSource {
                job: job.id(),
                stockpile: job.source(),
            });
        }
        let Some(destination_record) = state.inventory.get_stockpile(job.destination()) else {
            return Err(StateValidationError::UnknownJobDestination {
                job: job.id(),
                stockpile: job.destination(),
            });
        };
        if let Some(trace) = job.consumed_energy() {
            let Some(store) = state.energy.get_store(trace.source()) else {
                return Err(StateValidationError::UnknownJobEnergySource {
                    job: job.id(),
                    store: trace.source(),
                });
            };
            if store.definition() != trace.definition() {
                return Err(StateValidationError::JobEnergyDefinitionMismatch {
                    job: job.id(),
                    traced: trace.definition(),
                    stored: store.definition(),
                });
            }
            let Some(definition) = registries.energy().get_store(trace.definition()) else {
                return Err(StateValidationError::Energy(
                    EnergyValidationError::UnknownDefinition {
                        store: trace.source(),
                        definition: trace.definition(),
                    },
                ));
            };
            if definition.carrier() != trace.carrier() {
                return Err(StateValidationError::JobEnergyCarrierMismatch {
                    job: job.id(),
                    traced: trace.carrier(),
                    authored: definition.carrier(),
                });
            }
            if let Some(first) = occupied_energy.insert(trace.source(), job.id()) {
                return Err(StateValidationError::EnergyStoreDoubleBooked {
                    store: trace.source(),
                    first,
                    second: job.id(),
                });
            }
        }
        if let Some(trace) = job.released_energy() {
            let Some(store) = state.energy.get_store(trace.destination()) else {
                return Err(StateValidationError::UnknownJobEnergySink {
                    job: job.id(),
                    store: trace.destination(),
                });
            };
            if store.definition() != trace.definition() {
                return Err(StateValidationError::JobReleasedEnergyDefinitionMismatch {
                    job: job.id(),
                    traced: trace.definition(),
                    stored: store.definition(),
                });
            }
            let Some(definition) = registries.energy().get_store(trace.definition()) else {
                return Err(StateValidationError::Energy(
                    EnergyValidationError::UnknownDefinition {
                        store: trace.destination(),
                        definition: trace.definition(),
                    },
                ));
            };
            if definition.carrier() != trace.carrier() {
                return Err(StateValidationError::JobReleasedEnergyCarrierMismatch {
                    job: job.id(),
                    traced: trace.carrier(),
                    authored: definition.carrier(),
                });
            }
            if definition.max_input_power().is_zero() {
                return Err(StateValidationError::JobReleasedEnergySinkHasNoInputPower {
                    job: job.id(),
                    store: trace.destination(),
                });
            }
            let after = store.stored().checked_add(trace.energy()).ok_or(
                StateValidationError::JobReleasedEnergyCapacityOverflow {
                    job: job.id(),
                    store: trace.destination(),
                },
            )?;
            if after > definition.capacity() {
                return Err(StateValidationError::JobReleasedEnergyCapacityExceeded {
                    job: job.id(),
                    store: trace.destination(),
                    stored: store.stored(),
                    released: trace.energy(),
                    capacity: definition.capacity(),
                });
            }
            if let Some(first) = occupied_energy.insert(trace.destination(), job.id()) {
                return Err(StateValidationError::EnergyStoreDoubleBooked {
                    store: trace.destination(),
                    first,
                    second: job.id(),
                });
            }
        }
        if let Some(provider) = job.equipment_provider() {
            let Some(record) = state.equipment.get_equipment(provider.equipment()) else {
                return Err(StateValidationError::UnknownJobEquipment {
                    job: job.id(),
                    equipment: provider.equipment(),
                });
            };
            if record.definition() != provider.definition() {
                return Err(StateValidationError::JobEquipmentDefinitionMismatch {
                    job: job.id(),
                    traced: provider.definition(),
                    stored: record.definition(),
                });
            }
            if record.condition() != provider.condition() {
                return Err(StateValidationError::JobEquipmentConditionMismatch {
                    job: job.id(),
                    traced: provider.condition(),
                    stored: record.condition(),
                });
            }
            if let Some(first) = occupied_equipment.insert(provider.equipment(), job.id()) {
                return Err(StateValidationError::EquipmentDoubleBooked {
                    equipment: provider.equipment(),
                    first,
                    second: job.id(),
                });
            }
        }
        validate_loaded_comminution_job(registries, job)
            .map_err(StateValidationError::ComminutionJob)?;
        validate_loaded_thermal_job(registries, job).map_err(StateValidationError::ThermalJob)?;
        if job.completes_at() <= state.tick() {
            return Err(StateValidationError::JobAlreadyDue {
                job: job.id(),
                current: state.tick(),
                due: job.completes_at(),
            });
        }

        for trace in job.consumed_inputs() {
            let commodity = trace.profile().commodity();
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownJobConsumedCommodity {
                    job: job.id(),
                    commodity,
                });
            }
            for component in trace.profile().composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(
                        StateValidationError::UnknownJobConsumedCompositionMaterial {
                            job: job.id(),
                            material: component.material(),
                        },
                    );
                }
            }
        }

        for output in job.outputs() {
            if !registries.materials().has_commodity(output.commodity()) {
                return Err(StateValidationError::UnknownJobOutputCommodity {
                    job: job.id(),
                    commodity: output.commodity(),
                });
            }
            for component in output.composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(StateValidationError::UnknownJobOutputCompositionMaterial {
                        job: job.id(),
                        material: component.material(),
                    });
                }
            }
            validate_stockpile_storage(
                registries,
                destination_record,
                job.destination(),
                output.commodity(),
                output.composition(),
                output.temperature(),
            )
            .map_err(|error| StateValidationError::JobOutputStorage {
                job: job.id(),
                error,
            })?;
        }
        let output_mass = sum_lot_spec_mass(job.outputs())
            .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
        let current = expected_reservations
            .get(&job.destination())
            .copied()
            .unwrap_or(Mass::ZERO);
        let expected =
            current
                .checked_add(output_mass)
                .ok_or(StateValidationError::ReservedMassOverflow {
                    stockpile: job.destination(),
                })?;
        expected_reservations.insert(job.destination(), expected);
    }

    for stockpile in state.inventory.stockpiles() {
        let expected = expected_reservations
            .get(&stockpile.id())
            .copied()
            .unwrap_or(Mass::ZERO);
        if stockpile.reserved_inbound() != expected {
            return Err(StateValidationError::ReservedInboundMismatch {
                stockpile: stockpile.id(),
                reserved: stockpile.reserved_inbound(),
                expected,
            });
        }
    }

    Ok(())
}

/// Asserts every cheap runtime invariant in debug builds.
pub fn validate_invariants(_registries: &Registries, state: &AppState) {
    debug_assert!(
        state.random.has_valid_core_stream(),
        "Runtime Invariant 11 (Serialization Completeness): core RNG stream must remain valid"
    );
    debug_assert!(
        state.energy.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): energy store ID cursor must remain valid"
    );
    debug_assert!(
        state.fluid.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): fluid store ID cursor must remain valid"
    );
    debug_assert!(
        state.fluid.has_valid_records(),
        "Runtime Invariant 6 (Lifecycle Validity): fluid stores must have nonzero capacity and canonical nonempty contents"
    );
    debug_assert!(
        state.equipment.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): equipment ID cursor must remain valid"
    );
    debug_assert!(
        state.equipment.has_valid_support_index(),
        "Runtime Invariant 12 (Derived Data Consistency): equipment support reverse index must match support ownership"
    );
    debug_assert!(
        state.structures.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): structural ID cursor must remain valid"
    );
    debug_assert!(
        state.structures.has_valid_geometry(),
        "Runtime Invariant 6 (Lifecycle Validity): structural geometry must remain physically valid"
    );
    debug_assert!(
        state.geology.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological deposit ID cursor must remain valid"
    );
    debug_assert!(
        state.geological_knowledge.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): geological observation ID cursor must remain valid"
    );
    debug_assert!(
        state.inventory.has_valid_id_cursors(),
        "Runtime Invariant 8 (No Lost Runtime State): inventory ID cursors must remain nonzero"
    );
    debug_assert!(
        state.inventory.has_valid_support_index(),
        "Runtime Invariant 12 (Derived Data Consistency): inventory support reverse index must match stockpile support ownership"
    );
    debug_assert!(
        state.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production ID cursor must remain nonzero"
    );
    debug_assert!(
        state.production.has_valid_equipment_condition_outcomes(),
        "Runtime Invariant 6 (Lifecycle Validity): equipment-backed jobs must carry non-improving post-operation condition outcomes"
    );
    debug_assert!(
        state.production.has_unique_energy_reservations(),
        "Runtime Invariant 5 (Ownership Exclusivity): active production jobs must not share finite energy sources or sinks"
    );
    debug_assert!(
        state
            .production
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no active production job may remain due"
    );
}

pub(crate) fn apply_clock_advance(state: &mut AppState, next_tick: SimulationTick) {
    state.clock.tick = next_tick;
}

#[cfg(test)]
pub(crate) fn make_test_state_at_tick(world_seed: WorldSeed, tick: SimulationTick) -> AppState {
    let mut state = AppState::new(world_seed);
    state.clock.tick = tick;
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries, make_test_registries_with_process,
    };
    use crate::core::quantity::{Area, Force, Mass, Temperature};
    use crate::core::rng::RngAlgorithm;
    use crate::inventory::{add_stockpile, deposit_bulk_for_test, validate_transfer_bulk};
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
    use crate::matter::calculate_matter_accounting;
    use crate::production::{
        ProcessDefinition, ProcessId, ProcessResolution, make_test_process_resolution,
        validate_process_inputs, validate_start_process,
    };
    use crate::simulation::advance_tick;
    use crate::spatial::{VoxelBounds, VoxelCoord};
    use crate::structural::{
        StructuralElementId, StructuralLoadKind, StructuralMutationOutcome,
        ValidatedStructuralMutation, add_structural_element,
        materialize_structural_element_for_test, validate_activate_structural_element,
        validate_link_support, validate_set_structural_load,
    };

    const SOAK_PROCESS: ProcessId = ProcessId::new(900_201);

    fn make_test_soak_process() -> ProcessDefinition {
        ProcessDefinition::new(
            SOAK_PROCESS,
            "soak material transform",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        )
    }

    fn make_test_soak_resolution(
        registries: &Registries,
        state: &AppState,
        source: crate::inventory::StockpileId,
    ) -> ProcessResolution {
        let inputs = match validate_process_inputs(registries, state, SOAK_PROCESS, source) {
            Ok(inputs) => inputs,
            Err(error) => panic!("soak process input binding failed: {error}"),
        };
        make_test_process_resolution(
            inputs,
            29,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(450_000),
            )],
        )
    }

    fn add_soak_stockpile(state: &mut AppState, capacity: u64) -> crate::inventory::StockpileId {
        match add_stockpile(state, Mass::from_milligrams(capacity)) {
            Ok(id) => id,
            Err(error) => panic!("soak stockpile allocation failed: {error}"),
        }
    }

    fn make_soak_structural_bounds(x: i64, y: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, y, 0), VoxelCoord::new(x + 1, y + 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("soak structural bounds failed: {error}"),
        }
    }

    fn add_soak_structural_element(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
        y: i64,
        is_grounded: bool,
    ) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                make_soak_structural_bounds(x, y),
                crate::core::quantity::Length::from_micrometers(1),
                Area::from_square_millimeters(1_000),
            ),
            is_grounded,
        ) {
            Ok(element) => element,
            Err(error) => panic!("soak structural element allocation failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
        element
    }

    fn commit_soak_structural_mutation(
        token: ValidatedStructuralMutation,
        state: &mut AppState,
    ) -> StructuralMutationOutcome {
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("soak structural mutation failed: {error}"),
        }
    }

    fn build_soak_structure(registries: &Registries, state: &mut AppState) -> StructuralElementId {
        let left = add_soak_structural_element(registries, state, 0, 0, true);
        let right = add_soak_structural_element(registries, state, 2, 0, true);
        let deck = add_soak_structural_element(registries, state, 1, 1, false);

        for element in [left, right] {
            let token = match validate_activate_structural_element(registries, state, element) {
                Ok(token) => token,
                Err(error) => panic!("soak structural support activation failed: {error}"),
            };
            commit_soak_structural_mutation(token, state);
        }
        for support in [left, right] {
            let token = match validate_link_support(registries, state, deck, support) {
                Ok(token) => token,
                Err(error) => panic!("soak structural support link failed: {error}"),
            };
            commit_soak_structural_mutation(token, state);
        }
        let activation = match validate_activate_structural_element(registries, state, deck) {
            Ok(token) => token,
            Err(error) => panic!("soak deck activation failed: {error}"),
        };
        commit_soak_structural_mutation(activation, state);

        // Begin the soak with visible persistent damage but below post-crack failure capacity.
        let initial_load = match validate_set_structural_load(
            registries,
            state,
            deck,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(35_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak initial structural load failed: {error}"),
        };
        let outcome = commit_soak_structural_mutation(initial_load, state);
        assert_eq!(outcome.analysis().damage_events().len(), 1);
        deck
    }

    fn vary_soak_structural_load(
        registries: &Registries,
        state: &mut AppState,
        deck: StructuralElementId,
        step: u64,
    ) {
        let load = if (step / 19).is_multiple_of(2) {
            Force::from_millinewtons(20_000_000)
        } else {
            Force::from_millinewtons(35_000_000)
        };
        let token = match validate_set_structural_load(
            registries,
            state,
            deck,
            StructuralLoadKind::Snow,
            load,
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak structural load validation failed at step {step}: {error}"),
        };
        let outcome = commit_soak_structural_mutation(token, state);
        assert!(
            outcome.analysis().damage_events().is_empty(),
            "soak structural load generated unexpected new damage at step {step}"
        );
    }

    fn schedule_soak_process(
        registries: &Registries,
        state: &mut AppState,
        source: crate::inventory::StockpileId,
        processing: crate::inventory::StockpileId,
        wood: CommodityKey,
    ) {
        let available = match state.inventory().get_stockpile(source) {
            Some(record) => record.get_mass(wood),
            None => panic!("soak source disappeared"),
        };
        if available < Mass::from_milligrams(10) {
            return;
        }
        let resolution = make_test_soak_resolution(registries, state, source);
        let token = match validate_start_process(registries, state, &resolution, source, processing)
        {
            Ok(token) => token,
            Err(error) => panic!("soak process validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("soak process commit failed: {error}");
        }
    }

    fn transfer_soak_output(
        registries: &Registries,
        state: &mut AppState,
        processing: crate::inventory::StockpileId,
        archive: crate::inventory::StockpileId,
        charcoal: CommodityKey,
    ) {
        let available = match state.inventory().get_stockpile(processing) {
            Some(record) => record.get_mass(charcoal),
            None => panic!("soak processing stockpile disappeared"),
        };
        if available < Mass::from_milligrams(1) {
            return;
        }
        let token = match validate_transfer_bulk(
            registries,
            state,
            processing,
            archive,
            charcoal,
            Mass::from_milligrams(1),
        ) {
            Ok(token) => token,
            Err(error) => panic!("soak transfer validation failed: {error}"),
        };
        if let Err(error) = token.commit(state) {
            panic!("soak transfer commit failed: {error}");
        }
    }

    fn run_test_soak(seed: WorldSeed) -> AppState {
        let registries = make_test_registries_with_process(make_test_soak_process());
        let mut state = AppState::new(seed);
        let source = add_soak_stockpile(&mut state, 30_000);
        let processing = add_soak_stockpile(&mut state, 10_000);
        let archive = add_soak_stockpile(&mut state, 10_000);
        let structural_deck = build_soak_structure(&registries, &mut state);
        let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
        let charcoal = CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP);
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            source,
            wood,
            Mass::from_milligrams(20_000),
        ) {
            panic!("soak source deposit failed: {error}");
        }
        let initial_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("soak initial matter accounting failed: {error}"),
        };

        for step in 0_u64..10_000 {
            if step % 11 == 0 {
                schedule_soak_process(&registries, &mut state, source, processing, wood);
            }
            if step % 17 == 0 {
                transfer_soak_output(&registries, &mut state, processing, archive, charcoal);
            }
            if step % 19 == 0 {
                vary_soak_structural_load(&registries, &mut state, structural_deck, step);
            }
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("soak tick {step} failed: {error}");
            }
            if step % 257 == 0
                && let Err(error) = validate_loaded_state(&registries, &state)
            {
                panic!("soak exhaustive audit failed at step {step}: {error}");
            }
            if step % 257 == 0 {
                let accounted = match calculate_matter_accounting(&state) {
                    Ok(accounting) => accounting.total(),
                    Err(error) => panic!("soak matter accounting failed at step {step}: {error}"),
                };
                assert_eq!(
                    accounted, initial_matter,
                    "soak matter ownership changed at step {step}"
                );
            }
        }

        if let Err(error) = validate_loaded_state(&registries, &state) {
            panic!("soak final state failed validation: {error}");
        }
        let final_matter = match calculate_matter_accounting(&state) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("soak final matter accounting failed: {error}"),
        };
        assert_eq!(final_matter, initial_matter);
        state
    }

    #[test]
    fn new_state_starts_at_zero_with_versioned_rng() {
        let registries = build_registries();
        let state = AppState::new(WorldSeed::new(42));

        assert_eq!(state.world_seed(), WorldSeed::new(42));
        assert_eq!(state.tick(), SimulationTick::ZERO);
        assert_eq!(state.rng_algorithm(), RngAlgorithm::Xoshiro256StarStarV1);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn test_headless_mixed_system_soak_preserves_invariants_and_determinism() {
        let seed = WorldSeed::new(0x5A0C_D37E_4D11_0001);
        let first = run_test_soak(seed);
        let second = run_test_soak(seed);

        assert_eq!(first, second);
        assert_eq!(first.tick(), SimulationTick::new(10_000));
        assert!(
            first.inventory().lots().count() <= 8,
            "soak generated unbounded material-lot fragmentation"
        );
    }
}
