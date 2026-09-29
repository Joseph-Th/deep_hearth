//! Read-only current-state planning bounds shared by powered ore-processing families.

use crate::core::state::AppState;
use crate::energy::{
    EnergyStoreId, assess_energy_supply_access, calculate_mass_specific_energy_capacity,
};
use crate::equipment::{EquipmentId, resolve_available_equipment_provider};
use crate::maintenance::maximum_usable_active_ticks;
use crate::production::ProcessId;
use crate::registry::{ProcessExecutionFamily, Registries};

use super::PoweredOreProcessProfile;
use super::powered_physics::{
    PoweredOreEquipmentError, powered_ore_mass_capacity_for_active_ticks,
    resolve_powered_ore_equipment_limits, validate_powered_ore_process_capabilities,
};

mod envelope;
mod errors;

pub use envelope::{
    PoweredOreMassConstraint, PoweredOreMassEnvelope, PoweredOreReplenishmentConstraint,
};
pub use errors::PoweredOreMassEnvelopeError;

/// Derives the current monotonic mass bounds shared by powered comminution, screening, and
/// constituent separation.
///
/// This projection validates current equipment support, capability, and occupancy plus current
/// energy-supply access, but it does not reserve either resource and does not inspect a material
/// selection. Callers use it to size a candidate, then invoke the process-specific resolver for
/// exact feed/output legality.
pub fn assess_powered_ore_mass_envelope(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    equipment: EquipmentId,
    energy_store: EnergyStoreId,
) -> Result<PoweredOreMassEnvelope, PoweredOreMassEnvelopeError> {
    let profile = powered_profile(registries, process)
        .ok_or(PoweredOreMassEnvelopeError::UnknownPoweredProcess { process })?;
    let provider = resolve_available_equipment_provider(registries, state, equipment)
        .map_err(PoweredOreMassEnvelopeError::Equipment)?;
    validate_powered_ore_process_capabilities(
        registries,
        process,
        provider.definition(),
        provider.condition(),
    )
    .map_err(PoweredOreMassEnvelopeError::Capability)?;
    let equipment_limits = resolve_powered_ore_equipment_limits(
        provider.definition(),
        provider.condition(),
        profile.mass_flow_capability(),
        profile.max_batch_mass_capability(),
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            PoweredOreMassEnvelopeError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            PoweredOreMassEnvelopeError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { .. } => {
            unreachable!("planning resolves equipment limits without a selected batch")
        }
    })?;
    let energy = assess_energy_supply_access(registries, state, energy_store)
        .map_err(PoweredOreMassEnvelopeError::Energy)?;
    if energy.carrier() != profile.energy_carrier() {
        return Err(PoweredOreMassEnvelopeError::WrongEnergyCarrier {
            required: profile.energy_carrier(),
            provided: energy.carrier(),
        });
    }

    let specific_energy = profile.specific_energy();
    let stored_energy_capacity =
        calculate_mass_specific_energy_capacity(energy.available(), specific_energy);
    let replenished_energy_capacity =
        calculate_mass_specific_energy_capacity(energy.capacity(), specific_energy);
    let condition_ticks = maximum_usable_active_ticks(
        profile.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    );
    let physical_tick_duration = registries.core().physical_tick_duration();
    let condition_lifetime_capacity = powered_ore_mass_capacity_for_active_ticks(
        equipment_limits.processing_rate(),
        energy.max_output_power(),
        specific_energy,
        condition_ticks,
        physical_tick_duration,
    );

    Ok(PoweredOreMassEnvelope {
        equipment_capacity: equipment_limits.maximum_batch_mass(),
        stored_energy_capacity,
        replenished_energy_capacity,
        condition_lifetime_capacity,
        available_energy: energy.available(),
        processing_rate: equipment_limits.processing_rate(),
        available_power: energy.max_output_power(),
        specific_energy,
        condition_before: provider.condition(),
        wear_ppm_per_active_tick: profile.condition_wear_ppm_per_active_tick(),
        physical_tick_duration,
    })
}

pub(super) fn powered_profile(
    registries: &Registries,
    process: ProcessId,
) -> Option<PoweredOreProcessProfile> {
    let topology = registries.process_topology(process)?;
    match topology.execution_family() {
        ProcessExecutionFamily::Comminution => Some(
            registries
                .ore_processing()
                .get_comminution(process)
                .unwrap_or_else(|| {
                    panic!(
                        "comminution topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::Screening => Some(
            registries
                .ore_processing()
                .get_screening(process)
                .unwrap_or_else(|| {
                    panic!(
                        "screening topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::ConstituentSeparation => Some(
            registries
                .ore_processing()
                .get_constituent_separation(process)
                .unwrap_or_else(|| {
                    panic!(
                        "constituent-separation topology references missing ore process {}",
                        process.value()
                    )
                })
                .operating_profile(),
        ),
        ProcessExecutionFamily::ManualCraft
        | ProcessExecutionFamily::PoweredCraft
        | ProcessExecutionFamily::ManualComminution
        | ProcessExecutionFamily::ManualSeparation
        | ProcessExecutionFamily::SensibleHeating
        | ProcessExecutionFamily::Melting
        | ProcessExecutionFamily::Casting => None,
    }
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
