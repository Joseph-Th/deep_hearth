//! Trusted-load validation for production process, energy, equipment, and topology bindings.

use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergySinkCapacityError, EnergyValidationError, validate_energy_sink_capacity_at_release,
};
use crate::production::ProductionJobRecord;
use crate::registry::{ProcessEnergyRole, ProcessEquipmentRole, Registries};

use super::super::StateValidationError;

pub(super) fn validate_job_resource_topology(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    let topology = registries.process_topology(job.process()).ok_or(
        StateValidationError::MissingJobProcessTopology {
            job: job.id(),
            process: job.process(),
        },
    )?;
    validate_job_energy_topology(job, topology)?;
    validate_job_equipment_topology(job, topology)
}

fn validate_job_energy_topology(
    job: &ProductionJobRecord,
    topology: &crate::registry::ProcessTopology,
) -> Result<(), StateValidationError> {
    let energy_matches = match topology.energy_role() {
        ProcessEnergyRole::None => {
            job.consumed_energy().is_none() && job.released_energy().is_none()
        }
        ProcessEnergyRole::Supply(carrier) => {
            job.released_energy().is_none()
                && job.consumed_energy().is_some_and(|trace| {
                    trace.carrier() == carrier
                        && topology
                            .compatible_energy_stores()
                            .contains(&trace.definition())
                })
        }
        ProcessEnergyRole::Sink(carrier) => {
            job.consumed_energy().is_none()
                && job.released_energy().is_some_and(|trace| {
                    trace.carrier() == carrier
                        && topology
                            .compatible_energy_stores()
                            .contains(&trace.definition())
                })
        }
    };
    if !energy_matches {
        return Err(StateValidationError::JobEnergyTopologyMismatch {
            job: job.id(),
            process: job.process(),
        });
    }
    Ok(())
}

fn validate_job_equipment_topology(
    job: &ProductionJobRecord,
    topology: &crate::registry::ProcessTopology,
) -> Result<(), StateValidationError> {
    let equipment_matches = match topology.equipment_role() {
        ProcessEquipmentRole::None => job.equipment_provider().is_none(),
        ProcessEquipmentRole::Optional => job.equipment_provider().is_none_or(|provider| {
            topology
                .nominal_providers()
                .contains(&provider.definition())
        }),
        ProcessEquipmentRole::Required => job.equipment_provider().is_some_and(|provider| {
            topology
                .nominal_providers()
                .contains(&provider.definition())
        }),
    };
    if !equipment_matches {
        return Err(StateValidationError::JobEquipmentTopologyMismatch {
            job: job.id(),
            process: job.process(),
        });
    }
    Ok(())
}

pub(super) fn validate_job_process_and_source(
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

pub(super) fn validate_job_consumed_energy(
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

pub(super) fn validate_job_released_energy(
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
    validate_energy_sink_capacity_at_release(
        registries,
        store.definition(),
        store.stored(),
        trace.energy(),
        release_after,
    )
    .map_err(|error| match error {
        EnergySinkCapacityError::Overflow => {
            StateValidationError::JobReleasedEnergyCapacityOverflow {
                job: job.id(),
                store: trace.destination(),
            }
        }
        EnergySinkCapacityError::Insufficient {
            stored,
            requested,
            capacity,
        } => StateValidationError::JobReleasedEnergyCapacityExceeded {
            job: job.id(),
            store: trace.destination(),
            stored,
            released: requested,
            capacity,
        },
    })?;
    Ok(())
}

pub(super) fn validate_job_equipment(
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
