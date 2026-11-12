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
fn generated_air_requires_real_vertical_world_truth_without_mutation() {
    let (registries, state, _) = generated_player();
    let position = state.logistics().player().unwrap().position();
    let before = state.clone();
    assert_eq!(
        generated_air_at(&registries, &state, position, state.tick()),
        assess_player_weather(&registries, &state)
    );
    for y in [
        super::super::WORLD_MIN_Y - 1,
        super::super::WORLD_MAX_Y_EXCLUSIVE,
    ] {
        assert_eq!(
            generated_air_at(
                &registries,
                &state,
                VoxelCoord::new(position.x(), y, position.z()),
                state.tick(),
            ),
            None,
            "a generated horizontal chunk does not authorize climate outside world height"
        );
    }
    assert_eq!(
        generated_air_at(
            &registries,
            &state,
            VoxelCoord::new(256, position.y(), 256),
            state.tick(),
        ),
        None,
        "a remote ungenerated chunk cannot inherit local temperature"
    );
    assert_eq!(state, before);
}

#[test]
fn dead_player_cannot_inspect_local_weather_habitat_or_cover_until_respawn() {
    use crate::player::validate_respawn_player;
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let (registries, mut state, _) = generated_player();
    let living_weather = assess_player_weather(&registries, &state).unwrap();
    let habitat = crate::world::assess_player_habitat(&state);
    let cover = crate::logistics::assess_player_overhead_cover(&registries, &state);
    assert!(habitat.is_some());
    assert!(cover.is_some());
    let before = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::ZERO,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(assess_player_weather(&registries, &state), None);
    assert_eq!(crate::world::assess_player_habitat(&state), None);
    assert_eq!(
        crate::logistics::assess_player_overhead_cover(&registries, &state),
        None
    );

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(assess_player_weather(&registries, &replay), None);
    assert_eq!(crate::world::assess_player_habitat(&replay), None);
    assert_eq!(
        crate::logistics::assess_player_overhead_cover(&registries, &replay),
        None
    );
    assert_eq!(state, replay);
    let _ = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_respawn_player(&registries, &replay)
        .unwrap()
        .commit(&mut replay)
        .unwrap();
    assert_eq!(state, replay);
    assert_eq!(crate::world::assess_player_habitat(&state), habitat);
    assert_eq!(
        crate::logistics::assess_player_overhead_cover(&registries, &state),
        cover
    );
    assert_eq!(
        assess_player_weather(&registries, &state),
        Some(living_weather)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
    assert!(
        (COLD_EXPOSURE_MINIMUM_COOLING_PPM..=MAXIMUM_COOLING_PPM)
            .contains(&now.cooling_multiplier_ppm())
    );
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
        forecast.if_covered().unwrap() > forecast.if_open().unwrap(),
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

#[test]
fn upwind_terrain_reduces_actual_cooling_and_the_player_forecast_without_changing_weather() {
    use crate::core::state::apply_clock_advance;
    use crate::survival::PlayerWindShelter;
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5745_4154_4845_5253);
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
    let position = VoxelCoord::new(
        8,
        super::super::generation::terrain_height(seed, 8, 8) + 1,
        8,
    );
    assert!(
        state
            .world()
            .has_standing_clearance(registries.world(), position)
    );
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let front = (0..20)
        .map(|index| SimulationTick::new(index * WEATHER_FRONT_TICKS))
        .find(|&at| {
            weather_at(&registries, seed.value(), position, at)
                .wind_direction()
                .is_some()
        })
        .expect("local weather has a windy front");
    apply_clock_advance(&mut state, front);
    let weather = assess_player_weather(&registries, &state).unwrap();
    let (dx, dz) = weather.wind_direction().unwrap().delta();
    let lower = VoxelCoord::new(position.x() - dx, position.y(), position.z() - dz);
    let upper = VoxelCoord::new(lower.x(), lower.y() + 1, lower.z());
    assert!(state.world().tree_trunk_at(lower).is_none());
    assert!(state.world().tree_trunk_at(upper).is_none());
    // Establish a clear upwind air column before comparing the same physical
    // actor against a one- and two-block natural stone windbreak.
    for cell in [lower, upper] {
        set_generated_block_for_test(registries.world(), &mut state, cell, BlockStateId::AIR);
    }

    let mut outcomes = Vec::new();
    for (blocked, shelter) in [
        (0, PlayerWindShelter::Exposed),
        (1, PlayerWindShelter::Partial),
        (2, PlayerWindShelter::Sheltered),
    ] {
        let mut actor = state.clone();
        if blocked >= 1 {
            set_generated_block_for_test(registries.world(), &mut actor, lower, BLOCK_STATE_STONE);
        }
        if blocked == 2 {
            set_generated_block_for_test(registries.world(), &mut actor, upper, BLOCK_STATE_STONE);
        }
        assert_eq!(assess_player_weather(&registries, &actor), Some(weather));
        assert_eq!(
            local_airflow_at(&registries, &actor, position, front).map(PlayerWeather::wind),
            (blocked == 0).then_some(weather.wind()),
            "ground-level sound and scent must see the same upwind barrier as player warmth"
        );
        assert_eq!(
            local_airflow_at(
                &registries,
                &actor,
                VoxelCoord::new(position.x(), position.y() + 1, position.z()),
                front,
            )
            .map(PlayerWeather::wind),
            (blocked < 2).then_some(weather.wind()),
            "the open head-level inlet must remain windy behind a one-block windbreak"
        );
        let forecast = assess_player_unheated_comfort_runway(&registries, &actor).unwrap();
        assert_eq!(forecast.wind_shelter(), Some(shelter));
        assert_eq!(
            forecast.overhead_cover(),
            crate::logistics::PlayerOverheadCoverAssessment::Open
        );
        let cooling = weather.felt_cooling_multiplier_ppm(&registries, &actor);
        let before = actor.survival().player().unwrap().thermal_comfort_ppm();
        let expected_loss = registries
            .survival()
            .physiology()
            .thermal_comfort()
            .exposure_loss_ppm_per_tick(ThermalExposure::GeneratedOpen {
                cooling_multiplier_ppm: cooling,
            });
        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &actor)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut actor).unwrap();
            let _ = advance_control_tick(&registries, &mut replay).unwrap();
        }
        assert_eq!(actor, replay);
        assert_eq!(
            actor.survival().player().unwrap().thermal_comfort_ppm(),
            before - expected_loss,
            "the actual next survival tick must use the same windbreak as the forecast"
        );
        assert_eq!(validate_loaded_state(&registries, &actor), Ok(()));
        outcomes.push((cooling, expected_loss, forecast.current().unwrap()));
    }
    assert_eq!(outcomes[0].0, weather.cooling_multiplier_ppm());
    assert!(outcomes[0].0 > outcomes[1].0 && outcomes[1].0 > outcomes[2].0);
    assert!(outcomes[0].1 > outcomes[1].1 && outcomes[1].1 > outcomes[2].1);
    assert!(outcomes[0].2 < outcomes[1].2 && outcomes[1].2 < outcomes[2].2);
}

