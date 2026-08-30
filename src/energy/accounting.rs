//! Read-only explicit energy accounting across finite stores and modeled material thermal energy.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, PreciseEnergy, Temperature};
use crate::core::state::AppState;
use crate::fluid::{FluidDefinitionId, FluidStoreId};
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{CommodityKey, MaterialComposition};
use crate::registry::Registries;
use crate::thermal::{MaterialThermalEnergyError, calculate_material_thermal_energy};

mod fluid;

use fluid::account_fluid_material;

/// Snapshot of currently modeled explicit energy ownership.
///
/// Chemical, gravitational, elastic, kinetic, and environmental thermal energy are not inferred
/// here. This accounting covers finite stores, stored-fluid sensible heat, plus modeled sensible
/// and solid/liquid latent energy represented by authoritative material forms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplicitEnergyAccounting {
    stored: Energy,
    fluid_material_thermal: PreciseEnergy,
    geological_material_thermal: PreciseEnergy,
    structural_material_thermal: PreciseEnergy,
    equipment_material_thermal: PreciseEnergy,
    energy_storage_material_thermal: PreciseEnergy,
    storage_infrastructure_material_thermal: PreciseEnergy,
    inventory_material_thermal: PreciseEnergy,
    mining_material_thermal: PreciseEnergy,
    in_process_material_thermal: PreciseEnergy,
    in_process_supplied: Energy,
}

impl ExplicitEnergyAccounting {
    #[must_use]
    pub const fn stored(self) -> Energy {
        self.stored
    }

    #[must_use]
    pub const fn fluid_material_thermal(self) -> PreciseEnergy {
        self.fluid_material_thermal
    }

    #[must_use]
    pub const fn geological_material_thermal(self) -> PreciseEnergy {
        self.geological_material_thermal
    }

    #[must_use]
    pub const fn structural_material_thermal(self) -> PreciseEnergy {
        self.structural_material_thermal
    }

    #[must_use]
    pub const fn equipment_material_thermal(self) -> PreciseEnergy {
        self.equipment_material_thermal
    }

    #[must_use]
    pub const fn energy_storage_material_thermal(self) -> PreciseEnergy {
        self.energy_storage_material_thermal
    }

    #[must_use]
    pub const fn storage_infrastructure_material_thermal(self) -> PreciseEnergy {
        self.storage_infrastructure_material_thermal
    }

    #[must_use]
    pub const fn inventory_material_thermal(self) -> PreciseEnergy {
        self.inventory_material_thermal
    }

    #[must_use]
    pub const fn mining_material_thermal(self) -> PreciseEnergy {
        self.mining_material_thermal
    }

    #[must_use]
    pub const fn in_process_material_thermal(self) -> PreciseEnergy {
        self.in_process_material_thermal
    }

    #[must_use]
    pub const fn in_process_supplied(self) -> Energy {
        self.in_process_supplied
    }

    /// Exact total including sub-nanojoule material and fluid thermal energy.
    ///
    /// `None` means the exact aggregate exceeded the representable whole-nanojoule range.
    #[must_use]
    pub fn total(self) -> Option<PreciseEnergy> {
        let mut total = PreciseEnergy::from_energy(self.stored);
        total = total.checked_add(self.fluid_material_thermal)?;
        for energy in [
            self.geological_material_thermal,
            self.structural_material_thermal,
            self.equipment_material_thermal,
            self.energy_storage_material_thermal,
            self.storage_infrastructure_material_thermal,
            self.inventory_material_thermal,
            self.mining_material_thermal,
            self.in_process_material_thermal,
        ] {
            total = total.checked_add(energy)?;
        }
        total = total.checked_add(PreciseEnergy::from_energy(self.in_process_supplied))?;
        Some(total)
    }
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

/// Failure to project currently modeled explicit energy ownership exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplicitEnergyAccountingError {
    MaterialThermal(MaterialThermalEnergyError),
    UnknownFluidDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
    Overflow,
}

impl Display for ExplicitEnergyAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaterialThermal(error) => write!(
                formatter,
                "explicit energy accounting cannot determine material thermal energy: {error}"
            ),
            Self::UnknownFluidDefinition { store, definition } => write!(
                formatter,
                "fluid store {} references unknown fluid definition {} during explicit energy accounting",
                store.value(),
                definition.value()
            ),
            Self::Overflow => formatter.write_str("explicit energy accounting overflowed"),
        }
    }
}

impl Error for ExplicitEnergyAccountingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MaterialThermal(error) => Some(error),
            Self::UnknownFluidDefinition { .. } | Self::Overflow => None,
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

/// Projects explicit energy ownership without mutating state.
///
/// Material thermal energy uses absolute zero as the accounting reference. Liquid forms include
/// authored latent heat; unsupported mixed liquid phases fail explicitly rather than inventing an
/// alloy phase diagram. Stored fluids contribute exact sensible heat from represented volume,
/// material density, temperature, and specific heat; a fluid whose material has authored fusion
/// properties also contributes its liquid latent heat. Matter and fluid already transferred
/// into the terminal survival-consumption boundary are excluded because biological transformation,
/// waste, and consumed-material thermal fate are outside the current explicit-energy model.
pub fn calculate_explicit_energy_accounting(
    registries: &Registries,
    state: &AppState,
) -> Result<ExplicitEnergyAccounting, ExplicitEnergyAccountingError> {
    let mut accounting = ExplicitEnergyAccounting::default();
    account_energy_stores(registries, state, &mut accounting)?;
    account_fluid_material(registries, state, &mut accounting)?;
    account_geological_material(registries, state, &mut accounting)?;
    account_inventory_material(registries, state, &mut accounting)?;
    account_storage_infrastructure_material(registries, state, &mut accounting)?;
    account_embodied_material(registries, state, &mut accounting)?;
    account_in_flight_material(registries, state, &mut accounting)?;
    Ok(accounting)
}

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;
