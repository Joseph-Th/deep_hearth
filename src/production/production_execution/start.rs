//! Process-start validation and atomic commit; sibling completion handles in-flight scheduling and output.

use super::*;

/// Explicit route assigning one resolved physical stream to one stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessOutputRoute {
    stream: ProcessOutputStreamId,
    destination: StockpileId,
}

impl ProcessOutputRoute {
    #[must_use]
    pub const fn new(stream: ProcessOutputStreamId, destination: StockpileId) -> Self {
        Self {
            stream,
            destination,
        }
    }

    #[must_use]
    pub const fn stream(self) -> ProcessOutputStreamId {
        self.stream
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }
}

/// Failure while validating the start of one durable material-processing job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessError {
    UnknownProcess {
        process: ProcessId,
    },
    ManualCraftRequiresPlayerWork {
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
    MatterBalanceMismatch {
        input_mass: Mass,
        output_mass: Mass,
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
        store: crate::energy::EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
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
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StartProcessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => {
                write!(formatter, "unknown process id {}", process.value())
            }
            Self::ManualCraftRequiresPlayerWork { process } => write!(
                formatter,
                "manual craft process {} must start through the player-work boundary",
                process.value()
            ),
            Self::UnknownOutputMaterial { material } => {
                write!(
                    formatter,
                    "resolved output references unknown material {}",
                    material.value()
                )
            }
            Self::UnknownOutputForm { form } => {
                write!(
                    formatter,
                    "resolved output references unknown form {}",
                    form.value()
                )
            }
            Self::UnknownOutputCompositionMaterial { material } => write!(
                formatter,
                "resolved output composition references unknown material {}",
                material.value()
            ),
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
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
            Self::DestinationStorage(error) => {
                write!(
                    formatter,
                    "process destination rejects resolved output: {error}"
                )
            }
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
            Self::MatterBalanceMismatch {
                input_mass,
                output_mass,
            } => write!(
                formatter,
                "resolved process accounts for {} mg of output from {} mg of input",
                output_mass.milligrams(),
                input_mass.milligrams()
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
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "process start cannot update stored-matter load: {error}"
                )
            }
        }
    }
}

impl Error for StartProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownProcess { process: _process } => None,
            Self::ManualCraftRequiresPlayerWork { process: _process } => None,
            Self::UnknownOutputMaterial {
                material: _material,
            }
            | Self::UnknownOutputCompositionMaterial {
                material: _material,
            } => None,
            Self::UnknownOutputForm { form: _form } => None,
            Self::UnknownStockpile {
                stockpile: _stockpile,
            }
            | Self::MassOverflow {
                stockpile: _stockpile,
            } => None,
            Self::OutputRouteCountMismatch {
                streams: _streams,
                routes: _routes,
            } => None,
            Self::DuplicateOutputRoute { stream: _stream }
            | Self::UnknownOutputRoute { stream: _stream }
            | Self::MissingOutputRoute { stream: _stream } => None,
            Self::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed_after_consumption: _committed_after_consumption,
                requested_inbound: _requested_inbound,
            } => None,
            Self::MatterBalanceMismatch {
                input_mass: _input_mass,
                output_mass: _output_mass,
            } => None,
            Self::CompletionTickOverflow {
                current: _current,
                duration_ticks: _duration_ticks,
            } => None,
            Self::ResolutionSourceMismatch {
                bound: _bound,
                requested: _requested,
            } => None,
            Self::StaleResolvedInputs {
                expected_inventory_revision: _expected_inventory_revision,
                actual_inventory_revision: _actual_inventory_revision,
            } => None,
            Self::StaleResolvedEnergy {
                expected_energy_revision: _expected_energy_revision,
                actual_energy_revision: _actual_energy_revision,
            } => None,
            Self::StaleResolvedEquipment {
                expected_equipment_revision: _expected_equipment_revision,
                actual_equipment_revision: _actual_equipment_revision,
            } => None,
            Self::StaleResolvedStructure {
                expected_structure_revision: _expected_structure_revision,
                actual_structure_revision: _actual_structure_revision,
            } => None,
            Self::EnergyStoreBusy {
                store: _store,
                job: _job,
                release: _release,
            } => None,
            Self::ResolvedEquipmentMissing {
                equipment: _equipment,
            }
            | Self::ResolvedEquipmentDefinitionChanged {
                equipment: _equipment,
            }
            | Self::ResolvedEquipmentConditionChanged {
                equipment: _equipment,
            } => None,
            Self::ResolvedEquipmentSupportChanged {
                equipment: _equipment,
                expected: _expected,
                actual: _actual,
            } => None,
            Self::ResolvedEquipmentSupportMissing {
                equipment: _equipment,
                element: _element,
            } => None,
            Self::ResolvedEquipmentSupportNotActive {
                equipment: _equipment,
                element: _element,
                lifecycle: _lifecycle,
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
            Self::JobIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::ProductionRevisionExhausted
            | Self::EnergyRevisionExhausted
            | Self::ResolvedEnergyStoreMissing
            | Self::ResolvedEnergyInsufficient
            | Self::ResolvedEnergySinkMissing
            | Self::ResolvedEnergySinkCapacity => None,
        }
    }
}

