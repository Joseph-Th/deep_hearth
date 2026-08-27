//! Process-start admission and routing; child commit owns atomic mutation after validation.

mod commit;
mod routing;
pub use commit::StartProcessCommitError;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{
    EnergyConsumptionReservation, EnergyIngressReservation, EnergyIngressReservationError,
    EnergyReservationError, validate_energy_consumption_reservation,
    validate_energy_ingress_reservation,
};
use crate::equipment::{EquipmentId, ValidatedEquipmentUse};
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ConsumptionReservation, ReservationError, StockpileId,
    StockpileStorageError, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedStockpileStructuralLoad, validate_consumption_reservation_from_selection,
    validate_stockpile_stored_mass_changes,
};
use crate::material::{FormId, MaterialId};
use crate::mining::MiningJobId;
use crate::registry::Registries;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::definitions::ProcessId;
use super::super::resolution::{ProcessOutputStreamId, ProcessResolution, sum_output_stream_mass};
use super::super::state::{
    ProductionJobEquipment, ProductionJobId, ProductionJobIdentity, ProductionJobRecord,
    ProductionJobResources, ProductionJobSchedule, ProductionOccupancyRelease,
};
use routing::{ValidatedOutputRouting, validate_output_routing};

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
        store: crate::energy::EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EnergyStoreBusyManualPower {
        store: crate::energy::EnergyStoreId,
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
            Self::UnknownProcess { .. }
            | Self::ManualCraftRequiresPlayerWork { .. }
            | Self::UnknownOutputMaterial { .. }
            | Self::UnknownOutputForm { .. }
            | Self::UnknownOutputCompositionMaterial { .. }
            | Self::UnknownStockpile { .. }
            | Self::OutputRouteCountMismatch { .. }
            | Self::DuplicateOutputRoute { .. }
            | Self::UnknownOutputRoute { .. }
            | Self::MissingOutputRoute { .. }
            | Self::CapacityExceeded { .. }
            | Self::MassOverflow { .. }
            | Self::MatterBalanceMismatch { .. }
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
    let ValidatedOutputRouting {
        output_streams,
        inbound_by_destination,
        destination_structure_revision,
    } = validate_output_routing(registries, state, resolution, routes)?;

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
    let material_storage_history = reservation
        .oldest_storage_history_at(state.inventory(), current)
        .ok_or(StartProcessError::InputStorageAgeOverflow { stockpile: source })?;
    if material_storage_history
        .project(completes_at, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .is_none()
    {
        return Err(StartProcessError::InputStorageAgeOverflow { stockpile: source });
    }
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
        if state
            .player_work()
            .get_manual_power_energy_occupant(store)
            .is_some()
        {
            return Err(StartProcessError::EnergyStoreBusyManualPower { store });
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
            if state
                .player_work()
                .get_manual_power_equipment_occupant(trace.equipment())
                .is_some()
            {
                return Err(StartProcessError::EquipmentBusyManualPower {
                    equipment: trace.equipment(),
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
                material_storage_history,
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
