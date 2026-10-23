//! Generic same-fluid heat equilibrium must conserve molten phase energy too.

use super::*;
use crate::content::{MATERIAL_COPPER, make_test_registries_with_fluids};
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::{FluidDefinition, FluidDefinitionId};

#[test]
fn molten_metal_decant_keeps_phase_latent_heat_and_fluid_identity() {
    let molten_copper = FluidDefinitionId::new(912_401);
    let registries = make_test_registries_with_fluids(vec![FluidDefinition::new(
        molten_copper,
        "molten copper thermal mixing",
        MATERIAL_COPPER,
    )]);
    let mut state = AppState::new();
    let source = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(400),
        molten_copper,
        Volume::from_microliters(200),
        Temperature::from_millikelvin(1_357_771),
    )
    .unwrap();
    let destination = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(400),
        molten_copper,
        Volume::from_microliters(100),
        Temperature::from_millikelvin(1_357_773),
    )
    .unwrap();
    for store in [source, destination] {
        validate_place_fluid_store(&state, store, POSITION)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mass_before = calculate_fluid_volume_accounting(&state).unwrap();
    let heat_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap()
        .total()
        .unwrap();
    let _ = validate_transfer_fluid_between_local_stores(
        &registries,
        &state,
        source,
        destination,
        Volume::from_microliters(100),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let mixed = state
        .fluid()
        .get_store(destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(mixed.fluid(), molten_copper);
    assert_eq!(mixed.volume(), Volume::from_microliters(200));
    assert_eq!(
        mixed.temperature(),
        Temperature::from_millikelvin(1_357_772)
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        mass_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap()
            .total()
            .unwrap(),
        heat_before,
        "molten-fluid latent heat and sensible heat must both be conserved"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
