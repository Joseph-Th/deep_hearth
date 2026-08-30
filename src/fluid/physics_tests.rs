//! Exact homogeneous-fluid mass projection regressions.

use crate::content::{FLUID_WATER, MATERIAL_WATER, build_registries};
use crate::core::quantity::{Temperature, Volume};

use super::*;

#[test]
fn fluid_mass_projection_uses_exact_microgram_units() {
    let registries = build_registries();
    let store = FluidStoreId::new(1);
    let contents = FluidContents {
        fluid: FLUID_WATER,
        volume: Volume::from_microliters(7),
        temperature: Temperature::from_millikelvin(293_150),
    };

    let projected = project_fluid_material_mass(&registries, store, contents)
        .unwrap_or_else(|error| panic!("fluid mass projection failed: {error:?}"));

    assert_eq!(projected.material(), MATERIAL_WATER);
    assert_eq!(projected.micrograms(), 7_000);
}
