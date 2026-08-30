//! Validates production-job references, physical traces, output custody, and runtime resources.

use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::crafting::validate_loaded_manual_craft_job;
use crate::energy::{EnergyValidationError, project_energy_sink_stored_at_release};
use crate::inventory::{StockpileId, validate_stockpile_storage};
use crate::material::{validate_material_particle_size_state, validate_material_phase_state};
use crate::ore_processing::{
    validate_loaded_comminution_job, validate_loaded_constituent_separation_job,
    validate_loaded_screening_job,
};
use crate::production::{
    ProductionJobRecord, ProductionOutputStream, ProductionSuspensionReason, sum_lot_spec_mass,
};
use crate::registry::Registries;
use crate::thermal::validate_loaded_thermal_job;

use super::StateValidationError;
use super::reservations::ExpectedReservations;

pub(super) fn validate_production_references(
    registries: &Registries,
    state: &AppState,
) -> Result<ExpectedReservations, StateValidationError> {
    let mut expected_reservations = ExpectedReservations::default();
    for job in state.systems.production.jobs() {
        validate_production_job(registries, state, job, &mut expected_reservations)?;
    }
    Ok(expected_reservations)
}

fn validate_production_job(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    validate_job_process_and_source(registries, state, job)?;
    validate_job_consumed_energy(registries, state, job)?;
    validate_job_released_energy(registries, state, job)?;
    validate_job_equipment(registries, state, job)?;
    validate_job_subsystem_contracts(registries, job)?;
    validate_job_consumed_inputs(registries, job)?;
    validate_job_outputs(registries, state, job, expected_reservations)
}

fn validate_job_process_and_source(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
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
    Ok(())
}

fn validate_job_consumed_energy(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    let Some(trace) = job.consumed_energy() else {
        return Ok(());
    };
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
    Ok(())
}

fn validate_job_released_energy(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    let Some(trace) = job.released_energy() else {
        return Ok(());
    };
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
    let release_after = job.suspension().map_or_else(
        || {
            TickSpan::new(
                job.completes_at()
                    .value()
                    .checked_sub(state.tick().value())
                    .unwrap_or_else(|| {
                        unreachable!("production schedule was validated before energy release")
                    }),
            )
        },
        |suspension| suspension.remaining_active_time(),
    );
    let projected_stored = project_energy_sink_stored_at_release(
        registries,
        store.definition(),
        store.stored(),
        release_after,
    );
    let after = projected_stored.checked_add(trace.energy()).ok_or(
        StateValidationError::JobReleasedEnergyCapacityOverflow {
            job: job.id(),
            store: trace.destination(),
        },
    )?;
    if after > definition.capacity() {
        return Err(StateValidationError::JobReleasedEnergyCapacityExceeded {
            job: job.id(),
            store: trace.destination(),
            stored: projected_stored,
            released: trace.energy(),
            capacity: definition.capacity(),
        });
    }
    Ok(())
}

fn validate_job_equipment(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    let Some(provider) = job.equipment_provider() else {
        return Ok(());
    };
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
    let definition = registries
        .equipment()
        .get_equipment(record.definition())
        .unwrap_or_else(|| {
            unreachable!("validated equipment record definition disappeared from registry")
        });
    if definition.requires_structural_support() && !job.has_required_active_support() {
        return Err(
            StateValidationError::JobEquipmentSupportRequirementMissing {
                job: job.id(),
                equipment: provider.equipment(),
                definition: record.definition(),
            },
        );
    }
    if !job.is_suspended() && job.has_required_active_support() != record.supported_by().is_some() {
        return Err(StateValidationError::JobEquipmentSupportStateMismatch {
            job: job.id(),
            equipment: provider.equipment(),
            requires_active_support: job.has_required_active_support(),
            supported_by: record.supported_by(),
        });
    }
    Ok(())
}

fn validate_job_subsystem_contracts(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    if job.suspension().is_some_and(|suspension| {
        suspension.reason() == ProductionSuspensionReason::PlayerLaborUnavailable
    }) && registries.manual_process_exertion(job.process()).is_none()
    {
        return Err(StateValidationError::NonManualJobSuspendedForPlayerLabor {
            job: job.id(),
            process: job.process(),
        });
    }
    validate_loaded_comminution_job(registries, job)
        .map_err(StateValidationError::ComminutionJob)?;
    validate_loaded_constituent_separation_job(registries, job)
        .map_err(StateValidationError::ConstituentSeparationJob)?;
    validate_loaded_screening_job(registries, job).map_err(StateValidationError::ScreeningJob)?;
    validate_loaded_thermal_job(registries, job).map_err(StateValidationError::ThermalJob)?;
    validate_loaded_manual_craft_job(registries, job).map_err(StateValidationError::ManualCraftJob)
}

fn validate_job_consumed_inputs(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
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
        .map_err(
            |error| StateValidationError::InvalidJobConsumedParticleSizeState {
                job: job.id(),
                error,
            },
        )?;
        validate_material_phase_state(
            registries.materials(),
            commodity,
            trace.profile().composition(),
            trace.profile().temperature(),
        )
        .map_err(|error| StateValidationError::InvalidJobConsumedPhaseState {
            job: job.id(),
            error,
        })?;
    }
    Ok(())
}

fn validate_job_outputs(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    for stream in job.output_streams() {
        validate_job_output_stream(registries, state, job, stream, expected_reservations)?;
    }
    Ok(())
}

fn validate_job_output_stream(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    stream: &ProductionOutputStream,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    let destination = stream.destination();
    let Some(destination_record) = state.systems.inventory.get_stockpile(destination) else {
        return Err(StateValidationError::UnknownJobDestination {
            job: job.id(),
            stockpile: destination,
        });
    };
    for output in stream.outputs() {
        validate_job_output(registries, job, destination_record, destination, output)?;
    }
    let output_mass = sum_lot_spec_mass(stream.outputs())
        .ok_or(StateValidationError::JobOutputMassOverflow { job: job.id() })?;
    expected_reservations.add(destination, output_mass)
}

fn validate_job_output(
    registries: &Registries,
    job: &ProductionJobRecord,
    destination_record: &crate::inventory::StockpileRecord,
    destination: StockpileId,
    output: &crate::material::MaterialLotSpec,
) -> Result<(), StateValidationError> {
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
    })
}