#[test]
fn covered_player_needs_a_real_wind_inlet_at_each_exposed_body_level() {
    use crate::core::state::apply_clock_advance;
    use crate::survival::PlayerWindShelter;
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5745_4154_4845_5253);
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
    let position = VoxelCoord::new(
        8,
        super::super::generation::terrain_height(seed, 8, 8) + 1,
        8,
    );
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let front = (0..30)
        .map(|index| SimulationTick::new(index * WEATHER_FRONT_TICKS))
        .find(|&at| {
            let weather = weather_at(&registries, seed.value(), position, at);
            weather.wind_direction().is_some()
                && weather.ambient_temperature().millikelvin() < 293_150
        })
        .expect("a windy cool front exists");
    apply_clock_advance(&mut state, front);
    let weather = assess_player_weather(&registries, &state).unwrap();
    let (dx, dz) = weather.wind_direction().unwrap().delta();
    let upwind = VoxelCoord::new(position.x() - dx, position.y(), position.z() - dz);
    let upwind_head = VoxelCoord::new(upwind.x(), upwind.y() + 1, upwind.z());
    let upwind_roof = VoxelCoord::new(upwind.x(), upwind.y() + 2, upwind.z());
    let roof = VoxelCoord::new(position.x(), position.y() + 2, position.z());
    assert!(state.world().tree_trunk_at(upwind).is_none());
    assert!(state.world().tree_trunk_at(upwind_head).is_none());
    for cell in [upwind, upwind_head] {
        set_generated_block_for_test(registries.world(), &mut state, cell, BlockStateId::AIR);
    }
    for cell in [roof, upwind_roof] {
        set_generated_block_for_test(registries.world(), &mut state, cell, BLOCK_STATE_STONE);
    }
    assert!(!super::super::sky::has_open_sky(
        &registries,
        &state,
        position
    ));
    assert_eq!(local_airflow_at(&registries, &state, position, front), None);

    let mut rates = Vec::new();
    for (open_inlet, lower_wall, shelter, sheltered_levels) in [
        (false, false, PlayerWindShelter::Sheltered, 2),
        (true, true, PlayerWindShelter::Partial, 1),
        (true, false, PlayerWindShelter::Exposed, 0),
    ] {
        let mut actor = state.clone();
        if open_inlet {
            set_generated_block_for_test(
                registries.world(),
                &mut actor,
                upwind_roof,
                BlockStateId::AIR,
            );
        }
        if lower_wall {
            set_generated_block_for_test(registries.world(), &mut actor, upwind, BLOCK_STATE_STONE);
        }
        assert_eq!(
            local_airflow_at(&registries, &actor, position, front).is_some(),
            open_inlet && !lower_wall
        );
        assert_eq!(
            local_airflow_at(
                &registries,
                &actor,
                VoxelCoord::new(position.x(), position.y() + 1, position.z()),
                front,
            )
            .is_some(),
            open_inlet
        );
        assert_eq!(
            weather.sheltered_body_levels(&registries, &actor),
            sheltered_levels
        );
        assert_eq!(
            assess_player_unheated_comfort_runway(&registries, &actor)
                .unwrap()
                .wind_shelter(),
            Some(shelter)
        );
        assert_eq!(assess_player_weather(&registries, &actor), Some(weather));
        let cooling = weather.felt_cooling_multiplier_ppm(&registries, &actor);
        if !open_inlet {
            let celsius = (weather.ambient_temperature().millikelvin() as i32 - 273_150) / 1_000;
            assert_eq!(cooling, air_cooling_multiplier_ppm(celsius, 0));
        }
        let before = actor.survival().player().unwrap().thermal_comfort_ppm();
        let loss = registries
            .survival()
            .physiology()
            .thermal_comfort()
            .exposure_loss_ppm_per_tick(ThermalExposure::GeneratedOverheadCover {
                cooling_multiplier_ppm: cooling,
            });
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &actor)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
            let _ = advance_control_tick(&registries, &mut actor).unwrap();
            let _ = advance_control_tick(&registries, &mut replay).unwrap();
        }
        assert_eq!(actor, replay);
        assert_eq!(
            actor.survival().player().unwrap().thermal_comfort_ppm(),
            before - loss
        );
        assert_eq!(validate_loaded_state(&registries, &actor), Ok(()));
        rates.push(cooling);
    }
    assert!(rates[0] < rates[1] && rates[1] < rates[2]);
}

