//! Conservation at the exact finite fluid heat-mixing boundary.

use super::*;
use crate::energy::calculate_explicit_energy_accounting;

const WARM_TEMPERATURE: Temperature = Temperature::from_millikelvin(303_150);

#[test]
fn exact_temperature_equilibrium_preserves_total_heat_and_water_across_replay() {
    let mut fixture = fixture(Some((Volume::from_microliters(100_000), WARM_TEMPERATURE)));
    let water_before = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let heat_before = calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
        .unwrap()
        .total()
        .unwrap();
    let heat_in_fluid = calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
        .unwrap()
        .fluid_material_thermal();
    let revision = fixture.state.fluid().revision();

    let first = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(100_000),
    )
    .unwrap();
    let _ = first.commit(&mut fixture.state).unwrap();
    assert_eq!(fixture.state.fluid().revision(), revision + 1);
    let mixed = fixture
        .state
        .fluid()
        .get_store(fixture.destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(mixed.temperature(), Temperature::from_millikelvin(298_150));
    assert_eq!(mixed.volume(), Volume::from_microliters(200_000));
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.source)
            .unwrap()
            .contents()
            .unwrap()
            .temperature(),
        ROOM_TEMPERATURE,
        "the unmoved water does not magically equilibrate with the destination"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        water_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .total()
            .unwrap(),
        heat_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .fluid_material_thermal(),
        heat_in_fluid
    );

    let bytes =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let mut resumed = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    let next = |state: &AppState| {
        validate_transfer_fluid_between_local_stores(
            &fixture.registries,
            state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(200_000),
        )
        .unwrap()
    };
    let expected = next(&fixture.state);
    let actual = next(&resumed);
    let _ = expected.commit(&mut fixture.state).unwrap();
    let _ = actual.commit(&mut resumed).unwrap();
    assert_eq!(fixture.state, resumed);
    let destination = fixture
        .state
        .fluid()
        .get_store(fixture.destination)
        .unwrap()
        .contents()
        .unwrap();
    assert_eq!(destination.volume(), Volume::from_microliters(400_000));
    assert_eq!(
        destination.temperature(),
        Temperature::from_millikelvin(295_650)
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        water_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&fixture.registries, &fixture.state)
            .unwrap()
            .total()
            .unwrap(),
        heat_before
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
}

#[test]
fn thermally_unrepresentable_volume_weighting_rejects_without_any_state_change() {
    let fixture = fixture(Some((Volume::from_microliters(10), WARM_TEMPERATURE)));
    let before = fixture.state.clone();
    let token = validate_transfer_fluid_between_local_stores(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    );
    assert_eq!(
        token.err(),
        Some(FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: 3_324_650,
            combined_volume: Volume::from_microliters(11),
        })
    );
    assert_eq!(fixture.state, before);
}

#[test]
fn equilibrium_math_handles_full_width_volumes_without_thermal_rounding() {
    let source = FluidContents {
        fluid: FLUID_WATER,
        volume: Volume::from_microliters(u64::MAX),
        temperature: Temperature::from_millikelvin(u32::MAX),
    };
    let destination = FluidContents {
        fluid: FLUID_WATER,
        volume: Volume::from_microliters(u64::MAX - 1),
        temperature: Temperature::from_millikelvin(u32::MAX - 1),
    };
    let moved = Volume::from_microliters(1);
    assert_eq!(
        equilibrium_temperature(
            source,
            destination,
            moved,
            Volume::from_microliters(u64::MAX)
        ),
        Err(FluidTransferError::UnrepresentableEquilibriumTemperature {
            weighted_millikelvin_microliters: u128::from(u32::MAX)
                + u128::from(u64::MAX - 1) * u128::from(u32::MAX - 1),
            combined_volume: Volume::from_microliters(u64::MAX),
        })
    );
    // A near-maximum pair with equal temperatures is representable exactly.
    assert_eq!(
        equilibrium_temperature(
            destination,
            destination,
            moved,
            Volume::from_microliters(u64::MAX),
        ),
        Ok(Temperature::from_millikelvin(u32::MAX - 1))
    );
}

#[test]
fn bounded_mixing_grid_has_exact_heat_balance_or_refuses_rounding() {
    for source_temperature in [293_150_u32, 293_151, 305_500, u32::MAX] {
        for destination_temperature in [293_149_u32, 293_150, 303_150, u32::MAX] {
            for existing in [1_u64, 2, 3, 7, 10, 97, 1024] {
                for moved in [1_u64, 2, 3, 11, 53, 100] {
                    let source = FluidContents {
                        fluid: FLUID_WATER,
                        volume: Volume::from_microliters(200),
                        temperature: Temperature::from_millikelvin(source_temperature),
                    };
                    let destination = FluidContents {
                        fluid: FLUID_WATER,
                        volume: Volume::from_microliters(existing),
                        temperature: Temperature::from_millikelvin(destination_temperature),
                    };
                    let volume = Volume::from_microliters(moved);
                    let combined = Volume::from_microliters(existing + moved);
                    let weighted = u128::from(moved) * u128::from(source_temperature)
                        + u128::from(existing) * u128::from(destination_temperature);
                    let divisor = u128::from(combined.microliters());
                    match equilibrium_temperature(source, destination, volume, combined) {
                        Ok(temperature) => {
                            assert_eq!(
                                u128::from(temperature.millikelvin()) * divisor,
                                weighted,
                                "accepted blending must conserve exact sensible heat"
                            );
                            assert!(
                                (source_temperature.min(destination_temperature)
                                    ..=source_temperature.max(destination_temperature))
                                    .contains(&temperature.millikelvin())
                            );
                        }
                        Err(FluidTransferError::UnrepresentableEquilibriumTemperature {
                            weighted_millikelvin_microliters,
                            combined_volume,
                        }) => {
                            assert_eq!(weighted_millikelvin_microliters, weighted);
                            assert_eq!(combined_volume, combined);
                            assert_ne!(weighted % divisor, 0);
                        }
                        Err(error) => panic!("unexpected heat-mixing error: {error:?}"),
                    }
                }
            }
        }
    }
}
