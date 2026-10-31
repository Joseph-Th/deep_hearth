//! Naturally seeded basin water, rooted fruit, and exact custody under drought.

use super::*;

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::quantity::Volume;
use crate::core::state::validate_loaded_state;
use crate::fluid::{
    add_fluid_store, calculate_fluid_volume_accounting,
    validate_transfer_fluid_between_local_stores,
};
use crate::logistics::{relocate_player_for_test, validate_place_fluid_store};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind};
use crate::world::{
    ChunkCoord, WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};

fn rooted_basin() -> (
    Registries,
    AppState,
    SurfaceResourceId,
    crate::fluid::FluidStoreId,
) {
    let registries = build_registries();
    for seed_number in 0..160 {
        let seed = WorldGenerationSeed::new(seed_number);
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let spawn = VoxelCoord::new(
            0,
            super::super::generation::terrain_height(seed, 0, 0) + 1,
            0,
        );
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();

        let mut found = None;
        'scan: for source in state.surface().resources() {
            if source.kind() != SurfaceResourceKind::Opportunity || source.perennial.is_none() {
                continue;
            }
            if root_moisture_at(&state, source.position())
                != (RootMoisture::Basin {
                    fraction_ppm: 1_000_000,
                })
            {
                continue;
            }
            for (id, soil) in state.fluid().spring_soil() {
                if source.position().x().abs_diff(soil.position.x()) <= BASIN_ROOT_REACH_VOXELS
                    && source.position().z().abs_diff(soil.position.z()) <= BASIN_ROOT_REACH_VOXELS
                    && source.position().y().abs_diff(soil.position.y())
                        <= BASIN_ROOT_VERTICAL_REACH_VOXELS
                {
                    found = Some((source.id(), id));
                    break 'scan;
                }
            }
        }
        if let Some((source, id)) = found {
            return (registries, state, source, id);
        }
    }
    panic!("no original generated woodland has a rooted food patch close to a finite spring");
}

fn ready_crop(registries: &Registries, state: &mut AppState, source: SurfaceResourceId) -> Mass {
    let profile = state.world().generation_profile().unwrap();
    let origin = state.surface().get(source).unwrap().position();
    let since = state.tick();
    let habitat = habitat_at(profile.seed(), origin.x(), origin.z());
    let due = (1..50)
        .map(|index| SimulationTick::new(since.value() + index * FIELD_CHECK_TICKS))
        .find(|&at| {
            at.value().saturating_sub(since.value()) >= recovery_ticks(habitat)
                && (283_150..=303_150).contains(
                    &weather_at(registries, profile.seed().value(), origin, at)
                        .ambient_temperature()
                        .millikelvin(),
                )
        })
        .unwrap();
    crate::core::state::apply_clock_advance(state, due);
    apply_natural_forage_cycle(registries, state, due);
    assert_eq!(
        state.surface().get(source).unwrap().lifecycle(),
        SurfaceResourceLifecycle::Available
    );
    state.surface().get(source).unwrap().remaining_mass()
}