#[test]
fn airborne_eye_above_one_cell_windbreak_uses_actual_body_height() {
    use crate::core::state::apply_clock_advance;
    use crate::logistics::PlayerControlInput;
    use crate::simulation::advance_control_tick_with_input;
    use crate::survival::PlayerWindShelter;
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5745_4154_4845_5253);
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
    let standing = VoxelCoord::new(
        8,
        super::super::generation::terrain_height(seed, 8, 8) + 1,
        8,
    );
    validate_admit_player(&registries, &state, standing)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let front = (0..30)
        .map(|index| SimulationTick::new(index * WEATHER_FRONT_TICKS))
        .find(|&at| {
            let weather = weather_at(&registries, seed.value(), standing, at);
            weather.wind_direction().is_some()
                && weather.ambient_temperature().millikelvin() < 293_150
        })
        .expect("a cool windy front exists");
    apply_clock_advance(&mut state, front);
    let weather = assess_player_weather(&registries, &state).unwrap();
    let (dx, dz) = weather.wind_direction().unwrap().delta();
    let upwind = VoxelCoord::new(standing.x() - dx, standing.y(), standing.z() - dz);
    // A one-cell upwind barrier covers standing eyes, but an airborne eye
    // clears it while the feet remain within their original ground voxel.
    for y in standing.y()..=standing.y() + 2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(upwind.x(), y, upwind.z()),
            if y == standing.y() + 1 {
                BLOCK_STATE_STONE
            } else {
                BlockStateId::AIR
            },
        );
    }
    assert_eq!(
        super::super::player_eye_voxel(&registries, &state)
            .unwrap()
            .y(),
        standing.y() + 1
    );
    assert_eq!(weather.sheltered_body_levels(&registries, &state), 1);
    assert_eq!(
        assess_player_unheated_comfort_runway(&registries, &state)
            .unwrap()
            .wind_shelter(),
        Some(PlayerWindShelter::Partial)
    );

    let _ = advance_control_tick_with_input(
        &registries,
        &mut state,
        PlayerControlInput::NEUTRAL.with_jump(true),
    )
    .unwrap();
    for _ in 0..6 {
        if super::super::player_eye_voxel(&registries, &state)
            .is_some_and(|eye| eye.y() == standing.y() + 2)
        {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.logistics().player().unwrap().position(), standing);
    assert_eq!(
        super::super::player_eye_voxel(&registries, &state)
            .unwrap()
            .y(),
        standing.y() + 2
    );
    assert_eq!(weather.sheltered_body_levels(&registries, &state), 0);
    assert_eq!(
        weather.felt_cooling_multiplier_ppm(&registries, &state),
        weather.cooling_multiplier_ppm()
    );
    assert_eq!(
        assess_player_unheated_comfort_runway(&registries, &state)
            .unwrap()
            .wind_shelter(),
        Some(PlayerWindShelter::Exposed)
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, restored);
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
}

