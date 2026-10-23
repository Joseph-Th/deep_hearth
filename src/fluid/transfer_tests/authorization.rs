//! Player admission and fluid-type constraints on decant.

use super::*;
use crate::simulation::advance_tick;

#[test]
fn decant_token_cannot_commit_after_the_player_survival_state_advances() {
    let mut fixture = fixture(None);
    let token = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    )
    .unwrap();
    let expected = fixture.state.survival().revision();
    let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    let actual = fixture.state.survival().revision();
    assert_ne!(actual, expected);
    let before = fixture.state.clone();
    assert_eq!(
        token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleSurvivalRevision { expected, actual }),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn no_player_cannot_decant_through_the_player_owned_action() {
    let registries = build_registries();
    let state = AppState::new();
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &registries,
            &state,
            FluidStoreId::new(1),
            FluidStoreId::new(2),
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::PlayerNotAdmitted),
    );
}

#[test]
fn registered_fluid_identities_cannot_mix_even_at_equal_temperatures() {
    use crate::content::{MATERIAL_COPPER, MATERIAL_TIN, make_test_registries_with_fluids};
    use crate::fluid::{FluidDefinition, FluidDefinitionId};

    let first = FluidDefinitionId::new(912_001);
    let second = FluidDefinitionId::new(912_002);
    let registries = make_test_registries_with_fluids(vec![
        FluidDefinition::new(first, "test molten copper", MATERIAL_COPPER),
        FluidDefinition::new(second, "test molten tin", MATERIAL_TIN),
    ]);
    let molten_temperature = Temperature::from_millikelvin(1_400_000);
    let mut state = AppState::new();
    let a = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(100),
        first,
        Volume::from_microliters(50),
        molten_temperature,
    )
    .unwrap();
    let b = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(100),
        second,
        Volume::from_microliters(20),
        molten_temperature,
    )
    .unwrap();
    for store in [a, b] {
        validate_place_fluid_store(&state, store, POSITION)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &registries,
            &state,
            a,
            b,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::IncompatibleFluid {
            expected: second,
            found: first,
        }),
    );
    assert_eq!(state, before);
}