#[test]
fn moving_actual_spring_water_to_a_container_reduces_neighboring_fruit_yield() {
    let (registries, mut watered, source, basin) = rooted_basin();
    let plant = watered.surface().get(source).unwrap().position();
    let spring_position = watered
        .fluid()
        .spring_soil()
        .find(|(id, _)| *id == basin)
        .unwrap()
        .1
        .position;
    assert_eq!(
        root_moisture_at(&watered, plant),
        RootMoisture::Basin {
            fraction_ppm: 1_000_000
        }
    );

    let first_crop = watered.surface().get(source).unwrap().remaining_mass();
    let next_revision = watered.surface().revision() + 1;
    watered.surface_state_mut().apply_gathering(
        source,
        first_crop,
        SimulationTick::ZERO,
        next_revision,
    );
    let matter_baseline = calculate_matter_accounting(&watered).unwrap().total();
    let water_baseline = calculate_fluid_volume_accounting(&watered).unwrap();
    let mut drought = watered.clone();

    // Relocate existing source water by the player's canonical two-store
    // transfer; there is no test-only emptying, water deletion or loot reset.
    relocate_player_for_test(&mut drought, spring_position);
    let standing = drought.fluid().get_store(basin).unwrap().stored_volume();
    let container = add_fluid_store(&mut drought, standing).unwrap();
    validate_place_fluid_store(&drought, container, spring_position)
        .unwrap()
        .commit(&mut drought)
        .unwrap();
    let moved = validate_transfer_fluid_between_local_stores(
        &registries,
        &drought,
        basin,
        container,
        standing,
    )
    .unwrap();
    let _ = moved.commit(&mut drought).unwrap();
    assert_eq!(
        drought.fluid().get_store(basin).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        root_moisture_at(&drought, plant),
        RootMoisture::Basin { fraction_ppm: 0 }
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&drought).unwrap(),
        water_baseline
    );
    assert_eq!(
        calculate_matter_accounting(&drought).unwrap().total(),
        matter_baseline
    );

    // The same canonical transfer can put every microliter back in the
    // original basin. A vessel at this position cannot hydrate a root by
    // mere proximity; only restored natural custody does.
    let mut refilled = drought.clone();
    let returned = validate_transfer_fluid_between_local_stores(
        &registries,
        &refilled,
        container,
        basin,
        standing,
    )
    .unwrap();
    let _ = returned.commit(&mut refilled).unwrap();
    assert_eq!(
        root_moisture_at(&refilled, plant),
        RootMoisture::Basin {
            fraction_ppm: 1_000_000
        }
    );
    let mut half_full = drought.clone();
    let half = Volume::from_microliters(standing.microliters() / 2);
    let partial = validate_transfer_fluid_between_local_stores(
        &registries,
        &half_full,
        container,
        basin,
        half,
    )
    .unwrap();
    let _ = partial.commit(&mut half_full).unwrap();
    let RootMoisture::Basin { fraction_ppm } = root_moisture_at(&half_full, plant) else {
        panic!("half-filled spring must retain its original natural basin identity");
    };
    assert!((499_000..=500_000).contains(&fraction_ppm));

    let watered_crop = ready_crop(&registries, &mut watered, source);
    let drought_crop = ready_crop(&registries, &mut drought, source);
    let restored_crop = ready_crop(&registries, &mut refilled, source);
    let half_crop = ready_crop(&registries, &mut half_full, source);
    assert!(
        watered_crop > drought_crop,
        "draining real nearby spring water must reduce the next edible crop"
    );
    assert_eq!(
        restored_crop, watered_crop,
        "returning water to the natural basin should restore the same fruit yield"
    );
    assert!(
        drought_crop < half_crop && half_crop < watered_crop,
        "fruit yield should reflect partial physical water depletion"
    );
    for state in [&watered, &drought, &refilled, &half_full] {
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            matter_baseline
        );
        assert_eq!(
            calculate_fluid_volume_accounting(state).unwrap(),
            water_baseline
        );
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &drought)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, drought);
}

#[test]
fn water_stored_beneath_a_dry_looking_spring_remains_available_to_roots() {
    let (registries, mut state, source, basin) = rooted_basin();
    let location = state.surface().get(source).unwrap().position();
    let natural = state
        .fluid()
        .spring_soil()
        .find(|(id, _)| *id == basin)
        .unwrap()
        .1
        .position;
    let seed = state.world().generation_profile().unwrap().seed().value();
    let drying_front = (1..90)
        .map(|front| SimulationTick::new(front * FIELD_CHECK_TICKS))
        .find(|&at| {
            super::super::weather::spring_moisture_flux(&registries, &state, natural, at)
                == super::super::weather::SpringMoistureFlux::Drying
                && (283_150..=303_150).contains(
                    &weather_at(&registries, seed, location, at)
                        .ambient_temperature()
                        .millikelvin(),
                )
        })
        .expect("actual seeded weather should provide a drying front");
    let original_water = calculate_fluid_volume_accounting(&state).unwrap();
    let previous = state.fluid().get_store(basin).unwrap().stored_volume();
    crate::core::state::apply_clock_advance(&mut state, drying_front);
    crate::fluid::apply_natural_water_cycle(&registries, &mut state, drying_front);
    assert_eq!(
        state
            .fluid()
            .get_store(basin)
            .unwrap()
            .stored_volume()
            .microliters(),
        previous.microliters() - 250_000,
    );
    assert_eq!(
        state
            .fluid()
            .spring_soil()
            .find(|(id, _)| *id == basin)
            .unwrap()
            .1
            .soaked,
        Volume::from_microliters(250_000)
    );
    assert_eq!(
        root_moisture_at(&state, location),
        RootMoisture::Basin {
            fraction_ppm: 1_000_000
        }
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        original_water
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
