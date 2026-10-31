//! Weather is an observable, seed- and time-bound input to real survival.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{ThermalExposure, assess_player_unheated_comfort_runway};
use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

fn generated_player() -> (Registries, AppState, WorldGenerationSeed) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5745_4154_4845_5253);
    let mut state = AppState::new();
    assert_eq!(assess_player_weather(&registries, &state), None);
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
    assert_eq!(assess_player_weather(&registries, &state), None);
    let standing = VoxelCoord::new(
        0,
        super::super::generation::terrain_height(seed, 0, 0) + 1,
        0,
    );
    validate_admit_player(&registries, &state, standing)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, seed)
}

#[test]
fn weather_changes_with_real_time_season_and_region_without_random_mutation() {
    let (registries, state, seed) = generated_player();
    let before = state.clone();
    let here = state.logistics().player().unwrap().position();
    let now = assess_player_weather(&registries, &state).unwrap();
    assert_eq!(now.season(), Season::Spring);
    assert_eq!(now.phase(), DayPhase::Morning);
    assert!(now.changes_in().value() > 0 && now.changes_in().value() <= WEATHER_FRONT_TICKS);
    assert!((MINIMUM_COOLING_PPM..=MAXIMUM_COOLING_PPM).contains(&now.cooling_multiplier_ppm()));
    assert_eq!(assess_player_weather(&registries, &state), Some(now));
    assert_eq!(
        before, state,
        "observing weather cannot advance or mutate the world"
    );

    let mut observed = std::collections::BTreeSet::new();
    for front in 0..12 {
        let condition = weather_at(
            &registries,
            seed.value(),
            here,
            SimulationTick::new(front * WEATHER_FRONT_TICKS),
        );
        observed.insert((
            condition.ambient_temperature().millikelvin(),
            condition.wind() as u8,
        ));
    }
    assert!(
        observed.len() >= 2,
        "weather fronts must visibly change outdoor conditions"
    );

    let days_per_season = u64::from(registries.core().calendar().days_per_month())
        * u64::from(registries.core().calendar().months_per_year() / 4);
    let season_span = registries.core().calendar().ticks_per_day() * days_per_season;
    let summer = weather_at(
        &registries,
        seed.value(),
        here,
        SimulationTick::new(season_span),
    );
    let winter = weather_at(
        &registries,
        seed.value(),
        here,
        SimulationTick::new(season_span * 3),
    );
    assert_eq!(summer.season(), Season::Summer);
    assert_eq!(winter.season(), Season::Winter);
    assert!(summer.ambient_temperature() > winter.ambient_temperature());
    assert!(winter.cooling_multiplier_ppm() > summer.cooling_multiplier_ppm());

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(assess_player_weather(&registries, &restored), Some(now));
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
}

#[test]
fn the_exposed_player_loses_exactly_the_weather_adjusted_warmth_projected_before_the_tick() {
    let (registries, mut state, _) = generated_player();
    let current = assess_player_weather(&registries, &state).unwrap();
    let forecast = assess_player_unheated_comfort_runway(&registries, &state).unwrap();
    assert_eq!(forecast.weather_changes_in(), Some(current.changes_in()));
    assert!(
        forecast.if_covered() > forecast.if_open(),
        "an actual roof must reduce exposure"
    );

    let before = state.survival().player().unwrap().thermal_comfort_ppm();
    let next_weather =
        assess_player_weather_at(&registries, &state, SimulationTick::new(1)).unwrap();
    let expected_loss = registries
        .survival()
        .physiology()
        .thermal_comfort()
        .exposure_loss_ppm_per_tick(ThermalExposure::GeneratedOpen {
            cooling_multiplier_ppm: next_weather.cooling_multiplier_ppm(),
        });
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut continuation = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
        let _ = advance_control_tick(&registries, &mut continuation).unwrap();
    }
    assert_eq!(state, continuation);
    assert_eq!(state.tick(), SimulationTick::new(1));
    assert_eq!(
        state.survival().player().unwrap().thermal_comfort_ppm(),
        before - expected_loss
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

/// A weather front is an actual 20 Hz gameplay boundary, not just a calendar
/// query. Begin a valid fixture at the last settled tick before that boundary
/// and advance it using the same controls as ordinary movement and survival.
#[test]
fn a_generated_world_weather_front_changes_survival_on_its_actual_boundary_and_replays() {
    let (registries, mut state, seed) = generated_player();
    crate::core::state::apply_clock_advance(
        &mut state,
        SimulationTick::new(WEATHER_FRONT_TICKS - 1),
    );
    let before_weather = assess_player_weather(&registries, &state).unwrap();
    assert_eq!(before_weather.changes_in(), TickSpan::new(1));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let comfort_before = state.survival().player().unwrap().thermal_comfort_ppm();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
        let _ = advance_control_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(state, replay);
    assert_eq!(state.tick(), SimulationTick::new(WEATHER_FRONT_TICKS));
    let after_weather = assess_player_weather(&registries, &state).unwrap();
    let position = state.logistics().player().unwrap().position();
    assert_eq!(
        after_weather,
        weather_at(&registries, seed.value(), position, state.tick())
    );
    assert_eq!(
        after_weather.changes_in(),
        TickSpan::new(WEATHER_FRONT_TICKS)
    );
    let expected_cooling = registries
        .survival()
        .physiology()
        .thermal_comfort()
        .exposure_loss_ppm_per_tick(ThermalExposure::GeneratedOpen {
            cooling_multiplier_ppm: after_weather.cooling_multiplier_ppm(),
        });
    assert_eq!(
        state.survival().player().unwrap().thermal_comfort_ppm(),
        comfort_before - expected_cooling,
        "the newly arrived front must affect the very tick at which it begins"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
