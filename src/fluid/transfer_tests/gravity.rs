//! Short, downward, player-local transfer between real neighboring world voxels.

use super::*;
use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Length};
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::validate_gravity_feed_fluid_to_lower_store;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
};

const LOWER: VoxelCoord = VoxelCoord::new(0, -1, 0);

fn add_downhill_wall(fixture: &mut Fixture, materialized: bool) {
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(LOWER, VoxelCoord::new(1, 0, 1)).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(1_000_000),
    )
    .unwrap();
    let wall = add_structural_element(
        &fixture.registries,
        &mut fixture.state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    if materialized {
        materialize_structural_element_for_test(
            &fixture.registries,
            &mut fixture.state,
            wall,
            FORM_LOG,
        );
    }
}

#[test]
fn downhill_feed_rejects_an_embodied_wall_without_losing_water() {
    let mut fixture = fixture_at(None, LOWER);
    add_downhill_wall(&mut fixture, true);
    assert!(fixture.state.structures().blocks_full_cell(LOWER));
    let before = fixture.state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(100_000),
        )
        .err(),
        Some(FluidTransferError::BlockedGravityDestination {
            destination_position: LOWER,
        }),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn downhill_feed_token_is_invalidated_by_a_new_structural_revision() {
    let mut fixture = fixture_at(None, LOWER);
    let token = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(100_000),
    )
    .unwrap();
    let expected = fixture.state.structures().revision();
    add_downhill_wall(&mut fixture, false);
    let actual = fixture.state.structures().revision();
    let before = fixture.state.clone();
    assert_eq!(
        token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleStructureRevision { expected, actual }),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn one_voxel_gravity_feed_preserves_water_thermal_energy_and_save_continuation() {
    let mut fixture = fixture_at(
        Some((
            Volume::from_microliters(100_000),
            Temperature::from_millikelvin(303_150),
        )),
        LOWER,
    );
    let before_fluid = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let before_energy = calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
        .unwrap()
        .total()
        .unwrap();
    let first = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(100_000),
    )
    .unwrap();
    assert_eq!(first.volume().microliters(), 100_000);
    let _ = first.commit(&mut fixture.state).unwrap();
    let after_destination = fixture
        .state
        .fluid()
        .get_store(fixture.destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(
        after_destination.temperature(),
        Temperature::from_millikelvin(298_150)
    );
    assert_eq!(after_destination.volume().microliters(), 200_000);
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.source)
            .unwrap()
            .contents()
            .unwrap()
            .volume()
            .microliters(),
        900_000,
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        before_fluid
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .total()
            .unwrap(),
        before_energy,
    );
    validate_loaded_state(&fixture.registries, &fixture.state).unwrap();
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    let next = |state: &AppState| {
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(200_000),
        )
        .unwrap()
    };
    let first = next(&fixture.state);
    let second = next(&loaded);
    let _ = first.commit(&mut fixture.state).unwrap();
    let _ = second.commit(&mut loaded).unwrap();
    assert_eq!(fixture.state, loaded);
    let final_contents = fixture
        .state
        .fluid()
        .get_store(fixture.destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(
        final_contents.temperature(),
        Temperature::from_millikelvin(295_650)
    );
    assert_eq!(final_contents.volume().microliters(), 400_000);
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        before_fluid
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .total()
            .unwrap(),
        before_energy,
    );
}

#[test]
fn gravity_feed_forbids_upward_horizontal_remote_and_skipped_voxel_routes() {
    for destination in [
        POSITION,
        VoxelCoord::new(1, 0, 0),
        VoxelCoord::new(0, 1, 0),
        VoxelCoord::new(0, -2, 0),
        VoxelCoord::new(1, -1, 0),
        VoxelCoord::new(0, 0, 1),
    ] {
        let fixture = fixture_at(None, destination);
        let before = fixture.state.clone();
        assert_eq!(
            validate_gravity_feed_fluid_to_lower_store(
                &fixture.registries,
                &fixture.state,
                fixture.source,
                fixture.destination,
                Volume::from_microliters(1),
            )
            .err(),
            Some(FluidTransferError::InvalidGravityRoute {
                source_position: POSITION,
                destination_position: destination,
            }),
        );
        assert_eq!(fixture.state, before);
    }
    let valid = fixture_at(None, LOWER);
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &valid.registries,
            &valid.state,
            valid.destination,
            valid.source,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::SourceAccess(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store: valid.destination,
                store_position: LOWER,
                player_position: POSITION,
            },
        )),
    );
}

#[test]
fn gravity_feed_rechecks_fluid_logistics_and_player_authorization_before_mutation() {
    use crate::simulation::advance_tick;

    let mut fixture = fixture_at(None, LOWER);
    let fluid_token = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(2),
    )
    .unwrap();
    let fluid_expected = fixture.state.fluid().revision();
    let _ = add_fluid_store(&mut fixture.state, Volume::from_microliters(1)).unwrap();
    let fluid_actual = fixture.state.fluid().revision();
    let before = fixture.state.clone();
    assert_eq!(
        fluid_token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleFluidRevision {
            expected: fluid_expected,
            actual: fluid_actual,
        }),
    );
    assert_eq!(fixture.state, before);

    let logistics_token = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(2),
    )
    .unwrap();
    let logistics_expected = fixture.state.logistics().revision();
    let extra = add_fluid_store(&mut fixture.state, Volume::from_microliters(1)).unwrap();
    validate_place_fluid_store(&fixture.state, extra, LOWER)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let logistics_actual = fixture.state.logistics().revision();
    let before = fixture.state.clone();
    assert_eq!(
        logistics_token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleLogisticsRevision {
            expected: logistics_expected,
            actual: logistics_actual,
        }),
    );
    assert_eq!(fixture.state, before);

    let survival_token = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(2),
    )
    .unwrap();
    let survival_expected = fixture.state.survival().revision();
    let _ = advance_tick(&fixture.registries, &mut fixture.state).unwrap();
    let survival_actual = fixture.state.survival().revision();
    assert_ne!(survival_actual, survival_expected);
    let before = fixture.state.clone();
    assert_eq!(
        survival_token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleSurvivalRevision {
            expected: survival_expected,
            actual: survival_actual,
        }),
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn gravity_feed_refuses_fractional_temperature_without_spilling_water_downhill() {
    let fixture = fixture_at(
        Some((
            Volume::from_microliters(10),
            Temperature::from_millikelvin(303_150),
        )),
        LOWER,
    );
    let before = fixture.state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: 3_324_650,
            combined_volume: Volume::from_microliters(11),
        }),
    );
    assert_eq!(fixture.state, before);
}
