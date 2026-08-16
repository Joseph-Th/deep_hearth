//! Production validation, scheduling, completion planning, and atomic application for the sibling state owner.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{
    EnergyCommitError, EnergyConsumptionReservation, EnergyIngressReservation,
    EnergyIngressReservationError, EnergyReservationError, ReleasedEnergyTrace,
    apply_energy_consumption_reservation, apply_released_energy_outcomes,
    validate_energy_consumption_reservation, validate_energy_ingress_reservation,
};
use crate::equipment::{EquipmentId, EquipmentOperationConditionOutcome, ValidatedEquipmentUse};
use crate::inventory::{
    ConsumptionReservation, MaterialLotId, ReservationCommitError, ReservationError, StockpileId,
    StockpileStorageError, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedStockpileStructuralLoad, apply_consumption_reservation, apply_lot_cursor_and_revision,
    apply_reserved_deposit, next_material_lot_id, validate_consumption_reservation_from_selection,
    validate_stockpile_storage, validate_stockpile_stored_mass_changes,
    validate_stockpile_support_for_new_inbound,
};
use crate::material::{FormId, MaterialId, MaterialLotSpec};
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralElementId, StructuralLifecycle};

use super::definitions::ProcessId;
use super::resolution::{
    ProcessOutputStreamId, ProcessResolution, sum_lot_spec_mass, sum_output_stream_mass,
};
use super::state::{
    ProductionJobEquipment, ProductionJobId, ProductionJobIdentity, ProductionJobRecord,
    ProductionJobResources, ProductionJobSchedule, ProductionOccupancyRelease,
    ProductionOutputStream, ProductionSuspensionReason,
};

/// Explicit route assigning one resolved physical stream to one stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessOutputRoute {
    stream: ProcessOutputStreamId,
    destination: StockpileId,
}

/// Observable active-time scheduling change caused by a production provider becoming unavailable or
/// usable again. Work-in-process remains owned by the same job across both transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionAvailabilityChange {
    Suspended {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
    },
    Resumed {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    },
}

