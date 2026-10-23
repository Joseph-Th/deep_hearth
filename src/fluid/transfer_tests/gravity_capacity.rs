//! Gravity delivery inherits canonical capacity, location and full-drain behavior.

use super::*;
use crate::fluid::validate_gravity_feed_fluid_to_lower_store;

const BELOW: VoxelCoord = VoxelCoord::new(0, -1, 0);

#[test]
fn downhill_feed_obeys_full_vessel_capacity_and_exact_source_exhaustion() {
    let full = fixture_at(
        Some((Volume::from_microliters(1_000_000), ROOM_TEMPERATURE)),
        BELOW,
    );
    let before = full.state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &full.registries,
            &full.state,
            full.source,
            full.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::InsufficientCapacity {
            available: Volume::ZERO,
            requested: Volume::from_microliters(1),
        }),
    );
    assert_eq!(full.state, before);

    let mut fixture = fixture_at(None, BELOW);
    let water_before = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let _ = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1_000_000),
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
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
        1_000_000,
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        water_before
    );
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::EmptySource {
            store: fixture.source
        }),
    );
    validate_loaded_state(&fixture.registries, &fixture.state).unwrap();
}

#[test]
fn downhill_feed_requires_a_real_located_destination() {
    let mut fixture = fixture_at(None, BELOW);
    let missing = add_fluid_store(&mut fixture.state, Volume::from_microliters(200)).unwrap();
    let before = fixture.state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            missing,
            Volume::from_microliters(10),
        )
        .err(),
        Some(FluidTransferError::DestinationAccess(
            PlayerFluidStoreAccessError::UnlocatedFluidStore { store: missing },
        )),
    );
    assert_eq!(fixture.state, before);
}
