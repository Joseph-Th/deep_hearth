//! Player admission and fluid-type constraints on decant.

use super::*;
use crate::simulation::advance_tick;

#[test]
fn exclusive_player_work_blocks_local_and_downhill_pours_and_stales_prior_authorization() {
    use crate::content::{FORM_CHIP, MATERIAL_STONE, SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS};
    use crate::core::quantity::Mass;
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
        validate_start_player_surface_gathering,
    };

    for destination in [POSITION, VoxelCoord::new(0, -1, 0)] {
        let mut fixture = fixture_at(None, destination);
        let volume = Volume::from_microliters(100_000);
        let authorize = |state: &AppState| {
            if destination == POSITION {
                validate_transfer_fluid_between_local_stores(
                    &fixture.registries,
                    state,
                    fixture.source,
                    fixture.destination,
                    volume,
                )
            } else {
                validate_gravity_feed_fluid_to_lower_store(
                    &fixture.registries,
                    state,
                    fixture.source,
                    fixture.destination,
                    volume,
                )
            }
        };
        let token = authorize(&fixture.state).unwrap();
        let before_work_revision = fixture.state.player_work().revision();
        let source = validate_create_surface_resources(
            &fixture.registries,
            &fixture.state,
            [SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                POSITION,
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(50_000),
                ROOM_TEMPERATURE,
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap()],
        )
        .unwrap();
        let resource = source.first_resource_id();
        source.commit(&mut fixture.state).unwrap();
        validate_start_player_surface_gathering(
            &fixture.registries,
            &fixture.state,
            SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
            resource,
            Mass::from_milligrams(50_000),
        )
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
        let after_work_revision = fixture.state.player_work().revision();
        let fluid_before = fixture.state.fluid().revision();
        let accounting_before = calculate_fluid_volume_accounting(&fixture.state).unwrap();
        let unchanged = fixture.state.clone();
        validate_loaded_state(&fixture.registries, &fixture.state).unwrap();
        let encoded =
            serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
        let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&fixture.registries)
            .unwrap();
        assert_eq!(restored, fixture.state);

        assert_eq!(
            authorize(&fixture.state).err(),
            Some(FluidTransferError::PlayerBusy)
        );
        assert_eq!(
            token.commit(&mut fixture.state),
            Err(FluidTransferCommitError::StalePlayerWorkRevision {
                expected: before_work_revision,
                actual: after_work_revision,
            })
        );
        let options =
            player_fluid_transfer_options(&fixture.registries, &fixture.state, volume).unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].blocker(), Some(&FluidTransferError::PlayerBusy));
        assert_eq!(fixture.state.fluid().revision(), fluid_before);
        assert_eq!(
            calculate_fluid_volume_accounting(&fixture.state).unwrap(),
            accounting_before
        );
        assert_eq!(fixture.state, unchanged);
    }
}

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
