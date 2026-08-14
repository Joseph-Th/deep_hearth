//! Read-only explicit energy accounting across finite stores and sensible heat in geological,
//! structural, inventory, and in-process matter ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Temperature};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::thermal::{SensibleHeatError, calculate_sensible_heat};

/// Snapshot of currently modeled explicit energy ownership.
///
/// Chemical, gravitational, elastic, kinetic, latent, and environmental thermal energy are not
/// inferred here. This accounting intentionally covers only energy forms already represented by
/// authoritative runtime state: finite stores and sensible heat of geological, structural,
/// inventory, and in-process material below unresolved phase boundaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplicitEnergyAccounting {
    stored: Energy,
    geological_sensible: Energy,
    structural_sensible: Energy,
    inventory_sensible: Energy,
    in_process_sensible: Energy,
    in_process_supplied: Energy,
}

impl ExplicitEnergyAccounting {
    #[must_use]
    pub const fn stored(self) -> Energy {
        self.stored
    }

    #[must_use]
    pub const fn geological_sensible(self) -> Energy {
        self.geological_sensible
    }

    #[must_use]
    pub const fn structural_sensible(self) -> Energy {
        self.structural_sensible
    }

    #[must_use]
    pub const fn inventory_sensible(self) -> Energy {
        self.inventory_sensible
    }

    #[must_use]
    pub const fn in_process_sensible(self) -> Energy {
        self.in_process_sensible
    }

    #[must_use]
    pub const fn in_process_supplied(self) -> Energy {
        self.in_process_supplied
    }

    #[must_use]
    pub fn total(self) -> Option<Energy> {
        self.stored
            .checked_add(self.geological_sensible)?
            .checked_add(self.structural_sensible)?
            .checked_add(self.inventory_sensible)?
            .checked_add(self.in_process_sensible)?
            .checked_add(self.in_process_supplied)
    }
}

/// Failure to project currently modeled explicit energy ownership exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplicitEnergyAccountingError {
    SensibleHeat(SensibleHeatError),
    Overflow,
}

impl Display for ExplicitEnergyAccountingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SensibleHeat(error) => write!(
                formatter,
                "explicit energy accounting cannot determine material sensible heat: {error}"
            ),
            Self::Overflow => formatter.write_str("explicit energy accounting overflowed"),
        }
    }
}

impl Error for ExplicitEnergyAccountingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SensibleHeat(error) => Some(error),
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

/// Projects explicit energy ownership without mutating state.
///
/// Material sensible energy uses absolute zero as the accounting reference. If an owned material
/// is already beyond a phase boundary that the current thermal model cannot resolve, accounting
/// fails rather than inventing latent heat.
pub fn calculate_explicit_energy_accounting(
    registries: &Registries,
    state: &AppState,
) -> Result<ExplicitEnergyAccounting, ExplicitEnergyAccountingError> {
    let mut accounting = ExplicitEnergyAccounting::default();

    for store in state.energy().stores() {
        add_energy(&mut accounting.stored, store.stored())?;
    }

    for deposit in state.geology().deposits() {
        if deposit.remaining_mass().is_zero() {
            continue;
        }
        let sensible = calculate_sensible_heat(
            registries.materials(),
            deposit.remaining_mass(),
            deposit.composition(),
            Temperature::ZERO,
            deposit.temperature(),
        )
        .map_err(ExplicitEnergyAccountingError::SensibleHeat)?;
        add_energy(&mut accounting.geological_sensible, sensible.energy())?;
    }

    for lot in state.inventory().lots() {
        let sensible = calculate_sensible_heat(
            registries.materials(),
            lot.mass(),
            lot.composition(),
            Temperature::ZERO,
            lot.temperature(),
        )
        .map_err(ExplicitEnergyAccountingError::SensibleHeat)?;
        add_energy(&mut accounting.inventory_sensible, sensible.energy())?;
    }

    for element in state.structures().elements() {
        for trace in element.embodied_material() {
            let profile = trace.profile();
            let sensible = calculate_sensible_heat(
                registries.materials(),
                trace.mass(),
                profile.composition(),
                Temperature::ZERO,
                profile.temperature(),
            )
            .map_err(ExplicitEnergyAccountingError::SensibleHeat)?;
            add_energy(&mut accounting.structural_sensible, sensible.energy())?;
        }
    }

    for job in state.production().jobs() {
        for trace in job.consumed_inputs() {
            let profile = trace.profile();
            let sensible = calculate_sensible_heat(
                registries.materials(),
                trace.mass(),
                profile.composition(),
                Temperature::ZERO,
                profile.temperature(),
            )
            .map_err(ExplicitEnergyAccountingError::SensibleHeat)?;
            add_energy(&mut accounting.in_process_sensible, sensible.energy())?;
        }
        if let Some(energy) = job.consumed_energy() {
            add_energy(&mut accounting.in_process_supplied, energy.energy())?;
        }
    }

    Ok(accounting)
}
