//! Explicit energy projection for non-fluid authoritative owners.

use crate::core::quantity::{Energy, Mass, PreciseEnergy, Temperature};
use crate::core::state::AppState;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::thermal::calculate_material_thermal_energy;

use super::{ExplicitEnergyAccounting, ExplicitEnergyAccountingError};

fn add_energy(total: &mut Energy, value: Energy) -> Result<(), ExplicitEnergyAccountingError> {
    *total = total
        .checked_add(value)
        .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    Ok(())
}

fn add_precise_energy(
    total: &mut PreciseEnergy,
    value: PreciseEnergy,
) -> Result<(), ExplicitEnergyAccountingError> {
    *total = total
        .checked_add(value)
        .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    Ok(())
}

fn add_material_thermal_energy(
    registries: &Registries,
    total: &mut PreciseEnergy,
    mass: Mass,
    commodity: CommodityKey,
    composition: &MaterialComposition,
    temperature: Temperature,
) -> Result<(), ExplicitEnergyAccountingError> {
    let thermal = calculate_material_thermal_energy(
        registries.materials(),
        mass,
        commodity,
        composition,
        temperature,
    )
    .map_err(ExplicitEnergyAccountingError::MaterialThermal)?;
    add_precise_energy(total, thermal)
}

fn add_trace_thermal_energy(
    registries: &Registries,
    total: &mut PreciseEnergy,
    trace: &ConsumedMaterialTrace,
) -> Result<(), ExplicitEnergyAccountingError> {
    let profile = trace.profile();
    add_material_thermal_energy(
        registries,
        total,
        trace.mass(),
        profile.commodity(),
        profile.composition(),
        profile.temperature(),
    )
}

fn account_energy_stores(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for store in state.energy().stores() {
        add_energy(&mut accounting.stored, store.stored())?;
        for trace in store.embodied_material() {
            add_trace_thermal_energy(
                registries,
                &mut accounting.energy_storage_material_thermal,
                trace,
            )?;
        }
    }
    Ok(())
}

fn account_geological_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for deposit in state.geology().deposits() {
        if deposit.remaining_mass().is_zero() {
            continue;
        }
        add_material_thermal_energy(
            registries,
            &mut accounting.geological_material_thermal,
            deposit.remaining_mass(),
            deposit.commodity(),
            deposit.composition(),
            deposit.temperature(),
        )?;
    }
    Ok(())
}

fn account_inventory_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for lot in state.inventory().lots() {
        add_material_thermal_energy(
            registries,
            &mut accounting.inventory_material_thermal,
            lot.mass(),
            lot.commodity(),
            lot.composition(),
            lot.temperature(),
        )?;
    }
    Ok(())
}

fn account_storage_infrastructure_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for stockpile in state.inventory().stockpiles() {
        let Some(enclosure) = stockpile.enclosure() else {
            continue;
        };
        for trace in enclosure.embodied_material() {
            add_trace_thermal_energy(
                registries,
                &mut accounting.storage_infrastructure_material_thermal,
                trace,
            )?;
        }
    }
    Ok(())
}

fn account_embodied_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for element in state.structures().elements() {
        for trace in element.embodied_material() {
            add_trace_thermal_energy(
                registries,
                &mut accounting.structural_material_thermal,
                trace,
            )?;
        }
    }
    for equipment in state.equipment().equipment() {
        for trace in equipment.embodied_material() {
            add_trace_thermal_energy(
                registries,
                &mut accounting.equipment_material_thermal,
                trace,
            )?;
        }
    }
    Ok(())
}

fn account_in_flight_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for job in state.mining().jobs().filter(|job| job.is_ready_to_claim()) {
        let output = job.output();
        add_material_thermal_energy(
            registries,
            &mut accounting.mining_material_thermal,
            output.mass(),
            output.commodity(),
            output.composition(),
            output.temperature(),
        )?;
    }
    for job in state.production().jobs() {
        for trace in job.consumed_inputs() {
            add_trace_thermal_energy(
                registries,
                &mut accounting.in_process_material_thermal,
                trace,
            )?;
        }
        if let Some(energy) = job.consumed_energy() {
            add_energy(&mut accounting.in_process_supplied, energy.energy())?;
        }
    }
    Ok(())
}

pub(super) fn account_nonfluid_energy_owners(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    account_energy_stores(registries, state, accounting)?;
    account_geological_material(registries, state, accounting)?;
    account_inventory_material(registries, state, accounting)?;
    account_storage_infrastructure_material(registries, state, accounting)?;
    account_embodied_material(registries, state, accounting)?;
    account_in_flight_material(registries, state, accounting)?;
    Ok(())
}
