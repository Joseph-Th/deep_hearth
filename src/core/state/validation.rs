//! Persistent-state validation for root runtime; this child audits private owner data without exposing mutation.

use super::*;

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
    ScreeningJob(ScreeningJobValidationError),
    ThermalJob(ThermalJobValidationError),
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
            Self::ScreeningJob(error) => {
                write!(formatter, "invalid screening production job: {error}")
            }
            Self::ThermalJob(error) => write!(formatter, "invalid thermal production job: {error}"),
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
            Self::ComminutionJob(error) => Some(error),
            Self::ScreeningJob(error) => Some(error),
            Self::ThermalJob(error) => Some(error),
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
            Self::EnergyStoreDoubleBooked {
                store: _store,
                first: _first,
                second: _second,
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
            Self::EquipmentDoubleBooked {
                equipment: _equipment,
                first: _first,
                second: _second,
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

    validate_loaded_energy(registries.energy(), &state.systems.energy, state.tick())
        .map_err(StateValidationError::Energy)?;
    validate_loaded_fluid(registries.fluid(), &state.systems.fluid, state.tick())
        .map_err(StateValidationError::Fluid)?;
    validate_loaded_equipment(
        registries.equipment(),
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
    validate_loaded_inventory(registries.materials(), &state.systems.inventory)
        .map_err(StateValidationError::Inventory)?;

    let mut mounted_mass_by_element = BTreeMap::<StructuralElementId, AggregateMass>::new();
    for equipment in state.systems.equipment.equipment() {
        let Some(element) = equipment.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
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
    for structural in state.systems.structures.elements() {
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
    for stockpile in state.systems.inventory.stockpiles() {
        let Some(element) = stockpile.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
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
    for structural in state.systems.structures.elements() {
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

    for store in state.systems.fluid.stores() {
        let Some(element) = store.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
            return Err(StateValidationError::UnknownFluidSupport {
                store: store.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::FluidSupportedByPlannedElement {
                store: store.id(),
                element,
            });
        }
    }
    for structural in state.systems.structures.elements() {
        validate_existing_fluid_load(registries, state, structural.id())
            .map_err(StateValidationError::FluidStructuralLoad)?;
    }

    let structural_analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        &state.systems.structures,
    )
    .map_err(StateValidationError::StructureAnalysis)?;
    if let Some(event) = structural_analysis.damage_events().first().copied() {
        return Err(StateValidationError::UnresolvedStructuralDamage { event });
    }
    validate_loaded_geology(registries.materials(), &state.systems.geology, state.tick())
        .map_err(StateValidationError::Geology)?;
    validate_loaded_geological_knowledge(
        registries.materials(),
        &state.systems.geological_knowledge,
        state.tick(),
    )
    .map_err(StateValidationError::GeologicalKnowledge)?;
    validate_loaded_production(&state.systems.production)
        .map_err(StateValidationError::Production)?;

    for stockpile in state.systems.inventory.stockpiles() {
        for (commodity, _) in stockpile.contents() {
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownStoredCommodity {
                    stockpile: stockpile.id(),
                    commodity,
                });
            }
        }
    }
    for lot in state.systems.inventory.lots() {
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
    for job in state.systems.production.jobs() {
        if registries.production().get_process(job.process()).is_none() {
            return Err(StateValidationError::UnknownJobProcess {
                job: job.id(),
                process: job.process(),
            });
        }
        if state
            .systems
            .inventory
            .get_stockpile(job.source())
            .is_none()
        {
            return Err(StateValidationError::UnknownJobSource {
                job: job.id(),
                stockpile: job.source(),
            });
        }
        if let Some(trace) = job.consumed_energy() {
            let Some(store) = state.systems.energy.get_store(trace.source()) else {
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
            let Some(store) = state.systems.energy.get_store(trace.destination()) else {
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
            let Some(record) = state.systems.equipment.get_equipment(provider.equipment()) else {
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
        validate_loaded_screening_job(registries, job)
            .map_err(StateValidationError::ScreeningJob)?;
        validate_loaded_thermal_job(registries, job).map_err(StateValidationError::ThermalJob)?;
        if let Some(suspension) = job.suspension() {
            if suspension.suspended_at() > state.tick() {
                return Err(StateValidationError::JobSuspendedInFuture {
                    job: job.id(),
                    current: state.tick(),
                    suspended_at: suspension.suspended_at(),
                });
            }
        } else if job.completes_at() <= state.tick() {
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
            validate_material_particle_size_state(
                registries.materials(),
                commodity,
                trace.profile().particle_size_distribution(),
            )
            .map_err(|error| {
                StateValidationError::InvalidJobConsumedParticleSizeState {
                    job: job.id(),
                    error,
                }
            })?;
        }

        for stream in job.output_streams() {
            let destination = stream.destination();
            let Some(destination_record) = state.systems.inventory.get_stockpile(destination)
            else {
                return Err(StateValidationError::UnknownJobDestination {
                    job: job.id(),
                    stockpile: destination,
                });
            };
            for output in stream.outputs() {
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
                    destination,
                    output.commodity(),
                    output.composition(),
                    output.temperature(),
                    output.particle_size_distribution(),
                )
                .map_err(|error| StateValidationError::JobOutputStorage {
                    job: job.id(),
                    error,
                })?;
            }
            let output_mass = sum_lot_spec_mass(stream.outputs())
                .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
            let current = expected_reservations
                .get(&destination)
                .copied()
                .unwrap_or(Mass::ZERO);
            let expected = current.checked_add(output_mass).ok_or(
                StateValidationError::ReservedMassOverflow {
                    stockpile: destination,
                },
            )?;
            expected_reservations.insert(destination, expected);
        }
    }

    for stockpile in state.systems.inventory.stockpiles() {
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
        state.systems.energy.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): energy store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.fluid.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): fluid store ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.fluid.has_valid_records(),
        "Runtime Invariant 6 (Lifecycle Validity): fluid stores must have nonzero capacity and canonical nonempty contents"
    );
    debug_assert!(
        state.systems.fluid.has_valid_support_index(),
        "Runtime Invariant 12 (Derived Data Consistency): fluid support reverse index must match store support ownership"
    );
    debug_assert!(
        state.systems.equipment.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): equipment ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.equipment.has_valid_support_index(),
        "Runtime Invariant 12 (Derived Data Consistency): equipment support reverse index must match support ownership"
    );
    debug_assert!(
        state.systems.structures.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): structural ID cursor must remain valid"
    );
    debug_assert!(
        state.systems.structures.has_valid_geometry(),
        "Runtime Invariant 6 (Lifecycle Validity): structural geometry must remain physically valid"
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
        state.systems.inventory.has_valid_support_index(),
        "Runtime Invariant 12 (Derived Data Consistency): inventory support reverse index must match stockpile support ownership"
    );
    debug_assert!(
        state.systems.production.has_valid_id_cursor(),
        "Runtime Invariant 8 (No Lost Runtime State): production ID cursor must remain nonzero"
    );
    debug_assert!(
        state
            .systems
            .production
            .has_valid_equipment_condition_outcomes(),
        "Runtime Invariant 6 (Lifecycle Validity): equipment-backed jobs must carry non-improving post-operation condition outcomes"
    );
    debug_assert!(
        state.systems.production.has_valid_schedule_index(),
        "Runtime Invariants 3/6/12 (Index Completeness, Lifecycle Validity, Derived Data Consistency): production due-index and suspension scheduling must match active job records"
    );
    debug_assert!(
        state.systems.production.jobs().all(|job| {
            job.suspension()
                .is_none_or(|suspension| suspension.suspended_at() <= state.tick())
        }),
        "Runtime Invariant 6 (Lifecycle Validity): production suspension timestamps must not be later than the authoritative clock"
    );
    debug_assert!(
        state.systems.production.has_valid_energy_occupancy_index(),
        "Runtime Invariants 5/12 (Ownership Exclusivity, Derived Data Consistency): production energy occupancy index must contain exactly one owner for each active job reservation"
    );
    debug_assert!(
        state
            .systems
            .production
            .has_valid_equipment_occupancy_index(),
        "Runtime Invariants 5/12 (Ownership Exclusivity, Derived Data Consistency): production equipment occupancy index must contain exactly one owner for each active equipment reservation"
    );
    debug_assert!(
        state
            .systems
            .production
            .has_valid_stockpile_occupancy_index(),
        "Runtime Invariant 12 (Derived Data Consistency): production stockpile occupancy index must match every active job source and destination"
    );
    debug_assert!(
        state
            .systems
            .production
            .earliest_due_tick()
            .is_none_or(|due| due > state.tick()),
        "Runtime Invariant 6 (Lifecycle Validity): no active production job may remain due"
    );
}
