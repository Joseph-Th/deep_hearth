//! Fast slow-tick stepping must exactly match seventy-two neutral control frames.

use super::*;
use crate::core::state::validate_loaded_state;
use crate::world::WEATHER_FRONT_TICKS;

fn generated_grounded_world(seed_value: u64) -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(seed_value),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let spawn = (WORLD_MIN_Y + 1..WORLD_MAX_Y_EXCLUSIVE - 1)
        .rev()
        .map(|y| VoxelCoord::new(0, y, 0))
        .find(|&at| state.world().has_standing_clearance(registries.world(), at))
        .expect("seeded world must provide standing ground in its central column");
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    (registries, state)
}

#[test]
fn generated_woodland_water_and_senescence_match_neutral_control_replay() {
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::world::validate_generate_player_neighborhood;

    let mut exercised = 0;
    for seed in 0_u64..350 {
        let (registries, mut natural) = generated_grounded_world(seed);
        validate_generate_player_neighborhood(&registries, &natural)
            .unwrap()
            .expect("one generated central column leaves neighboring wilderness to discover")
            .commit(&mut natural)
            .unwrap();
        if natural.surface().perennial_resources().next().is_none()
            || natural.fluid().spring_soil().next().is_none()
        {
            continue;
        }
        exercised += 1;
        let baseline = calculate_matter_accounting(&natural).unwrap().total();
        // The second front is the first authored 2,400-tick fruit-senescence boundary.
        for at in [WEATHER_FRONT_TICKS - 1, 2_399] {
            for elapsed in [0_u64, 37, CONTROL_TICKS_PER_SIMULATION_TICK - 1] {
                let mut before = natural.clone();
                apply_clock_advance(&mut before, SimulationTick::new(at));
                for _ in 0..elapsed {
                    assert!(
                        advance_control_tick(&registries, &mut before)
                            .unwrap()
                            .simulation()
                            .is_none()
                    );
                }
                assert_eq!(validate_loaded_state(&registries, &before), Ok(()));
                let serialized =
                    serde_json::to_vec(&SaveEnvelope::new(&registries, &before)).unwrap();
                let mut control = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
                    .unwrap()
                    .into_state(&registries)
                    .unwrap();
                let mut fast = before;
                let result = advance_tick(&registries, &mut fast).unwrap_or_else(|error| {
                    panic!("seed={seed}, tick={at}, elapsed={elapsed}: {error}")
                });
                let mut control_result = None;
                for _ in elapsed..CONTROL_TICKS_PER_SIMULATION_TICK {
                    let outcome = advance_control_tick(&registries, &mut control).unwrap();
                    if let Some(simulation) = outcome.simulation() {
                        control_result = Some(simulation.clone());
                    }
                }
                assert_eq!(
                    control_result.as_ref(),
                    Some(&result),
                    "woodland outcome parity seed={seed} tick={at} elapsed={elapsed}"
                );
                assert_eq!(
                    control, fast,
                    "woodland state parity seed={seed} tick={at} elapsed={elapsed}"
                );
                assert_eq!(
                    calculate_matter_accounting(&fast).unwrap().total(),
                    baseline
                );
                assert_eq!(validate_loaded_state(&registries, &fast), Ok(()));
            }
        }
        if exercised == 3 {
            break;
        }
    }
    assert_eq!(
        exercised, 3,
        "three seeded woodland states must include natural springs"
    );
}

#[test]
fn neutral_control_steps_match_fast_forward_across_seeds_and_weather_front() {
    for seed in [0, 7, 41] {
        let (registries, initial) = generated_grounded_world(seed);
        for initial_tick in [0, WEATHER_FRONT_TICKS - 1] {
            for elapsed in [0_u64, 17, CONTROL_TICKS_PER_SIMULATION_TICK - 1] {
                let mut before = initial.clone();
                let at = crate::core::time::SimulationTick::new(initial_tick);
                apply_clock_advance(&mut before, at);
                for _ in 0..elapsed {
                    let frame = advance_control_tick(&registries, &mut before)
                        .unwrap_or_else(|error| panic!("seed={seed}, tick={initial_tick}, elapsed={elapsed}: initial control: {error}"));
                    assert!(frame.simulation().is_none());
                }
                assert_eq!(validate_loaded_state(&registries, &before), Ok(()));
                let mut slow = before.clone();
                let mut control = before;
                let fast = advance_tick(&registries, &mut slow).unwrap_or_else(|error| {
                    panic!("seed={seed}, tick={initial_tick}, elapsed={elapsed}: fast: {error}")
                });
                let mut actual = None;
                for _ in elapsed..CONTROL_TICKS_PER_SIMULATION_TICK {
                    let frame = advance_control_tick(&registries, &mut control)
                        .unwrap_or_else(|error| panic!("seed={seed}, tick={initial_tick}, elapsed={elapsed}: control: {error}"));
                    if let Some(simulation) = frame.simulation() {
                        actual = Some(simulation.clone());
                    }
                }
                assert_eq!(
                    actual.as_ref(),
                    Some(&fast),
                    "seed={seed}, tick={initial_tick}, elapsed={elapsed}: slow outcome diverges"
                );
                assert_eq!(
                    control, slow,
                    "seed={seed}, tick={initial_tick}, elapsed={elapsed}: durable state diverges"
                );
                assert_eq!(validate_loaded_state(&registries, &control), Ok(()));
            }
        }
    }
}
