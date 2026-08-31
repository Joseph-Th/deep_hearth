//! Exact stored-fluid thermal-energy accounting regressions.

use crate::content::{
    FLUID_WATER, MATERIAL_COPPER, build_registries, make_test_registries_with_fluids,
};
use crate::core::quantity::{Energy, Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::fluid::{FluidDefinition, FluidDefinitionId, add_fluid_store_with_contents_for_fixture};

use super::{PreciseEnergy, calculate_explicit_energy_accounting};

#[test]
fn stored_liquid_water_thermal_energy_includes_sensible_and_fusion_energy() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xEACC_0001));
    let volume = Volume::from_microliters(10);
    let temperature = Temperature::from_millikelvin(293_150);
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        temperature,
    )
    .unwrap_or_else(|error| panic!("water energy fixture failed: {error}"));

    let accounting = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("water energy accounting failed: {error}"));
    let sensible_nanojoules =
        u128::from(volume.microliters()) * u128::from(temperature.millikelvin()) * 4_184_u128;
    let latent_nanojoules = u128::from(volume.microliters()) * 1_000_u128 * 333_550_u128;
    let expected_nanojoules = sensible_nanojoules + latent_nanojoules;

    assert_eq!(
        accounting.fluid_material_thermal(),
        PreciseEnergy::from_energy(Energy::from_nanojoules(expected_nanojoules))
    );
    assert_eq!(
        accounting.total(),
        Some(PreciseEnergy::from_energy(Energy::from_nanojoules(
            expected_nanojoules,
        )))
    );
}

#[test]
fn fractional_fluid_sensible_heat_is_retained_without_rounding() {
    const COPPER_FLUID: FluidDefinitionId = FluidDefinitionId::new(940_101);

    let registries = make_test_registries_with_fluids(vec![FluidDefinition::new(
        COPPER_FLUID,
        "copper fluid energy fixture",
        MATERIAL_COPPER,
    )]);
    let mut state = AppState::new(WorldSeed::new(0xEACC_0002));
    let volume = Volume::from_microliters(1);
    let temperature = Temperature::from_millikelvin(1_357_771);
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        COPPER_FLUID,
        volume,
        temperature,
    )
    .unwrap_or_else(|error| panic!("fractional fluid energy fixture failed: {error}"));

    let accounting = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("fractional fluid energy accounting failed: {error}"));
    let picojoules = u128::from(volume.microliters())
        * 8_960_u128
        * u128::from(temperature.millikelvin())
        * 385_u128;
    let latent_nanojoules = u128::from(volume.microliters()) * 8_960_u128 * 205_000_u128;
    let expected_nanojoules = picojoules / 1_000 + latent_nanojoules;
    let expected_remainder = ((picojoules % 1_000) * 1_000) as u32;
    let fluid = accounting.fluid_material_thermal();

    assert_eq!(fluid.nanojoules_floor(), expected_nanojoules);
    assert_eq!(fluid.femtojoule_remainder(), expected_remainder);
    assert_ne!(expected_remainder, 0);
    assert_eq!(accounting.total(), Some(fluid));
    assert_eq!(
        fluid.whole_nanojoules(),
        None,
        "exact fractional fluid heat must not silently narrow to whole nanojoules"
    );
}

#[test]
fn fractional_fluid_remainders_carry_across_stores_without_per_store_rounding() {
    const COPPER_FLUID: FluidDefinitionId = FluidDefinitionId::new(940_102);

    let registries = make_test_registries_with_fluids(vec![FluidDefinition::new(
        COPPER_FLUID,
        "copper fluid carry fixture",
        MATERIAL_COPPER,
    )]);
    let mut state = AppState::new(WorldSeed::new(0xEACC_0003));
    let volume = Volume::from_microliters(1);
    let temperature = Temperature::from_millikelvin(1_357_771);
    for _ in 0..2 {
        add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            volume,
            COPPER_FLUID,
            volume,
            temperature,
        )
        .unwrap_or_else(|error| panic!("fractional fluid carry fixture failed: {error}"));
    }

    let accounting = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("fractional fluid carry accounting failed: {error}"));
    let sensible_picojoules = u128::from(volume.microliters())
        * 8_960_u128
        * u128::from(temperature.millikelvin())
        * 385_u128;
    let latent_nanojoules = u128::from(volume.microliters()) * 8_960_u128 * 205_000_u128;
    let per_store_floor = sensible_picojoules / 1_000 + latent_nanojoules;
    let per_store_remainder = sensible_picojoules % 1_000;
    let combined_remainder = per_store_remainder * 2;
    let fluid = accounting.fluid_material_thermal();

    assert_eq!(
        fluid.nanojoules_floor(),
        per_store_floor * 2 + combined_remainder / 1_000
    );
    assert_eq!(
        fluid.femtojoule_remainder(),
        ((combined_remainder % 1_000) * 1_000) as u32
    );
    assert_eq!(fluid.femtojoule_remainder(), 200_000);
}
