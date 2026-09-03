//! Trusted-load replay for direct manual-power work.

use crate::core::quantity::{Energy, Power, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyStoreOccupancy, calculate_power_duration_ceiling, energy_store_occupancy,
    validate_energy_sink_capacity_at_release,
};
use crate::equipment::{EquipmentOccupancy, equipment_occupancy, resolve_equipment_capability};
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

use super::{ActivePlayerJobs, PlayerWorkValidationError, validate_remaining_resources};
use crate::labor::power_physics::{
    calculate_metabolic_duration, metabolic_output_per_tick, resolve_manual_power_exertion,
};
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
    let method = registries
        .labor()
        .get_manual_power(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::ManualPowerMethodMissing)?;
    let transfer_power = validate_manual_power_bindings(registries, state, work, method)?;
    let (required_duration, exertion) =
        validate_manual_power_schedule(registries, state, work, method, transfer_power)?;
    validate_manual_power_destination_capacity(registries, state, work)?;
    let required_condition = calculate_usable_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        work.equipment_trace().condition(),
        required_duration,
    )
    .map_err(PlayerWorkValidationError::ManualPowerConditionDuration)?;
    if work.condition_after() != required_condition {
        return Err(PlayerWorkValidationError::ManualPowerConditionMismatch);
    }
    let remaining_ticks = work.completes_at().value() - state.tick().value();
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        exertion,
        TickSpan::new(remaining_ticks),
    )
}

fn validate_manual_power_bindings(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    validate_manual_power_equipment_record(state, work)?;
    validate_manual_power_resource_availability(state, work)?;
    let destination_power = validate_manual_power_destination(registries, state, work, method)?;
    let equipment_power = resolve_manual_power_equipment_output(registries, state, work, method)?;
    let transfer_power = std::cmp::min(equipment_power, destination_power);
    if transfer_power.is_zero() {
        return Err(PlayerWorkValidationError::ManualPowerZeroPower);
    }
    Ok(transfer_power)
}

fn validate_manual_power_equipment_record(
    state: &AppState,
    work: ManualPowerWork,
) -> Result<(), PlayerWorkValidationError> {
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentMissing)?;
    if equipment.definition() != work.equipment_trace().definition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch);
    }
    if equipment.condition() != work.equipment_trace().condition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentConditionMismatch);
    }
    if equipment.supported_by().is_some() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentMounted);
    }
    Ok(())
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
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .unwrap_or_else(|| unreachable!("manual power equipment was validated before capability"));
    let equipment_definition = registries
        .equipment()
        .get_equipment(equipment.definition())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch)?;
    let capability = resolve_equipment_capability(
        equipment_definition,
        equipment.condition(),
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
) -> Result<(), PlayerWorkValidationError> {
    let destination = state
        .energy()
        .get_store(work.destination())
        .unwrap_or_else(|| unreachable!("manual power destination was validated before capacity"));
    let remaining = TickSpan::new(work.completes_at().value() - state.tick().value());
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
) -> Result<(TickSpan, SurvivalExertion), PlayerWorkValidationError> {
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::ManualPowerScheduleInvalid);
    }
    let stored_duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
    let power_duration = calculate_power_duration_ceiling(
        transfer_power,
        work.output().energy(),
        registries.core().physical_tick_duration(),
    )
    .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    let metabolic_output = metabolic_output_per_tick(
        method.maximum_exertion().energy_cost_per_tick(),
        method.metabolic_efficiency_ppm(),
    );
    let metabolic_duration = calculate_metabolic_duration(work.output().energy(), metabolic_output)
        .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    let required_duration = std::cmp::max(power_duration, metabolic_duration);
    if stored_duration != required_duration {
        return Err(PlayerWorkValidationError::ManualPowerDurationMismatch);
    }
    let exertion = resolve_manual_power_exertion(
        work.output().energy(),
        stored_duration,
        method.maximum_exertion(),
        method.metabolic_efficiency_ppm(),
    )
    .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    Ok((required_duration, exertion))
}
