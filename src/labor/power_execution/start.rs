//! Admission and commit for direct player-powered generation.

use crate::capability::CapabilityValue;
use crate::core::quantity::Power;
use crate::core::state::AppState;
use crate::energy::{calculate_power_duration_ceiling, validate_energy_sink};
use crate::equipment::resolve_equipment_provider;
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;

use super::super::power_physics::{
    ManualPowerExertionError, ManualPowerMetabolicDurationError, calculate_metabolic_duration,
    metabolic_output_per_tick, resolve_manual_power_exertion,
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
        if let Some(job) = state
            .production()
            .get_equipment_occupant(self.work.equipment())
        {
            return Err(ManualPowerCommitError::EquipmentBusyProduction {
                equipment: self.work.equipment(),
                job: job.id(),
            });
        }
        if let Some(job) = state.mining().get_equipment_occupant(self.work.equipment()) {
            return Err(ManualPowerCommitError::EquipmentBusyMining {
                equipment: self.work.equipment(),
                job,
            });
        }
        if let Some(job) = state
            .production()
            .get_energy_occupant(self.work.destination())
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
    if let Some(job) = state.production().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyProduction {
            equipment: request.equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(request.equipment) {
        return Err(ManualPowerError::EquipmentBusyMining {
            equipment: request.equipment,
            job,
        });
    }
    let power_value = provider
        .get_capability(definition.power_capability())
        .ok_or(ManualPowerError::MissingPowerCapability {
            equipment: request.equipment,
            capability: definition.power_capability(),
        })?;
    let CapabilityValue::Power(equipment_power) = power_value else {
        return Err(ManualPowerError::PowerCapabilityKindMismatch {
            equipment: request.equipment,
            capability: definition.power_capability(),
            found: power_value.kind(),
        });
    };
    if equipment_power.is_zero() {
        return Err(ManualPowerError::ZeroEquipmentPower {
            equipment: request.equipment,
            capability: definition.power_capability(),
        });
    }
    let sink = validate_energy_sink(registries, state, request.destination, request.energy)
        .map_err(ManualPowerError::EnergySink)?;
    if sink.trace().carrier() != definition.carrier() {
        return Err(ManualPowerError::WrongCarrier {
            required: definition.carrier(),
            provided: sink.trace().carrier(),
        });
    }
    let transfer_power = std::cmp::min(equipment_power, sink.max_input_power());
    if transfer_power == Power::ZERO {
        return Err(ManualPowerError::ZeroTransferPower {
            equipment: request.equipment,
            destination: request.destination,
        });
    }
    let power_duration = calculate_power_duration_ceiling(
        transfer_power,
        request.energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(|_error| ManualPowerError::PowerDuration {
        energy: request.energy,
        power: transfer_power,
    })?;
    let metabolic_output = metabolic_output_per_tick(
        definition.maximum_exertion().energy_cost_per_tick(),
        definition.metabolic_efficiency_ppm(),
    );
    let metabolic_duration = calculate_metabolic_duration(request.energy, metabolic_output)
        .map_err(|error| match error {
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
        })?;
    let duration = std::cmp::max(power_duration, metabolic_duration);
    let exertion = resolve_manual_power_exertion(
        request.energy,
        duration,
        definition.maximum_exertion(),
        definition.metabolic_efficiency_ppm(),
    )
    .map_err(|error| match error {
        ManualPowerExertionError::EnergyOverflow
        | ManualPowerExertionError::ExceedsAuthoredMaximum => {
            ManualPowerError::ExertionResolution {
                method: request.method,
            }
        }
    })?;
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
        exertion,
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
