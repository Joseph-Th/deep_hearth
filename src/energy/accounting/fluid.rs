//! Exact stored-fluid sensible and latent thermal-energy projection.

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{Energy, PreciseEnergy};
use crate::core::state::AppState;
use crate::fluid::{
    FluidContents, FluidMassProjectionError, FluidStoreId, project_fluid_material_mass,
};
use crate::registry::Registries;

use super::{ExplicitEnergyAccounting, ExplicitEnergyAccountingError};

const PICOJOULES_PER_NANOJOULE: u128 = 1_000;

fn multiply_thousandth_scaled(
    whole: u128,
    remainder: u128,
    multiplier: u32,
) -> Option<(u128, u128)> {
    debug_assert!(remainder < PICOJOULES_PER_NANOJOULE);
    let whole = whole.checked_mul(u128::from(multiplier))?;
    let remainder_product = remainder * u128::from(multiplier);
    let carry = remainder_product / PICOJOULES_PER_NANOJOULE;
    let remainder = remainder_product % PICOJOULES_PER_NANOJOULE;
    Some((whole.checked_add(carry)?, remainder))
}

fn calculate_fluid_thermal_energy(
    registries: &Registries,
    store: FluidStoreId,
    contents: FluidContents,
) -> Result<PreciseEnergy, ExplicitEnergyAccountingError> {
    let mass =
        project_fluid_material_mass(registries, store, contents).map_err(|error| match error {
            FluidMassProjectionError::UnknownDefinition { store, definition } => {
                ExplicitEnergyAccountingError::UnknownFluidDefinition { store, definition }
            }
        })?;
    let material = registries
        .materials()
        .get_material(mass.material())
        .unwrap_or_else(|| {
            panic!(
                "validated fluid mass projection references missing material {}",
                mass.material().value()
            )
        });
    let (whole, remainder) = checked_mul_div_with_remainder(
        mass.micrograms(),
        u128::from(contents.temperature().millikelvin()),
        PICOJOULES_PER_NANOJOULE,
        0,
    )
    .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    let (nanojoules, picojoule_remainder) = multiply_thousandth_scaled(
        whole,
        remainder,
        material.properties().thermal().specific_heat_j_per_kg_k(),
    )
    .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    let femtojoule_remainder = u32::try_from(picojoule_remainder)
        .ok()
        .and_then(|remainder| remainder.checked_mul(1_000))
        .unwrap_or_else(|| unreachable!("normalized picojoule remainder fits femtojoules"));
    let mut thermal =
        PreciseEnergy::from_nanojoules_with_femtojoule_remainder(nanojoules, femtojoule_remainder)
            .unwrap_or_else(|| {
                unreachable!("normalized fluid thermal remainder is below one nanojoule")
            });
    if let Some(fusion) = material.properties().thermal().fusion() {
        let latent_nanojoules = mass
            .micrograms()
            .checked_mul(u128::from(fusion.latent_heat_j_per_kg()))
            .ok_or(ExplicitEnergyAccountingError::Overflow)?;
        thermal = thermal
            .checked_add(PreciseEnergy::from_energy(Energy::from_nanojoules(
                latent_nanojoules,
            )))
            .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    }
    Ok(thermal)
}

pub(super) fn account_fluid_material(
    registries: &Registries,
    state: &AppState,
    accounting: &mut ExplicitEnergyAccounting,
) -> Result<(), ExplicitEnergyAccountingError> {
    for store in state.fluid().stores() {
        let Some(contents) = store.contents() else {
            continue;
        };
        let thermal = calculate_fluid_thermal_energy(registries, store.id(), contents)?;
        accounting.fluid_material_thermal = accounting
            .fluid_material_thermal
            .checked_add(thermal)
            .ok_or(ExplicitEnergyAccountingError::Overflow)?;
    }
    Ok(())
}
