//! Read-only explicit energy accounting across finite stores and modeled material thermal energy.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::thermal::{MaterialThermalEnergyError, calculate_material_thermal_energy};

/// Snapshot of currently modeled explicit energy ownership.
///
/// Chemical, gravitational, elastic, kinetic, and environmental thermal energy are not inferred
/// here. This accounting covers finite stores plus modeled sensible and solid/liquid latent energy
/// represented by authoritative material forms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplicitEnergyAccounting {
    stored: Energy,
    geological_material_thermal: Energy,
    structural_material_thermal: Energy,
    equipment_material_thermal: Energy,
    energy_storage_material_thermal: Energy,
    inventory_material_thermal: Energy,
    mining_material_thermal: Energy,
    in_process_material_thermal: Energy,
    in_process_supplied: Energy,
}

impl ExplicitEnergyAccounting {
    #[must_use]
    pub const fn stored(self) -> Energy {
        self.stored
    }

    #[must_use]
    pub const fn geological_material_thermal(self) -> Energy {
        self.geological_material_thermal
    }

    #[must_use]
    pub const fn structural_material_thermal(self) -> Energy {
        self.structural_material_thermal
    }

    #[must_use]
    pub const fn equipment_material_thermal(self) -> Energy {
        self.equipment_material_thermal
    }

    #[must_use]
    pub const fn energy_storage_material_thermal(self) -> Energy {
        self.energy_storage_material_thermal
    }

    #[must_use]
    pub const fn inventory_material_thermal(self) -> Energy {
        self.inventory_material_thermal
    }

    #[must_use]
    pub const fn mining_material_thermal(self) -> Energy {
        self.mining_material_thermal
    }

    #[must_use]
    pub const fn in_process_material_thermal(self) -> Energy {
        self.in_process_material_thermal
    }

    #[must_use]
    pub const fn in_process_supplied(self) -> Energy {
        self.in_process_supplied
    }

    #[must_use]
    pub fn total(self) -> Option<Energy> {
        self.stored
            .checked_add(self.geological_material_thermal)?
            .checked_add(self.structural_material_thermal)?
            .checked_add(self.equipment_material_thermal)?
            .checked_add(self.energy_storage_material_thermal)?
            .checked_add(self.inventory_material_thermal)?
            .checked_add(self.mining_material_thermal)?
            .checked_add(self.in_process_material_thermal)?
            .checked_add(self.in_process_supplied)
    }
}

/// Failure to project currently modeled explicit energy ownership exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplicitEnergyAccountingError {
    MaterialThermal(MaterialThermalEnergyError),
    Overflow,
}

impl Display for ExplicitEnergyAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaterialThermal(error) => write!(
                formatter,
                "explicit energy accounting cannot determine material thermal energy: {error}"
            ),
            Self::Overflow => formatter.write_str("explicit energy accounting overflowed"),
        }
    }
}

impl Error for ExplicitEnergyAccountingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MaterialThermal(error) => Some(error),
            Self::Overflow => None,
        }
    }
}

fn add_energy(total: &mut Energy, value: Energy) -> Result<(), ExplicitEnergyAccountingError> {
    *total = total
        .checked_add(value)
        .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    Ok(())
}

fn add_material_thermal_energy(
    registries: &Registries,
    total: &mut Energy,
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
    add_energy(total, thermal)
}

fn add_trace_thermal_energy(
    registries: &Registries,
    total: &mut Energy,
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
    for job in state.mining().jobs() {
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

/// Projects explicit energy ownership without mutating state.
///
/// Material thermal energy uses absolute zero as the accounting reference. Liquid forms include
/// authored latent heat; unsupported mixed liquid phases fail explicitly rather than inventing an
/// alloy phase diagram.
pub fn calculate_explicit_energy_accounting(
    registries: &Registries,
    state: &AppState,
) -> Result<ExplicitEnergyAccounting, ExplicitEnergyAccountingError> {
    let mut accounting = ExplicitEnergyAccounting::default();
    account_energy_stores(registries, state, &mut accounting)?;
    account_geological_material(registries, state, &mut accounting)?;
    account_inventory_material(registries, state, &mut accounting)?;
    account_embodied_material(registries, state, &mut accounting)?;
    account_in_flight_material(registries, state, &mut accounting)?;
    Ok(accounting)
}
