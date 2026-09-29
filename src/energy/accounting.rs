//! Read-only explicit energy accounting across finite stores and modeled material thermal energy.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, PreciseEnergy};
use crate::core::state::AppState;
use crate::fluid::{FluidDefinitionId, FluidStoreId};
use crate::registry::Registries;
use crate::thermal::MaterialThermalEnergyError;

mod fluid;
mod owners;

use fluid::account_fluid_material;
use owners::account_nonfluid_energy_owners;

/// Snapshot of currently modeled explicit energy ownership.
///
/// Chemical, gravitational, elastic, kinetic, and environmental thermal energy are not inferred
/// here. This accounting covers finite stores, stored-fluid sensible heat, plus modeled sensible
/// and solid/liquid latent energy represented by authoritative material forms.
///
/// This is a diagnostic conservation surface, not actor-safe observation. Geological thermal
/// energy and the aggregate total include hidden finite geology and must not feed player policy;
/// actor geological decisions use acquired knowledge instead.
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
    account_fluid_material(registries, state, &mut accounting)?;
    account_nonfluid_energy_owners(registries, state, &mut accounting)?;
    Ok(accounting)
}

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;
