//! Owns process-start admission, resource routing, and atomic commit.

mod admission;
mod commit;
mod errors;
mod routing;
pub use commit::StartProcessCommitError;
pub use errors::StartProcessError;

use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{EnergyConsumptionReservation, EnergyIngressReservation};
use crate::equipment::ValidatedEquipmentUse;
use crate::inventory::{ConsumptionReservation, StockpileId, ValidatedStockpileStructuralLoad};
use crate::registry::Registries;

use super::super::resolution::{ProcessOutputStreamId, ProcessResolution};
use super::super::state::{
    ProductionJobEquipment, ProductionJobIdentity, ProductionJobRecord, ProductionJobResources,
    ProductionJobSchedule,
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

/// Consumed proof that process references, matter, capacity, time, and job identity are valid.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
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

pub(crate) fn validate_start_manual_process_routed(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
    source: StockpileId,
    routes: &[ProcessOutputRoute],
) -> Result<ValidatedStartProcess, StartProcessError> {
    validate_start_process_routed_internal(registries, state, resolution, source, routes, true)
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
    allow_player_labor: bool,
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
    if !allow_player_labor && registries.manual_process_exertion(process).is_some() {
        return Err(StartProcessError::ManualProcessRequiresPlayerWork { process });
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
                completed_suspension_time: TickSpan::ZERO,
                suspension: None,
            },
            resources: ProductionJobResources {
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
