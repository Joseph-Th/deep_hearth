//! Molten-phase heat equilibrium remains exact, but unprotected handling is unsafe.

use super::*;
use crate::content::{MATERIAL_COPPER, make_test_registries_with_fluids};
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::{FluidDefinition, FluidDefinitionId};

#[test]
fn molten_metal_equilibrium_is_exact_but_hand_decant_is_rejected() {
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
    let source_contents = state.fluid().get_store(source).unwrap().contents().unwrap();
    let destination_contents = state
        .fluid()
        .get_store(destination)
        .unwrap()
        .contents()
        .unwrap();
    let moved = Volume::from_microliters(100);
    let combined = destination_contents.volume().checked_add(moved).unwrap();
    assert_eq!(source_contents.fluid(), molten_copper);
    assert_eq!(destination_contents.fluid(), molten_copper);
    assert_eq!(
        equilibrium_temperature(source_contents, destination_contents, moved, combined),
        Ok(Temperature::from_millikelvin(1_357_772)),
        "thermally exact molten mixing must remain available to protected industrial owners"
    );
    let unchanged = state.clone();
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &registries,
            &state,
            source,
            destination,
            moved,
        )
        .err(),
        Some(FluidTransferError::UnsafeHandTemperature {
            store: source,
            temperature: source_contents.temperature(),
            maximum: crate::material::MAX_HAND_SAFE_ASSEMBLY_TEMPERATURE,
        })
    );
    assert_eq!(state, unchanged);
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
        "rejecting an unprotected molten pour must conserve latent and sensible heat"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
