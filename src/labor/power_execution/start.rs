//! Admission and commit for direct player-powered generation.

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::Power;
use crate::core::state::AppState;
use crate::energy::{
    EnergySinkError, EnergyStoreOccupancy, energy_store_occupancy, validate_energy_sink_access,
    validate_energy_sink_release,
};
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, ResolvedEquipmentProvider, equipment_occupancy,
    resolve_equipment_provider,
};
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;

use super::super::power_physics::{
    ManualPowerMetabolicDurationError, ManualPowerScheduleError, resolve_manual_power_schedule,
};
use super::super::{
    ManualPowerWork, PlayerWork, PlayerWorkResourceBudget, ValidatedPlayerWorkStart,
    validate_player_work_start,
};
use super::{ManualPowerCommitError, ManualPowerError, ManualPowerRequest};

/// Revision-bound admission token for direct player-powered generation.
#[must_use]
pub struct ValidatedManualPowerStart {
    work_start: ValidatedPlayerWorkStart,
    work: ManualPowerWork,
    resource_budget: PlayerWorkResourceBudget,
    expected_equipment_revision: u64,
    expected_energy_revision: u64,
}

impl ValidatedManualPowerStart {
    #[must_use]
    pub const fn work(&self) -> ManualPowerWork {
        self.work
    }

    /// Returns the authoritative survival reserve consumed if this work runs to completion.
    #[must_use]
    pub const fn resource_budget(&self) -> PlayerWorkResourceBudget {
        self.resource_budget
    }

    pub fn commit(self, state: &mut AppState) -> Result<ManualPowerWork, ManualPowerCommitError> {
        self.work_start
            .precheck(state)
            .map_err(ManualPowerCommitError::Work)?;
        if state.equipment().revision() != self.expected_equipment_revision {
            return Err(ManualPowerCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: state.equipment().revision(),
            });
        }
        if state.energy().revision() != self.expected_energy_revision {
            return Err(ManualPowerCommitError::StaleEnergyRevision {
                expected: self.expected_energy_revision,
                actual: state.energy().revision(),
            });
        }
        let equipment = self.work.equipment();
        match equipment_occupancy(state, equipment) {
            Some(EquipmentOccupancy::Production { job, .. }) => {
                return Err(ManualPowerCommitError::EquipmentBusyProduction { equipment, job });
            }
            Some(EquipmentOccupancy::Mining { job }) => {
                return Err(ManualPowerCommitError::EquipmentBusyMining { equipment, job });
            }
            Some(
                EquipmentOccupancy::ManualPower { .. }
                | EquipmentOccupancy::Prospecting { .. }
                | EquipmentOccupancy::Maintenance { .. },
            )
            | None => {}
        }
        if let Some(EnergyStoreOccupancy::Production { job, .. }) =
            energy_store_occupancy(state, self.work.destination())
        {
            return Err(ManualPowerCommitError::EnergyBusyProduction {
                store: self.work.destination(),
                job,
            });
        }
        self.work_start.apply(state);
        Ok(self.work)
    }
}

fn validate_manual_power_equipment_occupancy(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), ManualPowerError> {
    match equipment_occupancy(state, equipment) {
        Some(EquipmentOccupancy::Production { job, release }) => {
            Err(ManualPowerError::EquipmentBusyProduction {
                equipment,
                job,
                release,
            })
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            Err(ManualPowerError::EquipmentBusyMining { equipment, job })
        }
        Some(
            EquipmentOccupancy::ManualPower { .. }
            | EquipmentOccupancy::Prospecting { .. }
            | EquipmentOccupancy::Maintenance { .. },
        )
        | None => Ok(()),
    }
}

fn resolve_manual_power_equipment_power(
    provider: ResolvedEquipmentProvider<'_>,
    equipment: EquipmentId,
    capability: CapabilityId,
) -> Result<Power, ManualPowerError> {
    let value =
        provider
            .get_capability(capability)
            .ok_or(ManualPowerError::MissingPowerCapability {
                equipment,
                capability,
            })?;
    let CapabilityValue::Power(power) = value else {
        return Err(ManualPowerError::PowerCapabilityKindMismatch {
            equipment,
            capability,
            found: value.kind(),
        });
    };
    if power.is_zero() {
        return Err(ManualPowerError::ZeroEquipmentPower {
            equipment,
            capability,
        });
    }
    Ok(power)
}