impl ProductionAvailabilityChange {
    #[must_use]
    pub const fn job(self) -> ProductionJobId {
        match self {
            Self::Suspended { job, .. } | Self::Resumed { job, .. } => job,
        }
    }
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
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StartProcessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => {
                write!(formatter, "unknown process id {}", process.value())
            }
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
            | Self::ResolvedEquipmentMissing { .. }
            | Self::ResolvedEquipmentDefinitionChanged { .. }
            | Self::ResolvedEquipmentConditionChanged { .. }
            | Self::ResolvedEquipmentSupportChanged { .. }
            | Self::ResolvedEquipmentSupportMissing { .. }
            | Self::ResolvedEquipmentSupportNotActive { .. }
            | Self::EquipmentBusy { .. } => None,
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
            Self::StaleProductionRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleStructureRevision { .. } => None,
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
    validate_start_process_routed(
        registries,
        state,
        resolution,
        source,
        &[ProcessOutputRoute::new(stream.id(), destination)],
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
        EnergyIngressReservationError::UnknownStore { .. } => {
            StartProcessError::ResolvedEnergySinkMissing
        }
        EnergyIngressReservationError::CapacityOverflow { .. }
        | EnergyIngressReservationError::InsufficientCapacity { .. } => {
            StartProcessError::ResolvedEnergySinkCapacity
        }
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
        EnergyReservationError::UnknownStore { .. } => {
            StartProcessError::ResolvedEnergyStoreMissing
        }
        EnergyReservationError::InsufficientEnergy { .. } => {
            StartProcessError::ResolvedEnergyInsufficient
        }
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

/// Observable completion emitted by one simulation tick after authoritative output is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCompletion {
    job: ProductionJobId,
    process: ProcessId,
    routes: Vec<ProcessOutputRoute>,
}

impl ProcessCompletion {
    #[must_use]
    pub const fn job(&self) -> ProductionJobId {
        self.job
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub fn routes(&self) -> &[ProcessOutputRoute] {
        &self.routes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionPlan {
    tick: SimulationTick,
    revisions: CompletionRevisionPlan,
    next_lot_id_after: u64,
    availability_changes: Vec<ProductionAvailabilityChange>,
    entries: Vec<CompletionPlanEntry>,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
    released_energy_outcomes: Vec<ReleasedEnergyTrace>,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionRevisionPlan {
    expected_inventory_revision: u64,
    next_inventory_revision: u64,
    expected_production_revision: u64,
    next_production_revision: u64,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_structure_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompletionPlanEntry {
    job: ProductionJobId,
    process: ProcessId,
    output_streams: Vec<CompletionOutputStreamPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompletionOutputStreamPlan {
    id: ProcessOutputStreamId,
    destination: StockpileId,
    outputs: Vec<MaterialLotSpec>,
    output_lot_ids: Vec<MaterialLotId>,
    reserved_mass: Mass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionPlanError {
    MaterialLotIds,
    InventoryRevision,
    ProductionRevision,
    EquipmentRevision,
    EnergyRevision,
    ResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionApplication {
    pub(crate) completions: Vec<ProcessCompletion>,
    pub(crate) availability_changes: Vec<ProductionAvailabilityChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionCommitError {
    InventoryStale { expected: u64, actual: u64 },
    ProductionRevisionChanged { expected: u64, actual: u64 },
    EquipmentRevisionConflict { expected: u64, actual: u64 },
    EnergyRevisionConflict { expected: u64, actual: u64 },
    StructureRevisionConflict { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

fn has_required_active_equipment_support(state: &AppState, job: &ProductionJobRecord) -> bool {
    if !job.has_required_active_support() {
        return true;
    }
    let provider = match job.equipment_provider() {
        Some(provider) => provider,
        None => panic!(
            "runtime invariant broken: support-dependent production job {} has no equipment provider",
            job.id().value()
        ),
    };
    let equipment = match state.equipment().get_equipment(provider.equipment()) {
        Some(record) => record,
        None => panic!(
            "runtime invariant broken: production job {} references missing equipment {}",
            job.id().value(),
            provider.equipment().value()
        ),
    };
    equipment.supported_by().is_some_and(|element| {
        state
            .structures()
            .get_element(element)
            .is_some_and(|support| support.lifecycle() == StructuralLifecycle::Active)
    })
}

fn decide_availability_changes(
    state: &AppState,
) -> Result<Vec<ProductionAvailabilityChange>, CompletionPlanError> {
    let current = state.tick();
    let mut changes = Vec::new();
    let mut equipment_jobs = state.production().equipment_occupants().collect::<Vec<_>>();
    equipment_jobs.sort_unstable();
    for job_id in equipment_jobs {
        let job = match state.production().get_job(job_id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: equipment occupancy index references missing production job {}",
                job_id.value()
            ),
        };
        if !job.has_required_active_support() {
            continue;
        }
        let provider = match job.equipment_provider() {
            Some(provider) => provider,
            None => panic!(
                "runtime invariant broken: support-dependent production job {} has no equipment provider",
                job.id().value()
            ),
        };
        let support_active = has_required_active_equipment_support(state, job);
        match job.suspension() {
            None if !support_active => {
                let remaining = job
                    .completes_at()
                    .value()
                    .checked_sub(current.value())
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: running production job {} is already overdue",
                            job.id().value()
                        )
                    });
                assert!(
                    remaining != 0,
                    "runtime invariant broken: running job cannot suspend with zero active time"
                );
                changes.push(ProductionAvailabilityChange::Suspended {
                    job: job.id(),
                    reason: ProductionSuspensionReason::EquipmentSupportUnavailable {
                        equipment: provider.equipment(),
                    },
                    suspended_at: current,
                    remaining_active_time: TickSpan::new(remaining),
                });
            }
            Some(suspension) if support_active => {
                let remaining = suspension.remaining_active_time();
                let Some(scheduled_completion) = current.checked_add_span(remaining) else {
                    return Err(CompletionPlanError::ResumeTickOverflow {
                        job: job.id(),
                        current,
                        remaining,
                    });
                };
                changes.push(ProductionAvailabilityChange::Resumed {
                    job: job.id(),
                    reason: suspension.reason(),
                    resumed_at: current,
                    scheduled_completion,
                });
            }
            None | Some(_) => {}
        }
    }
    Ok(changes)
}

/// Decides provider availability transitions and all jobs due on one exact tick without mutating
/// production, inventory, equipment, energy, or structure.
pub(crate) fn decide_due_completions(
    registries: &Registries,
    state: &AppState,
    tick: SimulationTick,
) -> Result<CompletionPlan, CompletionPlanError> {
    let expected_inventory_revision = state.inventory().revision();
    let expected_production_revision = state.production().revision();
    let expected_equipment_revision = state.equipment().revision();
    let expected_energy_revision = state.energy().revision();
    let expected_structure_revision = state.structures().revision();
    let availability_changes = decide_availability_changes(state)?;
    let mut due_ids = state.production().jobs_due_at(tick);
    for change in &availability_changes {
        match *change {
            ProductionAvailabilityChange::Suspended { job, .. } => {
                due_ids.remove(&job);
            }
            ProductionAvailabilityChange::Resumed {
                job,
                scheduled_completion,
                ..
            } if scheduled_completion == tick => {
                due_ids.insert(job);
            }
            ProductionAvailabilityChange::Resumed { .. } => {}
        }
    }

    let next_inventory_revision = if due_ids.is_empty() {
        expected_inventory_revision
    } else {
        expected_inventory_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::InventoryRevision)?
    };
    let next_production_revision = if due_ids.is_empty() && availability_changes.is_empty() {
        expected_production_revision
    } else {
        expected_production_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::ProductionRevision)?
    };
    let mut next_lot_id = next_material_lot_id(state.inventory());
    let mut entries = Vec::with_capacity(due_ids.len());
    let mut equipment_outcomes = Vec::new();
    let mut released_energy_outcomes = Vec::new();
    let mut deposited_mass_by_destination = BTreeMap::<StockpileId, Mass>::new();
    for job_id in &due_ids {
        let job = match state.production().get_job(*job_id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: due index references missing production job {}",
                job_id.value()
            ),
        };
        let mut output_streams = Vec::with_capacity(job.output_streams().len());
        for stream in job.output_streams() {
            let reserved_mass = match sum_lot_spec_mass(stream.outputs()) {
                Some(mass) => mass,
                None => panic!(
                    "runtime invariant broken: production job {} output stream mass overflows",
                    job_id.value()
                ),
            };
            let mut output_lot_ids = Vec::with_capacity(stream.outputs().len());
            for _ in stream.outputs() {
                output_lot_ids.push(MaterialLotId::new(next_lot_id));
                next_lot_id = next_lot_id
                    .checked_add(1)
                    .ok_or(CompletionPlanError::MaterialLotIds)?;
            }
            output_streams.push(CompletionOutputStreamPlan {
                id: stream.id(),
                destination: stream.destination(),
                outputs: stream.outputs().to_vec(),
                output_lot_ids,
                reserved_mass,
            });
            let current = deposited_mass_by_destination
                .get(&stream.destination())
                .copied()
                .unwrap_or(Mass::ZERO);
            let next = current.checked_add(reserved_mass).ok_or(
                CompletionPlanError::DestinationMassOverflow {
                    stockpile: stream.destination(),
                },
            )?;
            deposited_mass_by_destination.insert(stream.destination(), next);
        }
        entries.push(CompletionPlanEntry {
            job: *job_id,
            process: job.process(),
            output_streams,
        });
        if let (Some(provider), Some(after)) =
            (job.equipment_provider(), job.equipment_condition_after())
            && after != provider.condition()
        {
            let record = match state.equipment().get_equipment(provider.equipment()) {
                Some(record) => record,
                None => panic!(
                    "runtime invariant broken: production job {} references missing equipment {}",
                    job_id.value(),
                    provider.equipment().value()
                ),
            };
            assert_eq!(
                record.definition(),
                provider.definition(),
                "runtime invariant broken: occupied equipment definition changed"
            );
            assert_eq!(
                record.condition(),
                provider.condition(),
                "runtime invariant broken: occupied equipment condition changed"
            );
            equipment_outcomes.push(EquipmentOperationConditionOutcome::new(
                provider.equipment(),
                provider.condition(),
                after,
            ));
        }
        if let Some(released) = job.released_energy() {
            released_energy_outcomes.push(released);
        }
    }

    let next_equipment_revision = if equipment_outcomes.is_empty() {
        expected_equipment_revision
    } else {
        expected_equipment_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EquipmentRevision)?
    };
    let next_energy_revision = if released_energy_outcomes.is_empty() {
        expected_energy_revision
    } else {
        expected_energy_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EnergyRevision)?
    };
    let mut mass_changes = Vec::with_capacity(deposited_mass_by_destination.len());
    for (destination, deposited) in deposited_mass_by_destination {
        let record = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| {
                panic!(
                    "due production destination {} disappeared",
                    destination.value()
                )
            });
        let stored_after = record.stored_mass().checked_add(deposited).ok_or(
            CompletionPlanError::DestinationMassOverflow {
                stockpile: destination,
            },
        )?;
        mass_changes.push(StockpileStoredMassChange::new_committed_inbound(
            destination,
            stored_after,
        ));
    }
    let structural_load = if mass_changes.is_empty() {
        None
    } else {
        validate_stockpile_stored_mass_changes(registries, state, mass_changes)
            .map_err(CompletionPlanError::StructuralLoad)?
    };

    Ok(CompletionPlan {
        tick,
        revisions: CompletionRevisionPlan {
            expected_inventory_revision,
            next_inventory_revision,
            expected_production_revision,
            next_production_revision,
            expected_equipment_revision,
            next_equipment_revision,
            expected_energy_revision,
            next_energy_revision,
            expected_structure_revision,
        },
        next_lot_id_after: next_lot_id,
        availability_changes,
        entries,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    })
}

/// Applies a previously decided due-job plan in stable job-ID order.
pub(crate) fn apply_completion_plan(
    state: &mut AppState,
    plan: CompletionPlan,
) -> Result<CompletionApplication, CompletionCommitError> {
    let CompletionPlan {
        tick,
        revisions:
            CompletionRevisionPlan {
                expected_inventory_revision,
                next_inventory_revision,
                expected_production_revision,
                next_production_revision,
                expected_equipment_revision,
                next_equipment_revision,
                expected_energy_revision,
                next_energy_revision,
                expected_structure_revision,
            },
        next_lot_id_after,
        availability_changes,
        entries,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    } = plan;

    let actual_inventory_revision = state.inventory().revision();
    if actual_inventory_revision != expected_inventory_revision {
        return Err(CompletionCommitError::InventoryStale {
            expected: expected_inventory_revision,
            actual: actual_inventory_revision,
        });
    }
    if !released_energy_outcomes.is_empty() {
        let actual_energy_revision = state.energy().revision();
        if actual_energy_revision != expected_energy_revision {
            return Err(CompletionCommitError::EnergyRevisionConflict {
                expected: expected_energy_revision,
                actual: actual_energy_revision,
            });
        }
    }
    if !equipment_outcomes.is_empty() || !availability_changes.is_empty() {
        let actual_equipment_revision = state.equipment().revision();
        if actual_equipment_revision != expected_equipment_revision {
            return Err(CompletionCommitError::EquipmentRevisionConflict {
                expected: expected_equipment_revision,
                actual: actual_equipment_revision,
            });
        }
    }
    let actual_production_revision = state.production().revision();
    if actual_production_revision != expected_production_revision {
        return Err(CompletionCommitError::ProductionRevisionChanged {
            expected: expected_production_revision,
            actual: actual_production_revision,
        });
    }
    if structural_load.is_some() || !availability_changes.is_empty() {
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != expected_structure_revision {
            return Err(CompletionCommitError::StructureRevisionConflict {
                expected: expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
    }
    if let Some(structural_load) = structural_load {
        debug_assert_eq!(
            structural_load.expected_revision(),
            expected_structure_revision
        );
        structural_load
            .commit(state)
            .map_err(CompletionCommitError::Structure)?;
    }

    for change in &availability_changes {
        match *change {
            ProductionAvailabilityChange::Suspended {
                job,
                reason,
                suspended_at,
                remaining_active_time,
            } => {
                state.production_state_mut().suspend_job(
                    job,
                    suspended_at,
                    remaining_active_time,
                    reason,
                );
            }
            ProductionAvailabilityChange::Resumed {
                job,
                scheduled_completion,
                ..
            } => {
                state
                    .production_state_mut()
                    .resume_job(job, scheduled_completion);
            }
        }
    }

    let mut completions = Vec::with_capacity(entries.len());
    for entry in entries {
        let CompletionPlanEntry {
            job,
            process,
            output_streams,
        } = entry;
        let routes = output_streams
            .iter()
            .map(|stream| ProcessOutputRoute::new(stream.id, stream.destination))
            .collect::<Vec<_>>();
        for stream in &output_streams {
            apply_reserved_deposit(
                state.inventory_state_mut(),
                stream.destination,
                &stream.outputs,
                &stream.output_lot_ids,
                stream.reserved_mass,
                tick,
            );
        }
        let removed = state.production_state_mut().remove_job(job);
        debug_assert_eq!(removed.process(), process);
        debug_assert_eq!(removed.output_streams().len(), output_streams.len());
        for (removed_stream, planned_stream) in removed.output_streams().iter().zip(&output_streams)
        {
            debug_assert_eq!(removed_stream.id(), planned_stream.id);
            debug_assert_eq!(removed_stream.destination(), planned_stream.destination);
            debug_assert_eq!(removed_stream.outputs(), planned_stream.outputs);
        }

        completions.push(ProcessCompletion {
            job,
            process,
            routes,
        });
    }

    if !completions.is_empty() {
        if !equipment_outcomes.is_empty() {
            state
                .equipment_state_mut()
                .apply_operation_condition_outcomes(
                    expected_equipment_revision,
                    next_equipment_revision,
                    &equipment_outcomes,
                );
        }
        if !released_energy_outcomes.is_empty() {
            apply_released_energy_outcomes(
                state.energy_state_mut(),
                expected_energy_revision,
                next_energy_revision,
                &released_energy_outcomes,
            );
        }
        apply_lot_cursor_and_revision(
            state.inventory_state_mut(),
            next_lot_id_after,
            next_inventory_revision,
        );
    }
    if !completions.is_empty() || !availability_changes.is_empty() {
        state
            .production_state_mut()
            .apply_revision(next_production_revision);
    }
    Ok(CompletionApplication {
        completions,
        availability_changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_CONCENTRATE, FORM_LOG, FORM_LUMP, FORM_ORE, MATERIAL_CHARCOAL, MATERIAL_COPPER,
        MATERIAL_SLAG, MATERIAL_WOOD, make_test_registries_with_process,
    };
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::state::{StateValidationError, validate_loaded_state};
    use crate::core::time::WorldSeed;
    use crate::inventory::{
        add_solid_stockpile_for_test, deposit_bulk_for_test, deposit_composed_lot_for_test,
    };
    use crate::material::{
        CommodityKey, CompositionComponent, CompositionConstraint, MaterialComposition,
        MaterialInputSpec, MaterialLotSpec,
    };
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
    use crate::production::{
        ProcessDefinition, ProcessInputError, ProductionValidationError,
        make_test_process_resolution, make_test_process_resolution_with_streams,
        validate_process_inputs,
    };
    use crate::simulation::advance_tick;

    const TEST_PROCESS: ProcessId = ProcessId::new(900_001);
    const TEST_COMPOSITION_PROCESS: ProcessId = ProcessId::new(900_002);

    fn wood_log() -> CommodityKey {
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    }

    fn charcoal_lump() -> CommodityKey {
        CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP)
    }

    fn slag_lump() -> CommodityKey {
        CommodityKey::new(MATERIAL_SLAG, FORM_LUMP)
    }

    fn copper_ore() -> CommodityKey {
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE)
    }

    fn copper_concentrate() -> CommodityKey {
        CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE)
    }

    fn make_copper_slag_composition(copper_parts_per_million: u32) -> MaterialComposition {
        let slag_parts_per_million = 1_000_000_u32 - copper_parts_per_million;
        match MaterialComposition::new(vec![
            CompositionComponent::new(MATERIAL_COPPER, copper_parts_per_million),
            CompositionComponent::new(MATERIAL_SLAG, slag_parts_per_million),
        ]) {
            Ok(composition) => composition,
            Err(error) => panic!("composition fixture failed: {error}"),
        }
    }

    fn minimum_copper_constraint(minimum: u32) -> CompositionConstraint {
        match CompositionConstraint::new(MATERIAL_COPPER, minimum, 1_000_000) {
            Ok(constraint) => constraint,
            Err(error) => panic!("constraint fixture failed: {error}"),
        }
    }

    fn make_test_process() -> ProcessDefinition {
        ProcessDefinition::new(
            TEST_PROCESS,
            "test mass conversion",
            vec![MaterialInputSpec::new(
                wood_log(),
                Mass::from_milligrams(10),
            )],
            Vec::new(),
        )
    }

    fn make_test_multi_stream_resolution(
        registries: &Registries,
        state: &AppState,
        source: StockpileId,
        duration_ticks: u64,
    ) -> ProcessResolution {
        let inputs = match validate_process_inputs(registries, state, TEST_PROCESS, source) {
            Ok(inputs) => inputs,
            Err(error) => panic!("multi-stream input binding failed: {error}"),
        };
        make_test_process_resolution_with_streams(
            inputs,
            duration_ticks,
            vec![
                (
                    ProcessOutputStreamId::new(20),
                    vec![MaterialLotSpec::new(
                        slag_lump(),
                        Mass::from_milligrams(4),
                        Temperature::from_millikelvin(600_000),
                    )],
                ),
                (
                    ProcessOutputStreamId::new(10),
                    vec![MaterialLotSpec::new(
                        charcoal_lump(),
                        Mass::from_milligrams(6),
                        Temperature::from_millikelvin(600_000),
                    )],
                ),
            ],
        )
    }

    fn make_test_registries() -> Registries {
        make_test_registries_with_process(make_test_process())
    }

    fn make_test_resolution(
        registries: &Registries,
        state: &AppState,
        source: StockpileId,
        duration_ticks: u64,
    ) -> ProcessResolution {
        let inputs = match validate_process_inputs(registries, state, TEST_PROCESS, source) {
            Ok(inputs) => inputs,
            Err(error) => panic!("test process input binding failed: {error}"),
        };
        make_test_process_resolution(
            inputs,
            duration_ticks,
            vec![MaterialLotSpec::new(
                charcoal_lump(),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(600_000),
            )],
        )
    }

    fn make_resolution_for_process(
        registries: &Registries,
        state: &AppState,
        source: StockpileId,
        process: ProcessId,
        duration_ticks: u64,
        outputs: Vec<MaterialLotSpec>,
    ) -> ProcessResolution {
        let inputs = match validate_process_inputs(registries, state, process, source) {
            Ok(inputs) => inputs,
            Err(error) => panic!("test process input binding failed: {error}"),
        };
        make_test_process_resolution(inputs, duration_ticks, outputs)
    }

    fn commit_process_for_test(
        token: ValidatedStartProcess,
        state: &mut AppState,
    ) -> ProductionJobId {
        match token.commit(state) {
            Ok(job) => job,
            Err(error) => panic!("validated process commit failed: {error}"),
        }
    }

    fn add_test_stockpile(state: &mut AppState, capacity: u64) -> StockpileId {
        match add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity)) {
            Ok(id) => id,
            Err(error) => panic!("fixture stockpile failed: {error}"),
        }
    }

