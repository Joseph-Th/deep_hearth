//! Player admission and fluid-type constraints on decant.

use super::*;
use crate::simulation::advance_tick;

#[test]
fn prepared_weapon_blocks_pours_and_invalidates_prior_transfer_authorization() {
    use crate::combat::{validate_cancel_primitive_preparation, validate_prepare_primitive_strike};
    use crate::content::EQUIPMENT_WOODEN_CLUB;
    use crate::equipment::validate_assemble_equipment;
    use crate::inventory::deposit_lot_for_test;
    use crate::logistics::{
        PlayerActiveSelection, validate_pickup_equipment, validate_set_active_selection,
    };
    use crate::spatial::ViewDirection;

    for destination_position in [POSITION, VoxelCoord::new(0, -1, 0)] {
        let mut fixture = fixture_at(None, destination_position);
        let volume = Volume::from_microliters(200_000);
        let source = fixture
            .state
            .logistics()
            .player()
            .unwrap()
            .carried_stockpile();
        let assembly = fixture
            .registries
            .equipment()
            .get_equipment(EQUIPMENT_WOODEN_CLUB)
            .unwrap()
            .assembly_profile()
            .unwrap()
            .clone();
        for input in assembly.inputs() {
            deposit_lot_for_test(
                &fixture.registries,
                &mut fixture.state,
                source,
                input.commodity(),
                input.mass(),
                ROOM_TEMPERATURE,
            )
            .unwrap();
        }
        let club = validate_assemble_equipment(
            &fixture.registries,
            &fixture.state,
            EQUIPMENT_WOODEN_CLUB,
            source,
        )
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
        validate_pickup_equipment(&fixture.registries, &fixture.state, club)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
        validate_set_active_selection(
            &fixture.registries,
            &fixture.state,
            Some(PlayerActiveSelection::equipment(club)),
        )
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();

        let validate = |state: &AppState| {
            if destination_position == POSITION {
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
        let prevalidated = validate(&fixture.state).unwrap();
        let combat_before = fixture.state.combat().revision();
        let facing = ViewDirection::new(1_000_000, 0, 0).unwrap();
        let _ = validate_prepare_primitive_strike(&fixture.registries, &fixture.state, facing)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
        let combat_after = fixture.state.combat().revision();
        let water_before = calculate_fluid_volume_accounting(&fixture.state).unwrap();
        let unchanged = fixture.state.clone();
        assert_eq!(
            validate(&fixture.state).err(),
            Some(FluidTransferError::PreparedStrikeActive)
        );
        let options =
            player_fluid_transfer_options(&fixture.registries, &fixture.state, volume).unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(
            options[0].blocker(),
            Some(&FluidTransferError::PreparedStrikeActive)
        );
        assert_eq!(
            prevalidated.commit(&mut fixture.state),
            Err(FluidTransferCommitError::StaleCombatRevision {
                expected: combat_before,
                actual: combat_after,
            })
        );
        assert_eq!(fixture.state, unchanged);
        assert_eq!(
            calculate_fluid_volume_accounting(&fixture.state).unwrap(),
            water_before
        );
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );

        let saved =
            serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&fixture.registries)
            .unwrap();
        assert_eq!(replay, fixture.state);
        assert_eq!(
            validate(&replay).err(),
            Some(FluidTransferError::PreparedStrikeActive)
        );
        for state in [&mut fixture.state, &mut replay] {
            let _ = validate_cancel_primitive_preparation(state)
                .unwrap()
                .commit(state)
                .unwrap();
            assert_eq!(
                validate_player_fluid_transfer_option(&fixture.registries, state, &options[0],)
                    .err(),
                Some(FluidTransferError::ObservedOutcomeChanged {
                    source: fixture.source,
                    destination: fixture.destination,
                }),
                "a formerly blocked catalog row requires a fresh observation"
            );
            let refreshed =
                player_fluid_transfer_options(&fixture.registries, state, volume).unwrap();
            assert!(refreshed[0].is_ready());
            let _ =
                validate_player_fluid_transfer_option(&fixture.registries, state, &refreshed[0])
                    .unwrap()
                    .commit(state)
                    .unwrap();
        }
        assert_eq!(fixture.state, replay);
        assert_eq!(
            calculate_fluid_volume_accounting(&fixture.state).unwrap(),
            water_before,
        );
        assert_eq!(
            validate_loaded_state(&fixture.registries, &fixture.state),
            Ok(())
        );
    }
}

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
fn local_pour_rejects_inaccessible_endpoints_before_inspecting_vessel_contents() {
    let remote = VoxelCoord::new(1, 0, 0);
    let mut fixture = fixture_at(None, remote);
    let missing = FluidStoreId::new(999_001);
    let amount = Volume::from_microliters(1);
    let empty_local = add_fluid_store(&mut fixture.state, amount).unwrap();
    validate_place_fluid_store(&fixture.state, empty_local, POSITION)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let before = fixture.state.clone();
    let remote_access = PlayerFluidStoreAccessError::RemoteKnownFluidStore {
        store: fixture.destination,
        store_position: remote,
        player_position: POSITION,
    };

    // An unknown destination must not override the remote source's access denial.
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &fixture.registries,
            &fixture.state,
            fixture.destination,
            missing,
            amount,
        )
        .err(),
        Some(FluidTransferError::SourceAccess(remote_access)),
    );
    // An empty local source must not reveal its contents before denying
    // access to the destination.
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &fixture.registries,
            &fixture.state,
            empty_local,
            fixture.destination,
            amount,
        )
        .err(),
        Some(FluidTransferError::DestinationAccess(remote_access)),
    );
    // Truly unknown local identities still have a precise typed error.
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            missing,
            amount,
        )
        .err(),
        Some(FluidTransferError::UnknownStore { store: missing }),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn gravity_feed_rejects_remote_source_before_unknown_lower_destination() {
    let lower = VoxelCoord::new(0, -1, 0);
    let fixture = fixture_at(None, lower);
    let before = fixture.state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.destination,
            FluidStoreId::new(999_002),
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::SourceAccess(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store: fixture.destination,
                store_position: lower,
                player_position: POSITION,
            },
        )),
    );
    assert_eq!(fixture.state, before);
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