fn map_manual_power_schedule_error(
    request: ManualPowerRequest,
    transfer_power: Power,
    error: ManualPowerScheduleError,
) -> ManualPowerError {
    match error {
        ManualPowerScheduleError::PowerDuration(_error) => ManualPowerError::PowerDuration {
            energy: request.energy,
            power: transfer_power,
        },
        ManualPowerScheduleError::MetabolicDuration(error) => match error {
            ManualPowerMetabolicDurationError::ZeroOutput => {
                ManualPowerError::MetabolicConversionTooSmall {
                    method: request.method,
                }
            }
            ManualPowerMetabolicDurationError::DurationOverflow => {
                ManualPowerError::MetabolicDurationOverflow {
                    method: request.method,
                    energy: request.energy,
                }
            }
        },
        ManualPowerScheduleError::Exertion(_error) => ManualPowerError::ExertionResolution {
            method: request.method,
        },
    }
}

/// Resolves and admits a direct player-power work order without creating energy before work finishes.
pub fn validate_start_manual_power(
    registries: &Registries,
    state: &AppState,
    request: ManualPowerRequest,
) -> Result<ValidatedManualPowerStart, ManualPowerError> {
    let definition = registries
        .labor()
        .get_manual_power(request.method)
        .copied()
        .ok_or(ManualPowerError::UnknownMethod {
            method: request.method,
        })?;
    if state
        .equipment()
        .get_equipment(request.equipment)
        .is_some_and(|equipment| equipment.supported_by().is_some())
    {
        return Err(ManualPowerError::EquipmentMounted {
            equipment: request.equipment,
        });
    }
    let provider = resolve_equipment_provider(registries, state, request.equipment)
        .map_err(ManualPowerError::Equipment)?;
    validate_manual_power_equipment_occupancy(state, request.equipment)?;
    let equipment_power = resolve_manual_power_equipment_power(
        provider,
        request.equipment,
        definition.power_capability(),
    )?;
    if request.energy.is_zero() {
        return Err(ManualPowerError::EnergySink(EnergySinkError::ZeroEnergy));
    }
    let sink_access = validate_energy_sink_access(registries, state, request.destination)
        .map_err(ManualPowerError::EnergySink)?;
    if sink_access.carrier() != definition.carrier() {
        return Err(ManualPowerError::WrongCarrier {
            required: definition.carrier(),
            provided: sink_access.carrier(),
        });
    }
    let transfer_power = std::cmp::min(equipment_power, sink_access.max_input_power());
    if transfer_power.is_zero() {
        return Err(ManualPowerError::ZeroTransferPower {
            equipment: request.equipment,
            destination: request.destination,
        });
    }
    let schedule = resolve_manual_power_schedule(
        request.energy,
        transfer_power,
        registries.core().physical_tick_duration(),
        definition.maximum_exertion(),
        definition.metabolic_efficiency_ppm(),
    )
    .map_err(|error| map_manual_power_schedule_error(request, transfer_power, error))?;
    let duration = schedule.duration();
    let sink = validate_energy_sink_release(registries, sink_access, request.energy, duration)
        .map_err(ManualPowerError::EnergySink)?;
    let completes_at = state.tick().checked_add_span(duration).ok_or(
        ManualPowerError::CompletionTickOverflow {
            method: request.method,
        },
    )?;
    let equipment_use = provider.validated_use();
    let condition_after = calculate_usable_condition_after_active_ticks(
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    )
    .map_err(ManualPowerError::ConditionDuration)?;
    state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerError::EquipmentRevisionExhausted)?;
    state
        .energy()
        .revision()
        .checked_add(1)
        .ok_or(ManualPowerError::EnergyRevisionExhausted)?;
    let work = ManualPowerWork::new(
        request.method,
        equipment_use.trace(),
        condition_after,
        sink.trace(),
        state.tick(),
        completes_at,
    );
    let work_start = validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualPower { work },
        duration,
        schedule.exertion(),
    )
    .map_err(ManualPowerError::Work)?;
    let resource_budget = work_start.resource_budget();
    Ok(ValidatedManualPowerStart {
        work_start,
        work,
        resource_budget,
        expected_equipment_revision: equipment_use.expected_equipment_revision(),
        expected_energy_revision: state.energy().revision(),
    })
}
