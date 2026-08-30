//! Exact derived physical quantities for homogeneous finite fluid contents.

use crate::material::MaterialId;
use crate::registry::Registries;

use super::{FluidContents, FluidDefinitionId, FluidStoreId};

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
