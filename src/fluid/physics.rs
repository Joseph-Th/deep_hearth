//! Exact derived physical quantities for homogeneous finite fluid contents.

use crate::core::quantity::{Temperature, Volume};
use crate::material::MaterialId;
use crate::registry::Registries;

use super::{FluidContents, FluidDefinitionId, FluidState, FluidStoreId};

/// The soil share of a spring is physical fluid custody, with its own persisted
/// temperature. Derive a uniform physical view without teaching the state owner
/// about the particular water identity used by natural generation.
pub(crate) fn project_soaked_spring_water(
    state: &FluidState,
) -> impl Iterator<Item = (FluidStoreId, FluidContents)> + '_ {
    state.spring_soil().filter_map(|(id, soil)| {
        soil.soaked_temperature.map(|temperature| {
            (
                id,
                FluidContents {
                    fluid: crate::content::FLUID_WATER,
                    volume: soil.soaked,
                    temperature,
                },
            )
        })
    })
}

/// A homogeneous fluid may mix only when the resulting absolute temperature is
/// exactly representable in millikelvin. No implicit environmental heat sink
/// absorbs a fractional remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnrepresentableFluidMix {
    pub(crate) weighted_millikelvin_microliters: u128,
    pub(crate) combined_volume: Volume,
}

pub(crate) fn exact_mixed_temperature(
    existing: (Volume, Temperature),
    incoming: (Volume, Temperature),
    combined_volume: Volume,
) -> Result<Temperature, UnrepresentableFluidMix> {
    debug_assert_eq!(existing.0.checked_add(incoming.0), Some(combined_volume));
    let weighted = u128::from(existing.0.microliters()) * u128::from(existing.1.millikelvin())
        + u128::from(incoming.0.microliters()) * u128::from(incoming.1.millikelvin());
    let divisor = u128::from(combined_volume.microliters());
    if weighted % divisor != 0 {
        return Err(UnrepresentableFluidMix {
            weighted_millikelvin_microliters: weighted,
            combined_volume,
        });
    }
    Ok(Temperature::from_millikelvin(
        u32::try_from(weighted / divisor)
            .expect("weighted temperature lies between the two represented temperatures"),
    ))
}

/// Exact material mass backing one represented fluid volume, stored in micrograms.
///
/// One microliter at one kilogram per cubic meter is exactly one microgram. Keeping that unit here
/// prevents structural and thermal consumers from independently re-deriving the same conversion or
/// prematurely rounding fractional milligrams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FluidMaterialMass {
    material: MaterialId,
    micrograms: u128,
}

impl FluidMaterialMass {
    #[must_use]
    pub(crate) const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub(crate) const fn micrograms(self) -> u128 {
        self.micrograms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FluidMassProjectionError {
    UnknownDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
}

pub(crate) fn project_fluid_material_mass(
    registries: &Registries,
    store: FluidStoreId,
    contents: FluidContents,
) -> Result<FluidMaterialMass, FluidMassProjectionError> {
    let definition = registries.fluid().get_fluid(contents.fluid()).ok_or(
        FluidMassProjectionError::UnknownDefinition {
            store,
            definition: contents.fluid(),
        },
    )?;
    let material = registries
        .materials()
        .get_material(definition.material())
        .unwrap_or_else(|| {
            panic!(
                "validated fluid definition {} references missing material {}",
                definition.id().value(),
                definition.material().value()
            )
        });

    // `Volume` is u64 and authored density is u32, so this product is strictly narrower than u128.
    let micrograms = u128::from(contents.volume().microliters())
        * u128::from(material.properties().density_kg_per_m3());
    Ok(FluidMaterialMass {
        material: definition.material(),
        micrograms,
    })
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
