//! Resource, schedule, and occupancy admission for one resolved production start.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::{
    ConsumedEnergyTrace, EnergyConsumptionReservation, EnergyIngressReservation,
    EnergyIngressReservationError, EnergyReservationError, ReleasedEnergyTrace,
    validate_energy_consumption_reservation, validate_energy_ingress_reservation,
};
use crate::equipment::{EquipmentOperationTrace, ValidatedEquipmentUse};
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ConsumptionReservation, MaterialStorageHistory,
    ReservationError, StockpileId, StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_consumption_reservation_from_selection, validate_stockpile_stored_mass_changes,
};
use crate::production::resolution::ProcessResolution;
use crate::production::state::ProductionJobId;
use crate::registry::Registries;
use crate::structural::StructuralLifecycle;

use super::StartProcessError;

#[must_use]
pub(super) struct ValidatedJobAllocation {
    pub(super) current: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) job_id: ProductionJobId,
    pub(super) next_job_id: u64,
    pub(super) expected_production_revision: u64,
    pub(super) next_production_revision: u64,
}

pub(super) fn validate_job_allocation(
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<ValidatedJobAllocation, StartProcessError> {
    let current = state.tick();
    let completes_at = current.checked_add_span(resolution.duration()).ok_or(
        StartProcessError::CompletionTickOverflow {
            current,
            duration_ticks: resolution.duration().value(),
        },
    )?;
    let job_value = state.production().next_job_id();
    let next_job_id = job_value
        .checked_add(1)
        .ok_or(StartProcessError::JobIdExhausted)?;
    let expected_production_revision = state.production().revision();
    let next_production_revision = expected_production_revision
        .checked_add(1)
        .ok_or(StartProcessError::ProductionRevisionExhausted)?;
    Ok(ValidatedJobAllocation {
        current,
        completes_at,
        job_id: ProductionJobId::new(job_value),
        next_job_id,
        expected_production_revision,
        next_production_revision,
    })
}

#[must_use]
pub(super) struct ValidatedMaterialReservation {
    pub(super) reservation: ConsumptionReservation,
    pub(super) storage_history: MaterialStorageHistory,
}

pub(super) fn validate_material_reservation(
    state: &AppState,
    resolution: &ProcessResolution,
    inbound_by_destination: BTreeMap<StockpileId, Mass>,
    completes_at: SimulationTick,
) -> Result<ValidatedMaterialReservation, StartProcessError> {
    let reservation = validate_consumption_reservation_from_selection(
        state.inventory(),
        resolution.selection().clone(),
        inbound_by_destination,
    )
    .map_err(map_reservation_error)?;
    let source = resolution.source();
    let storage_history = reservation
        .oldest_storage_history_at(state.inventory(), state.tick())
        .ok_or(StartProcessError::InputStorageAgeOverflow { stockpile: source })?;
    if storage_history
        .project(completes_at, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .is_none()
    {
        return Err(StartProcessError::InputStorageAgeOverflow { stockpile: source });
    }
    Ok(ValidatedMaterialReservation {
        reservation,
        storage_history,
    })
}

#[must_use]
pub(super) struct ValidatedEnergyReservations {
    pub(super) consumption: Option<EnergyConsumptionReservation>,
    pub(super) ingress: Option<EnergyIngressReservation>,
    pub(super) consumed: Option<ConsumedEnergyTrace>,
    pub(super) released: Option<ReleasedEnergyTrace>,
}

pub(super) fn validate_energy_reservations(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<ValidatedEnergyReservations, StartProcessError> {
    let consumption = match resolution.energy_supply() {
        Some(selection) => Some(
            validate_energy_consumption_reservation(state.energy(), selection)
                .map_err(map_energy_reservation_error)?,
        ),
        None => None,
    };
    let ingress = match resolution.energy_sink() {
        Some(selection) => Some(
            validate_energy_ingress_reservation(
                registries,
                state.energy(),
                selection,
                resolution.duration(),
            )
            .map_err(map_energy_ingress_reservation_error)?,
        ),
        None => None,
    };
    let consumed = consumption
        .as_ref()
        .map(EnergyConsumptionReservation::trace);
    let released = ingress.map(EnergyIngressReservation::trace);
    for store in consumed
        .map(|trace| trace.source())
        .into_iter()
        .chain(released.map(|trace| trace.destination()))
    {
        validate_energy_store_available(state, store)?;
    }
    Ok(ValidatedEnergyReservations {
        consumption,
        ingress,
        consumed,
        released,
    })
}

fn validate_energy_store_available(
    state: &AppState,
    store: crate::energy::EnergyStoreId,
) -> Result<(), StartProcessError> {
    if let Some(job_id) = state.production().get_energy_occupant(store) {
        let job = state.production().get_job(job_id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: energy occupancy index references missing production job {}",
                job_id.value()
            )
        });
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
    Ok(())
}

#[must_use]
pub(super) struct ValidatedEquipmentResources {
    pub(super) selection: Option<ValidatedEquipmentUse>,
    pub(super) provider: Option<EquipmentOperationTrace>,
}

pub(super) fn validate_equipment_resources(
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<ValidatedEquipmentResources, StartProcessError> {
    let Some(selection) = resolution.equipment_use() else {
        return Ok(ValidatedEquipmentResources {
            selection: None,
            provider: None,
        });
    };
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
    validate_equipment_support(state, selection, record.supported_by())?;
    validate_equipment_available(state, trace.equipment())?;
    Ok(ValidatedEquipmentResources {
        selection: Some(selection),
        provider: Some(trace),
    })
}

fn validate_equipment_support(
    state: &AppState,
    selection: ValidatedEquipmentUse,
    actual_support: Option<crate::structural::StructuralElementId>,
) -> Result<(), StartProcessError> {
    let equipment = selection.trace().equipment();
    let expected_support = selection.support();
    if actual_support != expected_support {
        return Err(StartProcessError::ResolvedEquipmentSupportChanged {
            equipment,
            expected: expected_support,
            actual: actual_support,
        });
    }
    let Some(expected_structure_revision) = selection.expected_structure_revision() else {
        return Ok(());
    };
    let actual_structure_revision = state.structures().revision();
    if actual_structure_revision != expected_structure_revision {
        return Err(StartProcessError::StaleResolvedStructure {
            expected_structure_revision,
            actual_structure_revision,
        });
    }
    let element = expected_support.unwrap_or_else(|| {
        panic!("validated equipment use has structural revision without a support element")
    });
    let Some(support) = state.structures().get_element(element) else {
        return Err(StartProcessError::ResolvedEquipmentSupportMissing { equipment, element });
    };
    if support.lifecycle() != StructuralLifecycle::Active {
        return Err(StartProcessError::ResolvedEquipmentSupportNotActive {
            equipment,
            element,
            lifecycle: support.lifecycle(),
        });
    }
    Ok(())
}

fn validate_equipment_available(
    state: &AppState,
    equipment: crate::equipment::EquipmentId,
) -> Result<(), StartProcessError> {
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(StartProcessError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(StartProcessError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(StartProcessError::EquipmentBusyManualPower { equipment });
    }
    if let Some(work) = state
        .player_work()
        .get_prospecting_equipment_occupant(equipment)
    {
        return Err(StartProcessError::EquipmentBusyProspecting {
            equipment,
            completes_at: work.completes_at(),
        });
    }
    Ok(())
}

pub(super) fn validate_source_structural_load(
    registries: &Registries,
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<Option<ValidatedStockpileStructuralLoad>, StartProcessError> {
    let source = resolution.source();
    let source_record = state
        .inventory()
        .get_stockpile(source)
        .ok_or(StartProcessError::UnknownStockpile { stockpile: source })?;
    let source_after = source_record
        .stored_mass()
        .checked_sub(resolution.input_mass())
        .ok_or(StartProcessError::MassOverflow { stockpile: source })?;
    validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(source, source_after)],
    )
    .map_err(StartProcessError::StructuralLoad)
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
        EnergyIngressReservationError::CapacityOverflow { store: _ }
        | EnergyIngressReservationError::InsufficientCapacity {
            store: _,
            stored: _,
            requested: _,
            capacity: _,
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