#[test]
fn outdoor_cooling_tapers_monotonically_into_neutral_summer_air() {
    for wind in [
        WindCondition::Calm,
        WindCondition::Breeze,
        WindCondition::Gusting,
    ] {
        let surcharge = wind_cooling_ppm(wind);
        let mut previous = air_cooling_multiplier_ppm(19, surcharge);
        assert!(previous >= COLD_EXPOSURE_MINIMUM_COOLING_PPM);
        for temperature in 20..=30 {
            let current = air_cooling_multiplier_ppm(temperature, surcharge);
            assert!(
                current <= previous,
                "warmer air cannot increase cold exposure"
            );
            previous = current;
        }
        assert!(air_cooling_multiplier_ppm(25, surcharge) > 0);
        assert_eq!(air_cooling_multiplier_ppm(26, surcharge), 0);
        assert_eq!(air_cooling_multiplier_ppm(30, surcharge), 0);
    }
}

#[test]
fn summer_heat_prevents_unheated_cold_injury_and_restores_chilled_comfort() {
    use crate::core::state::apply_clock_advance;
    use crate::survival::{Vitality, player_record_with_thermal_comfort};

    let (registries, mut state, seed) = generated_player();
    let position = state.logistics().player().unwrap().position();
    let calendar = registries.core().calendar();
    let season_days =
        u64::from(calendar.days_per_month()) * u64::from(calendar.months_per_year() / 4);
    let summer_start = calendar.ticks_per_day() * season_days;
    let summer_afternoon = summer_start + calendar.ticks_per_day() / 4;
    let hot_front = (0..calendar.ticks_per_day() / WEATHER_FRONT_TICKS)
        .map(|offset| SimulationTick::new(summer_afternoon + offset * WEATHER_FRONT_TICKS))
        .find(|&at| {
            let weather = weather_at(&registries, seed.value(), position, at);
            weather.phase() == DayPhase::Afternoon
                && weather.ambient_temperature().millikelvin() >= 299_150
        })
        .expect("a summer afternoon produces hot air at the player");
    apply_clock_advance(&mut state, hot_front);
    let weather = assess_player_weather(&registries, &state).unwrap();
    assert!(weather.ambient_temperature().millikelvin() >= 299_150);
    assert_eq!(weather.cooling_multiplier_ppm(), 0);
    assert_eq!(
        assess_player_unheated_comfort_runway(&registries, &state)
            .unwrap()
            .current(),
        None,
        "hot outdoor air must not advertise a countdown to cold"
    );

    let original = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            original.metabolic_energy(),
            original.hydration(),
            Vitality::from_parts_per_million_unchecked(500_000),
            original.nutrition(),
            original.vitality_recovery_remainder(),
            0,
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
    let after = state.survival().player().unwrap();
    assert!(
        after.thermal_comfort_ppm() > 0,
        "hot ambient air should warm an already cold person without a fire"
    );
    assert!(
        after.vitality().parts_per_million() >= 500_000,
        "warm outdoor air cannot continue applying cold injury"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
