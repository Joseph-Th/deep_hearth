//! Trusted-load replay for direct manual-power work.

use crate::core::quantity::{Energy, Power, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyStoreOccupancy, energy_store_occupancy, validate_energy_sink_capacity_at_release,
};
use crate::equipment::{
    EquipmentDefinition, EquipmentOccupancy, EquipmentOperationTrace, equipment_occupancy,
    resolve_equipment_capability,
};
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};
use crate::labor::power_physics::{ManualPowerScheduleError, resolve_manual_power_schedule};
use crate::labor::{ManualPowerDefinition, ManualPowerWork};

pub(super) fn validate_manual_power_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: ManualPowerWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    if state.equipment().revision().checked_add(1).is_none() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentRevisionExhausted);
    }
    if state.energy().revision().checked_add(1).is_none() {
        return Err(PlayerWorkValidationError::ManualPowerEnergyRevisionExhausted);
    }
    let method = registries
        .labor()
        .get_manual_power(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::ManualPowerMethodMissing)?;
    let transfer_power = validate_manual_power_bindings(registries, state, work, method)?;
    let (required_duration, remaining_duration, exertion) =
        validate_manual_power_schedule(registries, state, work, method, transfer_power)?;
    validate_manual_power_destination_capacity(registries, state, work, remaining_duration)?;
    let required_condition = calculate_usable_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        work.equipment_trace().condition(),
        required_duration,
    )
    .map_err(PlayerWorkValidationError::ManualPowerConditionDuration)?;
    if work.condition_after() != required_condition {
        return Err(PlayerWorkValidationError::ManualPowerConditionMismatch);
    }
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        exertion,
        remaining_duration,
    )
}

fn validate_manual_power_bindings(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    let equipment_definition =
        validate_manual_power_equipment_record(registries, state, work.equipment_trace())?;
    validate_manual_power_resource_availability(state, work)?;
    let destination_power = validate_manual_power_destination(registries, state, work, method)?;
    let equipment_power = resolve_manual_power_equipment_output(
        equipment_definition,
        work.equipment_trace(),
        method,
    )?;
    let transfer_power = std::cmp::min(equipment_power, destination_power);
    if transfer_power.is_zero() {
        return Err(PlayerWorkValidationError::ManualPowerZeroPower);
    }
    Ok(transfer_power)
}

fn validate_manual_power_equipment_record<'registry>(
    registries: &'registry Registries,
    state: &AppState,
    trace: EquipmentOperationTrace,
) -> Result<&'registry EquipmentDefinition, PlayerWorkValidationError> {
    let equipment = state
        .equipment()
        .get_equipment(trace.equipment())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentMissing)?;
    if equipment.definition() != trace.definition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch);
    }
    let definition = registries
        .equipment()
        .get_equipment(equipment.definition())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch)?;
    if definition.requires_structural_support() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentRequiresStructuralSupport);
    }
    if equipment.condition() != trace.condition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentConditionMismatch);
    }
    if equipment.supported_by().is_some() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentMounted);
    }
    Ok(definition)
}

fn validate_manual_power_resource_availability(
    state: &AppState,
    work: ManualPowerWork,
) -> Result<(), PlayerWorkValidationError> {
    if matches!(
        equipment_occupancy(state, work.equipment()),
        Some(EquipmentOccupancy::Production { .. } | EquipmentOccupancy::Mining { .. })
    ) || matches!(
        energy_store_occupancy(state, work.destination()),
        Some(EnergyStoreOccupancy::Production { .. })
    ) {
        return Err(PlayerWorkValidationError::ManualPowerResourceDoubleBooked);
    }
    Ok(())
}

fn validate_manual_power_destination(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    let destination = state
        .energy()
        .get_store(work.destination())
        .ok_or(PlayerWorkValidationError::ManualPowerDestinationMissing)?;
    if destination.definition() != work.output().definition() {
        return Err(PlayerWorkValidationError::ManualPowerDestinationDefinitionMismatch);
    }
    let energy_definition = registries
        .energy()
        .get_store(destination.definition())
        .ok_or(PlayerWorkValidationError::ManualPowerDestinationDefinitionMismatch)?;
    if energy_definition.carrier() != method.carrier()
        || work.output().carrier() != method.carrier()
    {
        return Err(PlayerWorkValidationError::ManualPowerCarrierMismatch);
    }
    if energy_definition.max_input_power().is_zero() {
        return Err(PlayerWorkValidationError::ManualPowerDestinationCannotAcceptEnergy);
    }
    Ok(energy_definition.max_input_power())
}

fn resolve_manual_power_equipment_output(
    equipment_definition: &EquipmentDefinition,
    trace: EquipmentOperationTrace,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    let capability = resolve_equipment_capability(
        equipment_definition,
        trace.condition(),
        method.power_capability(),
    )
    .ok_or(PlayerWorkValidationError::ManualPowerEquipmentCapabilityMissing)?;
    let crate::capability::CapabilityValue::Power(equipment_power) = capability else {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentCapabilityKindMismatch);
    };
    Ok(equipment_power)
}

fn validate_manual_power_destination_capacity(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    remaining: TickSpan,
) -> Result<(), PlayerWorkValidationError> {
    let destination = state
        .energy()
        .get_store(work.destination())
        .unwrap_or_else(|| unreachable!("manual power destination was validated before capacity"));
    validate_energy_sink_capacity_at_release(
        registries,
        destination.definition(),
        destination.stored(),
        work.output().energy(),
        remaining,
    )
    .map(|_projected_stored| ())
    .map_err(|_error| PlayerWorkValidationError::ManualPowerDestinationCapacityExceeded)
}

fn validate_manual_power_schedule(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
    transfer_power: Power,
) -> Result<(TickSpan, TickSpan, crate::survival::SurvivalExertion), PlayerWorkValidationError> {
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::ManualPowerScheduleInvalid)?;
    let stored_duration = schedule.duration;
    let required = resolve_manual_power_schedule(
        work.output().energy(),
        transfer_power,
        registries.core().physical_tick_duration(),
        method.maximum_exertion(),
        method.metabolic_efficiency_ppm(),
    )
    .map_err(|_error: ManualPowerScheduleError| {
        PlayerWorkValidationError::ManualPowerDurationMismatch
    })?;
    let required_duration = required.duration();
    if stored_duration != required_duration {
        return Err(PlayerWorkValidationError::ManualPowerDurationMismatch);
    }
    Ok((required_duration, schedule.remaining, required.exertion()))
}

#[cfg(test)]
#[path = "manual_power_tests.rs"]
mod tests;
