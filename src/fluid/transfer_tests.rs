//! Exact local water custody, stale authorization, and conservation.

#[path = "transfer_tests/authorization.rs"]
mod authorization;

#[path = "transfer_tests/thermal.rs"]
mod thermal;

#[path = "transfer_tests/gravity.rs"]
mod gravity;

#[path = "transfer_tests/gravity_capacity.rs"]
mod gravity_capacity;

#[path = "transfer_tests/terrain.rs"]
mod terrain;

#[path = "transfer_tests/catalog.rs"]
mod catalog;

#[path = "transfer_tests/molten.rs"]
mod molten;

use super::*;
use crate::content::{FLUID_WATER, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::fluid::{
    add_fluid_store, add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting,
};
use crate::logistics::validate_place_fluid_store;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

const POSITION: VoxelCoord = VoxelCoord::new(0, 0, 0);
const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

struct Fixture {
    registries: Registries,
    state: AppState,
    source: FluidStoreId,
    destination: FluidStoreId,
}

fn fixture(destination_contents: Option<(Volume, Temperature)>) -> Fixture {
    fixture_at(destination_contents, POSITION)
}

fn fixture_at(
    destination_contents: Option<(Volume, Temperature)>,
    destination_position: VoxelCoord,
) -> Fixture {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(2_000_000),
        FLUID_WATER,
        Volume::from_microliters(1_000_000),
        ROOM_TEMPERATURE,
    )
    .unwrap();
    let destination = match destination_contents {
        Some((volume, temperature)) => add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            Volume::from_microliters(1_000_000),
            FLUID_WATER,
            volume,
            temperature,
        )
        .unwrap(),
        None => add_fluid_store(&mut state, Volume::from_microliters(1_000_000)).unwrap(),
    };
    for (store, position) in [(source, POSITION), (destination, destination_position)] {
        validate_place_fluid_store(&state, store, position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, POSITION)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    Fixture {
        registries,
        state,
        source,
        destination,
    }
}

#[test]
fn same_voxel_decant_and_merge_preserve_water_temperature_volume_and_save_continuation() {
    let mut fixture = fixture(None);
    let baseline = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let revision_before = fixture.state.fluid().revision();
    let first = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(400_000),
    )
    .unwrap();
    assert_eq!(first.volume().microliters(), 400_000);
    let outcome = first.commit(&mut fixture.state).unwrap();
    assert!(outcome.structural_analysis().is_none());
    assert_eq!(fixture.state.fluid().revision(), revision_before + 1);
    let moved = fixture
        .state
        .fluid()
        .get_store(fixture.destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(moved.fluid(), FLUID_WATER);
    assert_eq!(moved.temperature(), ROOM_TEMPERATURE);
    assert_eq!(moved.volume().microliters(), 400_000);
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.source)
            .unwrap()
            .stored_volume()
            .microliters(),
        600_000
    );

    let second = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(600_000),
    )
    .unwrap();
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    let loaded_token = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &loaded,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(600_000),
    )
    .unwrap();
    let _ = second.commit(&mut fixture.state).unwrap();
    let _ = loaded_token.commit(&mut loaded).unwrap();
    assert_eq!(fixture.state, loaded);
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.source)
            .unwrap()
            .contents(),
        None
    );
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.destination)
            .unwrap()
            .stored_volume()
            .microliters(),
        1_000_000
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        baseline
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}

#[test]
fn local_decant_rejects_unrepresentable_heat_and_insufficient_volume_or_capacity() {
    let regular = fixture(Some((Volume::from_microliters(200_000), ROOM_TEMPERATURE)));
    let before = regular.state.clone();
    let decant = |volume| {
        validate_transfer_fluid_between_local_stores(
            &regular.registries,
            &regular.state,
            regular.source,
            regular.destination,
            Volume::from_microliters(volume),
        )
        .err()
    };
    assert_eq!(decant(0), Some(FluidTransferError::ZeroVolume));
    assert_eq!(
        decant(1_000_001),
        Some(FluidTransferError::InsufficientVolume {
            available: Volume::from_microliters(1_000_000),
            requested: Volume::from_microliters(1_000_001),
        })
    );
    assert_eq!(
        decant(900_000),
        Some(FluidTransferError::InsufficientCapacity {
            available: Volume::from_microliters(800_000),
            requested: Volume::from_microliters(900_000),
        })
    );
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &regular.registries,
            &regular.state,
            regular.source,
            regular.source,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::SameStore),
    );
    assert_eq!(regular.state, before);

    let different_temperature = Temperature::from_millikelvin(303_150);
    let hot = fixture(Some((Volume::from_microliters(10), different_temperature)));
    let hot_before = hot.state.clone();
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &hot.registries,
            &hot.state,
            hot.source,
            hot.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: 3_324_650,
            combined_volume: Volume::from_microliters(11),
        }),
    );
    assert_eq!(hot.state, hot_before);
}

#[test]
fn prevalidated_decant_rejects_stale_owner_revision_without_any_movement() {
    let mut fixture = fixture(None);
    let token = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    )
    .unwrap();
    let expected = fixture.state.fluid().revision();
    let _unrelated = add_fluid_store(&mut fixture.state, Volume::from_microliters(1)).unwrap();
    let actual = fixture.state.fluid().revision();
    let before = fixture.state.clone();
    assert_eq!(
        token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleFluidRevision { expected, actual })
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn manual_decant_rejects_remote_stores_without_teleporting_water() {
    let fixture = fixture_at(None, VoxelCoord::new(1, 0, 0));
    let before = fixture.state.clone();
    assert_eq!(
        validate_transfer_fluid_between_local_stores(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::DestinationAccess(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store: fixture.destination,
                store_position: VoxelCoord::new(1, 0, 0),
                player_position: POSITION,
            }
        ))
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn prevalidated_decant_rejects_stale_logistics_without_any_movement() {
    let mut fixture = fixture(None);
    let token = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    )
    .unwrap();
    let expected = fixture.state.logistics().revision();
    let extra = add_fluid_store(&mut fixture.state, Volume::from_microliters(1)).unwrap();
    validate_place_fluid_store(&fixture.state, extra, POSITION)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let actual = fixture.state.logistics().revision();
    let before = fixture.state.clone();
    assert_eq!(
        token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(fixture.state, before);
}