    fn deposit_test_wood(
        registries: &Registries,
        state: &mut AppState,
        stockpile: StockpileId,
        mass: u64,
    ) {
        if let Err(error) = deposit_bulk_for_test(
            registries,
            state,
            stockpile,
            wood_log(),
            Mass::from_milligrams(mass),
        ) {
            panic!("fixture deposit failed: {error}");
        }
    }

    #[test]
    fn process_consumes_inputs_reserves_capacity_and_completes_on_due_tick() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(10));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 20);
        let resolution = make_test_resolution(&registries, &state, source, 3);

        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("process validation failed: {error}"),
            };
        let job = commit_process_for_test(token, &mut state);

        let source_record = match state.inventory().get_stockpile(source) {
            Some(record) => record,
            None => panic!("source disappeared"),
        };
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        assert_eq!(
            source_record.get_mass(wood_log()),
            Mass::from_milligrams(10)
        );
        assert_eq!(
            destination_record.reserved_inbound(),
            Mass::from_milligrams(10)
        );
        assert_eq!(
            state
                .production()
                .get_job(job)
                .map(ProductionJobRecord::completes_at),
            Some(SimulationTick::new(3))
        );

        for expected_tick in 1..=2 {
            let outcome = match advance_tick(&registries, &mut state) {
                Ok(outcome) => outcome,
                Err(error) => panic!("tick failed: {error}"),
            };
            assert_eq!(outcome.tick(), SimulationTick::new(expected_tick));
            assert!(outcome.production_completions().is_empty());
        }

        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("completion tick failed: {error}"),
        };
        assert_eq!(outcome.production_completions().len(), 1);
        assert_eq!(outcome.production_completions()[0].job(), job);
        assert!(state.production().get_job(job).is_none());
        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
        assert_eq!(
            destination_record.get_mass(charcoal_lump()),
            Mass::from_milligrams(10)
        );
        let output_lots: Vec<_> = destination_record.lot_ids().collect();
        assert_eq!(output_lots.len(), 1);
        let output_lot = match state.inventory().get_lot(output_lots[0]) {
            Some(lot) => lot,
            None => panic!("completed output lot disappeared"),
        };
        assert_eq!(
            output_lot.temperature(),
            Temperature::from_millikelvin(600_000)
        );
        assert_eq!(output_lot.created_at(), SimulationTick::new(3));
    }

    #[test]
    fn routed_output_streams_reserve_and_complete_by_identity_not_route_order() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(10_001));
        let source = add_test_stockpile(&mut state, 20);
        let charcoal_destination = add_test_stockpile(&mut state, 10);
        let slag_destination = add_test_stockpile(&mut state, 10);
        deposit_test_wood(&registries, &mut state, source, 10);
        let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
        assert_eq!(
            resolution
                .output_streams()
                .iter()
                .map(|stream| stream.id())
                .collect::<Vec<_>>(),
            vec![
                ProcessOutputStreamId::new(10),
                ProcessOutputStreamId::new(20)
            ]
        );

        let token = match validate_start_process_routed(
            &registries,
            &state,
            &resolution,
            source,
            &[
                ProcessOutputRoute::new(ProcessOutputStreamId::new(20), slag_destination),
                ProcessOutputRoute::new(ProcessOutputStreamId::new(10), charcoal_destination),
            ],
        ) {
            Ok(token) => token,
            Err(error) => panic!("multi-stream process validation failed: {error}"),
        };
        let job = commit_process_for_test(token, &mut state);

        assert_eq!(
            state
                .inventory()
                .get_stockpile(charcoal_destination)
                .map(|record| record.reserved_inbound()),
            Some(Mass::from_milligrams(6))
        );
        assert_eq!(
            state
                .inventory()
                .get_stockpile(slag_destination)
                .map(|record| record.reserved_inbound()),
            Some(Mass::from_milligrams(4))
        );
        let stored_routes = match state.production().get_job(job) {
            Some(record) => record
                .output_streams()
                .iter()
                .map(|stream| (stream.id(), stream.destination()))
                .collect::<Vec<_>>(),
            None => panic!("multi-stream job disappeared after start"),
        };
        assert_eq!(
            stored_routes,
            vec![
                (ProcessOutputStreamId::new(10), charcoal_destination),
                (ProcessOutputStreamId::new(20), slag_destination),
            ]
        );
        if let Err(error) = validate_loaded_state(&registries, &state) {
            panic!("multi-stream running state failed validation: {error}");
        }
        let encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("multi-stream save serialization failed: {error}"),
        };
        let mut noncanonical = encoded.clone();
        let streams = match noncanonical["state"]["systems"]["production"]["jobs"]
            [job.value().to_string()]["output_streams"]
            .as_array_mut()
        {
            Some(streams) => streams,
            None => panic!("multi-stream save omitted production output streams"),
        };
        streams.reverse();
        let noncanonical: LoadedSaveEnvelope = match serde_json::from_value(noncanonical) {
            Ok(loaded) => loaded,
            Err(error) => {
                panic!("noncanonical multi-stream save failed structural decode: {error}")
            }
        };
        assert_eq!(
            noncanonical.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Production(
                ProductionValidationError::NonCanonicalOutputStreamOrder { job }
            )))
        );

        let loaded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
            Ok(loaded) => loaded,
            Err(error) => panic!("multi-stream save deserialization failed: {error}"),
        };
        let restored = match loaded.into_state(&registries) {
            Ok(restored) => restored,
            Err(error) => panic!("multi-stream save validation failed: {error}"),
        };
        assert_eq!(restored, state);

        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("multi-stream completion tick failed: {error}"),
        };
        assert_eq!(outcome.production_completions().len(), 1);
        assert_eq!(outcome.production_completions()[0].job(), job);
        assert_eq!(
            outcome.production_completions()[0].routes(),
            [
                ProcessOutputRoute::new(ProcessOutputStreamId::new(10), charcoal_destination,),
                ProcessOutputRoute::new(ProcessOutputStreamId::new(20), slag_destination),
            ]
        );
        let charcoal_record = match state.inventory().get_stockpile(charcoal_destination) {
            Some(record) => record,
            None => panic!("charcoal destination disappeared"),
        };
        assert_eq!(charcoal_record.reserved_inbound(), Mass::ZERO);
        assert_eq!(
            charcoal_record.get_mass(charcoal_lump()),
            Mass::from_milligrams(6)
        );
        let slag_record = match state.inventory().get_stockpile(slag_destination) {
            Some(record) => record,
            None => panic!("slag destination disappeared"),
        };
        assert_eq!(slag_record.reserved_inbound(), Mass::ZERO);
        assert_eq!(slag_record.get_mass(slag_lump()), Mass::from_milligrams(4));
    }

    #[test]
    fn duplicate_output_route_is_rejected_atomically() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(10_002));
        let source = add_test_stockpile(&mut state, 20);
        let first_destination = add_test_stockpile(&mut state, 10);
        let second_destination = add_test_stockpile(&mut state, 10);
        deposit_test_wood(&registries, &mut state, source, 10);
        let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
        let before = state.clone();

        let result = validate_start_process_routed(
            &registries,
            &state,
            &resolution,
            source,
            &[
                ProcessOutputRoute::new(ProcessOutputStreamId::new(10), first_destination),
                ProcessOutputRoute::new(ProcessOutputStreamId::new(10), second_destination),
            ],
        );

        assert_eq!(
            result,
            Err(StartProcessError::DuplicateOutputRoute {
                stream: ProcessOutputStreamId::new(10),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn shared_destination_capacity_is_checked_against_aggregate_stream_mass() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(10_003));
        let source = add_test_stockpile(&mut state, 20);
        let destination = add_test_stockpile(&mut state, 9);
        deposit_test_wood(&registries, &mut state, source, 10);
        let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
        let before = state.clone();

        let result = validate_start_process_routed(
            &registries,
            &state,
            &resolution,
            source,
            &[
                ProcessOutputRoute::new(ProcessOutputStreamId::new(10), destination),
                ProcessOutputRoute::new(ProcessOutputStreamId::new(20), destination),
            ],
        );

        assert_eq!(
            result,
            Err(StartProcessError::CapacityExceeded {
                stockpile: destination,
                capacity: Mass::from_milligrams(9),
                committed_after_consumption: Mass::ZERO,
                requested_inbound: Mass::from_milligrams(10),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn failed_process_start_is_atomic() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(11));
        let source = add_test_stockpile(&mut state, 100);
        add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 5);
        let before = state.clone();

        let result = validate_process_inputs(&registries, &state, TEST_PROCESS, source);

        assert!(matches!(
            result,
            Err(ProcessInputError::InsufficientMass { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn resolved_process_cannot_create_or_destroy_unaccounted_matter() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(111));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 10);
        let lossy_resolution = make_resolution_for_process(
            &registries,
            &state,
            source,
            TEST_PROCESS,
            3,
            vec![MaterialLotSpec::new(
                charcoal_lump(),
                Mass::from_milligrams(9),
                Temperature::from_millikelvin(600_000),
            )],
        );
        let before = state.clone();

        let result =
            validate_start_process(&registries, &state, &lossy_resolution, source, destination);

        assert!(matches!(
            result,
            Err(StartProcessError::MatterBalanceMismatch {
                input_mass,
                output_mass,
            }) if input_mass == Mass::from_milligrams(10)
                && output_mass == Mass::from_milligrams(9)
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn reserved_output_capacity_cannot_be_taken_by_later_deposits() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(12));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 12);
        deposit_test_wood(&registries, &mut state, source, 10);
        let resolution = make_test_resolution(&registries, &state, source, 20);
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("process validation failed: {error}"),
            };
        commit_process_for_test(token, &mut state);

        let result = deposit_bulk_for_test(
            &registries,
            &mut state,
            destination,
            wood_log(),
            Mass::from_milligrams(3),
        );

        assert!(matches!(
            result,
            Err(crate::inventory::DepositError::CapacityExceeded { .. })
        ));
    }

    #[test]
    fn same_stockpile_process_accounts_for_consumed_space_before_reserving_output() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(13));
        let stockpile = add_test_stockpile(&mut state, 10);
        deposit_test_wood(&registries, &mut state, stockpile, 10);
        let resolution = make_test_resolution(&registries, &state, stockpile, 2);

        let token =
            match validate_start_process(&registries, &state, &resolution, stockpile, stockpile) {
                Ok(token) => token,
                Err(error) => panic!("same-stockpile process validation failed: {error}"),
            };
        commit_process_for_test(token, &mut state);

        let record = match state.inventory().get_stockpile(stockpile) {
            Some(record) => record,
            None => panic!("stockpile disappeared"),
        };
        assert_eq!(record.stored_mass(), Mass::ZERO);
        assert_eq!(record.reserved_inbound(), Mass::from_milligrams(10));
    }

    #[test]
    fn same_tick_completions_are_emitted_in_stable_job_id_order() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(14));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 20);

        let first_resolution = make_test_resolution(&registries, &state, source, 1);
        let first = match validate_start_process(
            &registries,
            &state,
            &first_resolution,
            source,
            destination,
        ) {
            Ok(token) => commit_process_for_test(token, &mut state),
            Err(error) => panic!("first process validation failed: {error}"),
        };
        let second_resolution = make_test_resolution(&registries, &state, source, 1);
        let second = match validate_start_process(
            &registries,
            &state,
            &second_resolution,
            source,
            destination,
        ) {
            Ok(token) => commit_process_for_test(token, &mut state),
            Err(error) => panic!("second process validation failed: {error}"),
        };

        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("completion tick failed: {error}"),
        };
        let completed: Vec<_> = outcome
            .production_completions()
            .iter()
            .map(|completion| completion.job())
            .collect();
        assert_eq!(completed, vec![first, second]);
    }

    #[test]
    fn compatible_production_outputs_coalesce_and_preserve_provenance_range() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(141));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 20);

        let first_resolution = make_test_resolution(&registries, &state, source, 1);
        let first = match validate_start_process(
            &registries,
            &state,
            &first_resolution,
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("first process validation failed: {error}"),
        };
        commit_process_for_test(first, &mut state);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("first completion failed: {error}");
        }

        let second_resolution = make_test_resolution(&registries, &state, source, 1);
        let second = match validate_start_process(
            &registries,
            &state,
            &second_resolution,
            source,
            destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("second process validation failed: {error}"),
        };
        commit_process_for_test(second, &mut state);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("second completion failed: {error}");
        }

        let record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        let lot_ids: Vec<_> = record.lot_ids().collect();
        assert_eq!(lot_ids.len(), 1);
        let lot = match state.inventory().get_lot(lot_ids[0]) {
            Some(lot) => lot,
            None => panic!("coalesced lot disappeared"),
        };
        assert_eq!(lot.mass(), Mass::from_milligrams(20));
        assert_eq!(lot.created_at(), SimulationTick::new(1));
        assert_eq!(lot.latest_created_at(), SimulationTick::new(2));
    }

    #[test]
    fn resolution_source_mismatch_is_rejected_before_any_start_mutation() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(1415));
        let source = add_test_stockpile(&mut state, 100);
        let other_source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 10);
        deposit_test_wood(&registries, &mut state, other_source, 10);
        let resolution = make_test_resolution(&registries, &state, source, 10);
        let before = state.clone();

        assert_eq!(
            validate_start_process(&registries, &state, &resolution, other_source, destination,),
            Err(StartProcessError::ResolutionSourceMismatch {
                bound: source,
                requested: other_source,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn resolved_inputs_become_stale_after_inventory_changes_before_start_validation() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(1416));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 20);
        let resolution = make_test_resolution(&registries, &state, source, 10);
        let expected_revision = state.inventory().revision();
        add_test_stockpile(&mut state, 1);
        let before = state.clone();

        assert_eq!(
            validate_start_process(&registries, &state, &resolution, source, destination),
            Err(StartProcessError::StaleResolvedInputs {
                expected_inventory_revision: expected_revision,
                actual_inventory_revision: expected_revision + 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn stale_inventory_revision_rejects_validated_process_without_mutation() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(15));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 20);
        let resolution = make_test_resolution(&registries, &state, source, 10);
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("process validation failed: {error}"),
            };

        add_test_stockpile(&mut state, 1);
        let before_commit = state.clone();
        let result = token.commit(&mut state);

        assert!(matches!(
            result,
            Err(StartProcessCommitError::StaleInventoryRevision { .. })
        ));
        assert_eq!(state, before_commit);
    }

    #[test]
    fn stale_production_revision_rejects_second_validated_token_without_mutation() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(16));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 30);
        let resolution = make_test_resolution(&registries, &state, source, 10);
        let stale =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("first process validation failed: {error}"),
            };
        let winner =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("second process validation failed: {error}"),
            };
        commit_process_for_test(winner, &mut state);
        let before_stale_commit = state.clone();

        let result = stale.commit(&mut state);

        assert!(matches!(
            result,
            Err(StartProcessCommitError::StaleProductionRevision { .. })
        ));
        assert_eq!(state, before_stale_commit);
    }

    #[test]
    fn in_flight_job_uses_committed_output_snapshot_after_later_resolution_differs() {
        let registries = make_test_registries();
        let mut state = AppState::new(WorldSeed::new(17));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        deposit_test_wood(&registries, &mut state, source, 10);
        let resolution = make_test_resolution(&registries, &state, source, 1);
        let later_resolution = make_resolution_for_process(
            &registries,
            &state,
            source,
            TEST_PROCESS,
            1,
            vec![
                MaterialLotSpec::new(
                    charcoal_lump(),
                    Mass::from_milligrams(1),
                    Temperature::from_millikelvin(900_000),
                ),
                MaterialLotSpec::new(
                    slag_lump(),
                    Mass::from_milligrams(9),
                    Temperature::from_millikelvin(900_000),
                ),
            ],
        );
        assert_ne!(resolution.outputs(), later_resolution.outputs());
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("original process validation failed: {error}"),
            };
        commit_process_for_test(token, &mut state);

        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion after later resolution change failed: {error}");
        }

        let destination_record = match state.inventory().get_stockpile(destination) {
            Some(record) => record,
            None => panic!("destination disappeared"),
        };
        assert_eq!(
            destination_record.get_mass(charcoal_lump()),
            Mass::from_milligrams(10)
        );
        let lot_id = match destination_record.lot_ids().next() {
            Some(id) => id,
            None => panic!("committed output lot is missing"),
        };
        let lot = match state.inventory().get_lot(lot_id) {
            Some(lot) => lot,
            None => panic!("committed output lot record is missing"),
        };
        assert_eq!(lot.temperature(), Temperature::from_millikelvin(600_000));
    }

    #[test]
    fn composition_constrained_process_consumes_only_eligible_lots() {
        let input = match MaterialInputSpec::with_constraints(
            copper_ore(),
            Mass::from_milligrams(10),
            vec![minimum_copper_constraint(800_000)],
        ) {
            Ok(input) => input,
            Err(error) => panic!("input fixture failed: {error}"),
        };
        let process = ProcessDefinition::new(
            TEST_COMPOSITION_PROCESS,
            "test concentration",
            vec![input],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(18));
        let source = add_test_stockpile(&mut state, 100);
        let destination = add_test_stockpile(&mut state, 100);
        let poor = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            copper_ore(),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(300_000),
            make_copper_slag_composition(600_000),
        ) {
            Ok(id) => id,
            Err(error) => panic!("poor ore fixture failed: {error}"),
        };

        let poor_only =
            validate_process_inputs(&registries, &state, TEST_COMPOSITION_PROCESS, source);
        match poor_only {
            Err(ProcessInputError::InsufficientMass { available, .. }) => {
                assert_eq!(available, Mass::ZERO);
            }
            Err(error) => panic!("unexpected composition validation error: {error}"),
            Ok(_) => panic!("poor ore incorrectly satisfied rich-ore input constraint"),
        }

        let rich = match deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            copper_ore(),
            Mass::from_milligrams(11),
            Temperature::from_millikelvin(300_000),
            make_copper_slag_composition(900_000),
        ) {
            Ok(id) => id,
            Err(error) => panic!("rich ore fixture failed: {error}"),
        };
        let resolution = make_resolution_for_process(
            &registries,
            &state,
            source,
            TEST_COMPOSITION_PROCESS,
            5,
            vec![
                MaterialLotSpec::new(
                    copper_concentrate(),
                    Mass::from_milligrams(8),
                    Temperature::from_millikelvin(350_000),
                ),
                MaterialLotSpec::new(
                    slag_lump(),
                    Mass::from_milligrams(2),
                    Temperature::from_millikelvin(350_000),
                ),
            ],
        );
        let token =
            match validate_start_process(&registries, &state, &resolution, source, destination) {
                Ok(token) => token,
                Err(error) => panic!("rich ore should satisfy process: {error}"),
            };
        commit_process_for_test(token, &mut state);

        let poor_lot = match state.inventory().get_lot(poor) {
            Some(lot) => lot,
            None => panic!("poor ore lot disappeared"),
        };
        let rich_lot = match state.inventory().get_lot(rich) {
            Some(lot) => lot,
            None => panic!("rich ore lot disappeared"),
        };
        assert_eq!(poor_lot.mass(), Mass::from_milligrams(20));
        assert_eq!(rich_lot.mass(), Mass::from_milligrams(1));
    }

    #[test]
    fn overlapping_composition_inputs_cannot_double_count_one_lot() {
        let first = match MaterialInputSpec::with_constraints(
            copper_ore(),
            Mass::from_milligrams(6),
            vec![minimum_copper_constraint(800_000)],
        ) {
            Ok(input) => input,
            Err(error) => panic!("first input fixture failed: {error}"),
        };
        let second = match MaterialInputSpec::with_constraints(
            copper_ore(),
            Mass::from_milligrams(6),
            vec![minimum_copper_constraint(850_000)],
        ) {
            Ok(input) => input,
            Err(error) => panic!("second input fixture failed: {error}"),
        };
        let process = ProcessDefinition::new(
            TEST_COMPOSITION_PROCESS,
            "overlapping composition selection",
            vec![first, second],
            Vec::new(),
        );
        let registries = make_test_registries_with_process(process);
        let mut state = AppState::new(WorldSeed::new(19));
        let source = add_test_stockpile(&mut state, 100);
        add_test_stockpile(&mut state, 100);
        if let Err(error) = deposit_composed_lot_for_test(
            &registries,
            &mut state,
            source,
            copper_ore(),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
            make_copper_slag_composition(900_000),
        ) {
            panic!("overlap lot fixture failed: {error}");
        }

        let result = validate_process_inputs(&registries, &state, TEST_COMPOSITION_PROCESS, source);

        match result {
            Err(ProcessInputError::InsufficientMass {
                available,
                requested,
                ..
            }) => {
                assert_eq!(available, Mass::from_milligrams(4));
                assert_eq!(requested, Mass::from_milligrams(6));
            }
            Err(error) => panic!("unexpected overlap validation error: {error}"),
            Ok(_) => panic!("overlapping inputs double-counted one material lot"),
        }
    }
}
