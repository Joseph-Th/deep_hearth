//! Process-start admission and routing; child commit owns atomic mutation after validation.

mod admission;
mod commit;
mod routing;
pub use commit::StartProcessCommitError;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{EnergyConsumptionReservation, EnergyIngressReservation};
use crate::equipment::{EquipmentId, ValidatedEquipmentUse};
use crate::inventory::{
    ConsumptionReservation, StockpileId, StockpileStorageError, StockpileStructuralLoadError,
    ValidatedStockpileStructuralLoad,
};
use crate::material::{FormId, MaterialId};
use crate::mining::MiningJobId;
use crate::registry::Registries;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::definitions::ProcessId;
use super::super::resolution::{ProcessOutputStreamId, ProcessResolution};
use super::super::state::{
    ProductionJobEquipment, ProductionJobId, ProductionJobIdentity, ProductionJobRecord,
    ProductionJobResources, ProductionJobSchedule, ProductionOccupancyRelease,
};
use admission::{
    ValidatedEnergyReservations, ValidatedEquipmentResources, ValidatedJobAllocation,
    ValidatedMaterialReservation, validate_energy_reservations, validate_equipment_resources,
    validate_job_allocation, validate_material_reservation, validate_source_structural_load,
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
    let ValidatedJobAllocation {
        current,
        completes_at,
        job_id,
        next_job_id,
        expected_production_revision,
        next_production_revision,
    } = validate_job_allocation(state, resolution)?;
    let ValidatedMaterialReservation {
        input_mass,
        reservation,
        storage_history: material_storage_history,
    } = validate_material_reservation(state, resolution, inbound_by_destination, completes_at)?;
    let consumed_inputs = reservation.consumed_inputs().to_vec();
    let ValidatedEnergyReservations {
        consumption: energy_reservation,
        ingress: energy_ingress_reservation,
        consumed: consumed_energy,
        released: released_energy,
    } = validate_energy_reservations(registries, state, resolution)?;
    let ValidatedEquipmentResources {
        selection: equipment_use,
        provider: equipment_provider,
    } = validate_equipment_resources(state, resolution)?;
    let structural_load = validate_source_structural_load(registries, state, resolution)?;

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
        next_job_id,
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
