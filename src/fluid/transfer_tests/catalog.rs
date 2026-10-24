//! Player action discovery reuses the executable fluid and terrain validators.

use super::*;
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::{
    PlayerFluidTransferRoute, player_fluid_transfer_options,
    validate_gravity_feed_fluid_to_lower_store,
};

const BELOW: VoxelCoord = VoxelCoord::new(0, -1, 0);

#[test]
fn local_and_downhill_options_are_discoverable_and_executable_without_remote_leaks() {
    let mut fixture = fixture_at(None, BELOW);
    let local = add_fluid_store(&mut fixture.state, Volume::from_microliters(500_000)).unwrap();
    let remote = add_fluid_store(&mut fixture.state, Volume::from_microliters(500_000)).unwrap();
    for (store, position) in [(local, POSITION), (remote, VoxelCoord::new(8, 0, 0))] {
        validate_place_fluid_store(&fixture.state, store, position)
            .unwrap()
            .commit(&mut fixture.state)
            .unwrap();
    }
    let before = fixture.state.clone();
    let choices = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(200_000),
    )
    .unwrap();
    assert_eq!(
        fixture.state, before,
        "catalog lookup cannot reserve or mutate"
    );
    assert_eq!(choices.len(), 2);
    assert!(choices.iter().all(|row| row.is_ready()));
    assert!(choices.iter().all(|row| row.source() == fixture.source));
    assert_eq!(choices[0].destination(), fixture.destination);
    assert_eq!(choices[0].route(), PlayerFluidTransferRoute::OneVoxelDown);
    assert_eq!(choices[1].destination(), local);
    assert_eq!(choices[1].route(), PlayerFluidTransferRoute::SameVoxel);
    assert_eq!(choices[0].requested(), Volume::from_microliters(200_000));
    assert_eq!(
        choices[0].source_after().unwrap().volume().microliters(),
        800_000
    );
    assert_eq!(
        choices[0]
            .destination_after()
            .unwrap()
            .volume()
            .microliters(),
        200_000
    );
    assert_eq!(
        choices[0].destination_after().unwrap().temperature(),
        ROOM_TEMPERATURE
    );
    assert!(fixture.state.logistics().has_valid_fluid_position_index());
    assert_eq!(
        fixture
            .state
            .logistics()
            .fluid_stores_at(POSITION)
            .collect::<Vec<_>>(),
        vec![fixture.source, local]
    );
    assert_eq!(
        fixture
            .state
            .logistics()
            .fluid_stores_at(BELOW)
            .collect::<Vec<_>>(),
        vec![fixture.destination]
    );
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    assert!(restored.logistics().has_valid_fluid_position_index());
    assert_eq!(restored, fixture.state);
    assert_eq!(
        player_fluid_transfer_options(
            &fixture.registries,
            &restored,
            Volume::from_microliters(200_000),
        )
        .unwrap(),
        choices,
        "rebuilding the derived spatial index must preserve transfer discovery"
    );

    let before_water = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let before_heat = calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
        .unwrap()
        .total()
        .unwrap();
    let _ = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        choices[0].source(),
        choices[0].destination(),
        choices[0].requested(),
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        before_water
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .total()
            .unwrap(),
        before_heat
    );
    let refreshed = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(200_000),
    )
    .unwrap();
    assert_eq!(
        refreshed[0].source_after().unwrap().volume().microliters(),
        600_000
    );
    assert_eq!(
        refreshed[0]
            .destination_after()
            .unwrap()
            .volume()
            .microliters(),
        400_000
    );
    validate_loaded_state(&fixture.registries, &fixture.state).unwrap();
}

#[test]
fn catalog_reports_exact_amount_specific_temperature_and_capacity_blockers() {
    let mut fixture = fixture_at(
        Some((
            Volume::from_microliters(10),
            Temperature::from_millikelvin(303_150),
        )),
        BELOW,
    );
    let one = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(1),
    )
    .unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].route(), PlayerFluidTransferRoute::OneVoxelDown);
    assert!(!one[0].is_ready());
    assert_eq!(
        one[0].blocker(),
        Some(&FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: 3_324_650,
            combined_volume: Volume::from_microliters(11),
        })
    );
    assert_eq!(one[0].destination_after(), None);
    let ten = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(10),
    )
    .unwrap();
    assert_eq!(ten[0].blocker(), None);
    assert_eq!(
        ten[0].destination_after().unwrap().temperature(),
        Temperature::from_millikelvin(298_150)
    );

    // The planner does not guess an available amount when the vessel is full.
    let local = add_fluid_store_with_contents_for_fixture(
        &fixture.registries,
        &mut fixture.state,
        Volume::from_microliters(100),
        FLUID_WATER,
        Volume::from_microliters(100),
        ROOM_TEMPERATURE,
    )
    .unwrap();
    validate_place_fluid_store(&fixture.state, local, POSITION)
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let full = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(10),
    )
    .unwrap();
    let blocked = full.iter().find(|row| row.destination() == local).unwrap();
    assert_eq!(
        blocked.blocker(),
        Some(&FluidTransferError::InsufficientCapacity {
            available: Volume::ZERO,
            requested: Volume::from_microliters(10),
        })
    );
}

#[test]
fn catalog_requires_an_admitted_living_player_and_positive_request() {
    let empty = AppState::new();
    let registries = build_registries();
    assert_eq!(
        player_fluid_transfer_options(&registries, &empty, Volume::from_microliters(1),),
        Err(FluidTransferError::PlayerNotAdmitted)
    );
    let fixture = fixture(None);
    assert_eq!(
        player_fluid_transfer_options(&fixture.registries, &fixture.state, Volume::ZERO),
        Err(FluidTransferError::ZeroVolume),
    );
}

#[test]
fn discovery_reports_unknown_downhill_terrain_without_remote_custody_leaks() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

    let mut fixture = fixture_at(None, BELOW);
    let remote = add_fluid_store_with_contents_for_fixture(
        &fixture.registries,
        &mut fixture.state,
        Volume::from_microliters(200_000),
        FLUID_WATER,
        Volume::from_microliters(150_000),
        ROOM_TEMPERATURE,
    )
    .unwrap();
    validate_place_fluid_store(&fixture.state, remote, VoxelCoord::new(30, 0, 30))
        .unwrap()
        .commit(&mut fixture.state)
        .unwrap();
    let _ = validate_generate_chunk(
        &fixture.registries,
        &fixture.state,
        WorldGenerationSeed::new(0xF17D_2026),
        ChunkCoord::new(10, 10),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    let before = fixture.state.clone();
    let choices = player_fluid_transfer_options(
        &fixture.registries,
        &fixture.state,
        Volume::from_microliters(5),
    )
    .unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].source(), fixture.source);
    assert_eq!(choices[0].destination(), fixture.destination);
    assert_eq!(
        choices[0].blocker(),
        Some(&FluidTransferError::UnknownGravityTerrain {
            destination_position: BELOW,
        })
    );
    assert_eq!(fixture.state, before);
}