/// Failure when a validated process start is committed after either owning state has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessCommitError {
    StaleProductionRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StaleEnergyRevision { expected: u64, actual: u64 },
    StaleEquipmentRevision { expected: u64, actual: u64 },
    StaleStructureRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for StartProcessCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "validated process start expected production revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated process start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "validated process start expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated process start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated process start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated process start could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for StartProcessCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleProductionRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEnergyRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleStructureRevision {
                expected: _expected,
                actual: _actual,
            } => None,
        }
    }
}

/// Consumed proof that process references, matter, capacity, time, and job identity are valid.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedStartProcess {
    job: ProductionJobRecord,
    next_job_id: u64,
    expected_production_revision: u64,
    next_production_revision: u64,
    reservation: ConsumptionReservation,
    energy_reservation: Option<EnergyConsumptionReservation>,
    energy_ingress_reservation: Option<EnergyIngressReservation>,
    equipment_use: Option<ValidatedEquipmentUse>,
    destination_structure_revision: Option<u64>,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl ValidatedStartProcess {
    pub(crate) const fn job_id(&self) -> ProductionJobId {
        self.job.identity.id
    }

    /// Commits input consumption, output reservation, and job insertion as one canonical operation.
    pub fn commit(self, state: &mut AppState) -> Result<ProductionJobId, StartProcessCommitError> {
        let Self {
            job,
            next_job_id,
            expected_production_revision,
            next_production_revision,
            reservation,
            energy_reservation,
            energy_ingress_reservation,
            equipment_use,
            destination_structure_revision,
            structural_load,
        } = self;
        let job_id = job.id();

        let actual_production_revision = state.production().revision();
        if actual_production_revision != expected_production_revision {
            return Err(StartProcessCommitError::StaleProductionRevision {
                expected: expected_production_revision,
                actual: actual_production_revision,
            });
        }
        if let Some(energy) = energy_ingress_reservation {
            let expected_energy_revision = energy.expected_revision();
            let actual_energy_revision = state.energy().revision();
            if actual_energy_revision != expected_energy_revision {
                return Err(StartProcessCommitError::StaleEnergyRevision {
                    expected: expected_energy_revision,
                    actual: actual_energy_revision,
                });
            }
        }
        let expected_inventory_revision = reservation.expected_revision();
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != expected_inventory_revision {
            return Err(StartProcessCommitError::StaleInventoryRevision {
                expected: expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }
        if let Some(energy) = energy_reservation {
            let expected_energy_revision = energy.expected_revision();
            let actual_energy_revision = state.energy().revision();
            if actual_energy_revision != expected_energy_revision {
                return Err(StartProcessCommitError::StaleEnergyRevision {
                    expected: expected_energy_revision,
                    actual: actual_energy_revision,
                });
            }
        }
        if let Some(equipment) = equipment_use {
            let expected_equipment_revision = equipment.expected_equipment_revision();
            let actual_equipment_revision = state.equipment().revision();
            if actual_equipment_revision != expected_equipment_revision {
                return Err(StartProcessCommitError::StaleEquipmentRevision {
                    expected: expected_equipment_revision,
                    actual: actual_equipment_revision,
                });
            }
            if let Some(expected_structure_revision) = equipment.expected_structure_revision() {
                let actual_structure_revision = state.structures().revision();
                if actual_structure_revision != expected_structure_revision {
                    return Err(StartProcessCommitError::StaleStructureRevision {
                        expected: expected_structure_revision,
                        actual: actual_structure_revision,
                    });
                }
            }
        }
        if let Some(expected_structure_revision) = destination_structure_revision {
            let actual_structure_revision = state.structures().revision();
            if actual_structure_revision != expected_structure_revision {
                return Err(StartProcessCommitError::StaleStructureRevision {
                    expected: expected_structure_revision,
                    actual: actual_structure_revision,
                });
            }
        }
        if let Some(structural_load) = &structural_load {
            let expected_structure_revision = structural_load.expected_revision();
            let actual_structure_revision = state.structures().revision();
            if actual_structure_revision != expected_structure_revision {
                return Err(StartProcessCommitError::StaleStructureRevision {
                    expected: expected_structure_revision,
                    actual: actual_structure_revision,
                });
            }
        }
        if let Some(structural_load) = structural_load {
            structural_load
                .commit(state)
                .map_err(StartProcessCommitError::Structure)?;
        }
        apply_consumption_reservation(state.inventory_state_mut(), reservation).map_err(
            |error| match error {
                ReservationCommitError::StaleInventoryRevision { expected, actual } => {
                    StartProcessCommitError::StaleInventoryRevision { expected, actual }
                }
            },
        )?;
        if let Some(energy) = energy_reservation {
            apply_energy_consumption_reservation(state.energy_state_mut(), energy).map_err(
                |error| match error {
                    EnergyCommitError::StaleRevision { expected, actual } => {
                        StartProcessCommitError::StaleEnergyRevision { expected, actual }
                    }
                },
            )?;
        }
        state
            .production_state_mut()
            .insert_job(job, next_job_id, next_production_revision);
        Ok(job_id)
    }
}

/// Validates all preconditions for starting a timed material transformation without mutating state.
pub fn validate_start_process(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, StartProcessError> {
    let Some(stream) = resolution.single_output_stream() else {
        return Err(StartProcessError::OutputRouteCountMismatch {
            streams: resolution.output_streams().len(),
            routes: 1,
        });
    };
    validate_start_process_routed_internal(
        registries,
        state,
        resolution,
        source,
        &[ProcessOutputRoute::new(stream.id(), destination)],
        false,
    )
}

pub(crate) fn validate_start_manual_process(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    destination: StockpileId,
) -> Result<ValidatedStartProcess, StartProcessError> {
    let Some(stream) = resolution.single_output_stream() else {
        return Err(StartProcessError::OutputRouteCountMismatch {
            streams: resolution.output_streams().len(),
            routes: 1,
        });
    };
    validate_start_process_routed_internal(
        registries,
        state,
        resolution,
        source,
        &[ProcessOutputRoute::new(stream.id(), destination)],
        true,
    )
}

/// Validates a resolved process while assigning one destination to each inseparable output stream.
///
/// Routes bind typed stream identities rather than relying on vector position. Multiple streams may
/// intentionally share one stockpile; their capacity reservation is aggregated atomically.
pub fn validate_start_process_routed(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    routes: &[ProcessOutputRoute],
) -> Result<ValidatedStartProcess, StartProcessError> {
    validate_start_process_routed_internal(registries, state, resolution, source, routes, false)
}

fn validate_start_process_routed_internal(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    routes: &[ProcessOutputRoute],
    allow_manual_craft: bool,
) -> Result<ValidatedStartProcess, StartProcessError> {
    let process = resolution.process();
    if source != resolution.source() {
        return Err(StartProcessError::ResolutionSourceMismatch {
            bound: resolution.source(),
            requested: source,
        });
    }
    if registries.production().get_process(process).is_none() {
        return Err(StartProcessError::UnknownProcess { process });
    }
    if !allow_manual_craft && registries.crafting().get_manual(process).is_some() {
        return Err(StartProcessError::ManualCraftRequiresPlayerWork { process });
    }
    if routes.len() != resolution.output_streams().len() {
        return Err(StartProcessError::OutputRouteCountMismatch {
            streams: resolution.output_streams().len(),
            routes: routes.len(),
        });
    }
    let stream_ids = resolution
        .output_streams()
        .iter()
        .map(|stream| stream.id())
        .collect::<BTreeSet<_>>();
    let mut destinations_by_stream = BTreeMap::new();
    for route in routes {
        if !stream_ids.contains(&route.stream()) {
            return Err(StartProcessError::UnknownOutputRoute {
                stream: route.stream(),
            });
        }
        if destinations_by_stream
            .insert(route.stream(), route.destination())
            .is_some()
        {
            return Err(StartProcessError::DuplicateOutputRoute {
                stream: route.stream(),
            });
        }
    }

    for stream in resolution.output_streams() {
        for output in stream.outputs() {
            if registries
                .materials()
                .get_material(output.commodity().material())
                .is_none()
            {
                return Err(StartProcessError::UnknownOutputMaterial {
                    material: output.commodity().material(),
                });
            }
            if registries
                .materials()
                .get_form(output.commodity().form())
                .is_none()
            {
                return Err(StartProcessError::UnknownOutputForm {
                    form: output.commodity().form(),
                });
            }
            for component in output.composition().components() {
                if registries
                    .materials()
                    .get_material(component.material())
                    .is_none()
                {
                    return Err(StartProcessError::UnknownOutputCompositionMaterial {
                        material: component.material(),
                    });
                }
            }
        }
    }

    let mut inbound_by_destination = BTreeMap::<StockpileId, Mass>::new();
    let mut output_streams = Vec::with_capacity(resolution.output_streams().len());
    for stream in resolution.output_streams() {
        let destination = destinations_by_stream.get(&stream.id()).copied().ok_or(
            StartProcessError::MissingOutputRoute {
                stream: stream.id(),
            },
        )?;
        let Some(destination_record) = state.inventory().get_stockpile(destination) else {
            return Err(StartProcessError::UnknownStockpile {
                stockpile: destination,
            });
        };
        for output in stream.outputs() {
            validate_stockpile_storage(
                registries,
                destination_record,
                destination,
                output.commodity(),
                output.composition(),
                output.temperature(),
                output.particle_size_distribution(),
            )
            .map_err(StartProcessError::DestinationStorage)?;
        }
        let stream_mass = match sum_lot_spec_mass(stream.outputs()) {
            Some(mass) => mass,
            None => panic!("resolved process stream mass overflowed after resolution validation"),
        };
        let current = inbound_by_destination
            .get(&destination)
            .copied()
            .unwrap_or(Mass::ZERO);
        let inbound = current
            .checked_add(stream_mass)
            .ok_or(StartProcessError::MassOverflow {
                stockpile: destination,
            })?;
        inbound_by_destination.insert(destination, inbound);
        output_streams.push(ProductionOutputStream {
            id: stream.id(),
            destination,
            outputs: stream.outputs().to_vec(),
        });
    }
    let mut destination_structure_revision = None;
    for destination in inbound_by_destination.keys().copied() {
        if let Some(revision) = validate_stockpile_support_for_new_inbound(state, destination)
            .map_err(StartProcessError::StructuralLoad)?
        {
            if let Some(existing) = destination_structure_revision {
                debug_assert_eq!(existing, revision);
            } else {
                destination_structure_revision = Some(revision);
            }
        }
    }

    let current = state.tick();
    let Some(completes_at) = current.checked_add_span(resolution.duration()) else {
        return Err(StartProcessError::CompletionTickOverflow {
            current,
            duration_ticks: resolution.duration().value(),
        });
    };

    let next_job_value = state.production().next_job_id();
    let Some(next_after) = next_job_value.checked_add(1) else {
        return Err(StartProcessError::JobIdExhausted);
    };
    let job_id = ProductionJobId::new(next_job_value);
    let expected_production_revision = state.production().revision();
    let Some(next_production_revision) = expected_production_revision.checked_add(1) else {
        return Err(StartProcessError::ProductionRevisionExhausted);
    };

    let output_mass = match sum_output_stream_mass(resolution.output_streams()) {
        Some(mass) => mass,
        None => panic!("resolved process output mass overflowed after resolution validation"),
    };
    let input_mass = resolution.input_mass();
    if output_mass != input_mass {
        return Err(StartProcessError::MatterBalanceMismatch {
            input_mass,
            output_mass,
        });
    }
    let reservation = validate_consumption_reservation_from_selection(
        state.inventory(),
        resolution.selection().clone(),
        inbound_by_destination,
    )
    .map_err(map_reservation_error)?;
    let consumed_inputs = reservation.consumed_inputs().to_vec();
    let energy_reservation = match resolution.energy_supply() {
        Some(selection) => Some(
            validate_energy_consumption_reservation(state.energy(), selection)
                .map_err(map_energy_reservation_error)?,
        ),
        None => None,
    };
    let consumed_energy = energy_reservation.map(EnergyConsumptionReservation::trace);
    let energy_ingress_reservation = match resolution.energy_sink() {
        Some(selection) => Some(
            validate_energy_ingress_reservation(registries, state.energy(), selection)
                .map_err(map_energy_ingress_reservation_error)?,
        ),
        None => None,
    };
    let released_energy = energy_ingress_reservation.map(EnergyIngressReservation::trace);
    for store in consumed_energy
        .map(|trace| trace.source())
        .into_iter()
        .chain(released_energy.map(|trace| trace.destination()))
    {
        if let Some(job_id) = state.production().get_energy_occupant(store) {
            let job = match state.production().get_job(job_id) {
                Some(job) => job,
                None => panic!(
                    "runtime invariant broken: energy occupancy index references missing production job {}",
                    job_id.value()
                ),
            };
            return Err(StartProcessError::EnergyStoreBusy {
                store,
                job: job_id,
                release: job.occupancy_release(),
            });
        }
    }
    let equipment_use = resolution.equipment_use();
    let equipment_provider = match equipment_use {
        Some(selection) => {
            let expected = selection.expected_equipment_revision();
            let actual = state.equipment().revision();
            if actual != expected {
                return Err(StartProcessError::StaleResolvedEquipment {
                    expected_equipment_revision: expected,
                    actual_equipment_revision: actual,
                });
            }
            let trace = selection.trace();
            let Some(record) = state.equipment().get_equipment(trace.equipment()) else {
                return Err(StartProcessError::ResolvedEquipmentMissing {
                    equipment: trace.equipment(),
                });
            };
            if record.definition() != trace.definition() {
                return Err(StartProcessError::ResolvedEquipmentDefinitionChanged {
                    equipment: trace.equipment(),
                });
            }
            if record.condition() != trace.condition() {
                return Err(StartProcessError::ResolvedEquipmentConditionChanged {
                    equipment: trace.equipment(),
                });
            }
            let expected_support = selection.support();
            let actual_support = record.supported_by();
            if actual_support != expected_support {
                return Err(StartProcessError::ResolvedEquipmentSupportChanged {
                    equipment: trace.equipment(),
                    expected: expected_support,
                    actual: actual_support,
                });
            }
            if let Some(expected_structure_revision) = selection.expected_structure_revision() {
                let actual_structure_revision = state.structures().revision();
                if actual_structure_revision != expected_structure_revision {
                    return Err(StartProcessError::StaleResolvedStructure {
                        expected_structure_revision,
                        actual_structure_revision,
                    });
                }
                let element = match expected_support {
                    Some(element) => element,
                    None => panic!(
                        "validated equipment use has structural revision without a support element"
                    ),
                };
                let Some(support) = state.structures().get_element(element) else {
                    return Err(StartProcessError::ResolvedEquipmentSupportMissing {
                        equipment: trace.equipment(),
                        element,
                    });
                };
                if support.lifecycle() != StructuralLifecycle::Active {
                    return Err(StartProcessError::ResolvedEquipmentSupportNotActive {
                        equipment: trace.equipment(),
                        element,
                        lifecycle: support.lifecycle(),
                    });
                }
            }
            if let Some(job) = state.production().get_equipment_occupant(trace.equipment()) {
                return Err(StartProcessError::EquipmentBusy {
                    equipment: trace.equipment(),
                    job: job.id(),
                    release: job.occupancy_release(),
                });
            }
            if let Some(job) = state.mining().get_equipment_occupant(trace.equipment()) {
                return Err(StartProcessError::EquipmentBusyMining {
                    equipment: trace.equipment(),
                    job,
                });
            }
            Some(trace)
        }
        None => None,
    };
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(StartProcessError::UnknownStockpile { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(input_mass)
        .ok_or(StartProcessError::MassOverflow { stockpile: source })?;
    let structural_load = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StartProcessError::StructuralLoad)?;

    Ok(ValidatedStartProcess {
        job: ProductionJobRecord {
            identity: ProductionJobIdentity {
                id: job_id,
                process,
                source,
            },
            schedule: ProductionJobSchedule {
                started_at: current,
                completes_at,
                active_duration: resolution.duration(),
                suspension: None,
            },
            resources: ProductionJobResources {
                consumed_mass: input_mass,
                consumed_inputs,
                consumed_energy,
                released_energy,
            },
            equipment: ProductionJobEquipment {
                provider: equipment_provider,
                requires_active_support: equipment_use
                    .is_some_and(|selection| selection.support().is_some()),
                condition_after: resolution.equipment_condition_after(),
            },
            output_streams,
        },
        next_job_id: next_after,
        expected_production_revision,
        next_production_revision,
        reservation,
        energy_reservation,
        energy_ingress_reservation,
        equipment_use,
        destination_structure_revision,
        structural_load,
    })
}

fn map_energy_ingress_reservation_error(error: EnergyIngressReservationError) -> StartProcessError {
    match error {
        EnergyIngressReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedEnergy {
                expected_energy_revision: expected,
                actual_energy_revision: actual,
            }
        }
        EnergyIngressReservationError::UnknownStore { store: _store } => {
            StartProcessError::ResolvedEnergySinkMissing
        }
        EnergyIngressReservationError::CapacityOverflow { store: _store } => {
            StartProcessError::ResolvedEnergySinkCapacity
        }
        EnergyIngressReservationError::InsufficientCapacity {
            store: _store,
            stored: _stored,
            requested: _requested,
            capacity: _capacity,
        } => StartProcessError::ResolvedEnergySinkCapacity,
    }
}

fn map_energy_reservation_error(error: EnergyReservationError) -> StartProcessError {
    match error {
        EnergyReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedEnergy {
                expected_energy_revision: expected,
                actual_energy_revision: actual,
            }
        }
        EnergyReservationError::UnknownStore { store: _store } => {
            StartProcessError::ResolvedEnergyStoreMissing
        }
        EnergyReservationError::InsufficientEnergy {
            store: _store,
            available: _available,
            requested: _requested,
        } => StartProcessError::ResolvedEnergyInsufficient,
        EnergyReservationError::RevisionExhausted => StartProcessError::EnergyRevisionExhausted,
    }
}

fn map_reservation_error(error: ReservationError) -> StartProcessError {
    match error {
        ReservationError::UnknownStockpile { stockpile } => {
            StartProcessError::UnknownStockpile { stockpile }
        }
        ReservationError::MassOverflow { stockpile } => {
            StartProcessError::MassOverflow { stockpile }
        }
        ReservationError::CapacityExceeded {
            stockpile,
            capacity,
            committed_after_consumption,
            requested_inbound,
        } => StartProcessError::CapacityExceeded {
            stockpile,
            capacity,
            committed_after_consumption,
            requested_inbound,
        },
        ReservationError::RevisionExhausted => StartProcessError::InventoryRevisionExhausted,
        ReservationError::StaleSelection { expected, actual } => {
            StartProcessError::StaleResolvedInputs {
                expected_inventory_revision: expected,
                actual_inventory_revision: actual,
            }
        }
    }
}
