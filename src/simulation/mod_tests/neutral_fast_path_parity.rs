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
fn generated_scout_return_cargo_matches_neutral_control_boundary() {
    use crate::content::{FORM_CHIP, FORM_FOOD, MATERIAL_STONE, MATERIAL_WILD_NUTS};
    use crate::core::quantity::{Mass, Temperature};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
    };
    use crate::world_generation::validate_generate_initial_wilderness;

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
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
    let surface = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(8, y, 8)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let spawn = VoxelCoord::new(8, surface + 1, 8);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let junction = state
        .surface()
        .resources()
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD))
        .expect("generated scout work junction contains finite wild nuts")
        .position();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(crate::logistics::player_body_is_grounded(
        &registries,
        &state
    ));

    let stone = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        junction,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let ingress = validate_create_surface_resources(&registries, &state, [stone]).unwrap();
    let cargo = ingress.first_resource_id();
    ingress.commit(&mut state).unwrap();
    let baseline = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    for due_before in [179_u64, 184, 189] {
        while state.tick().value() < due_before {
            let _ = advance_tick(&registries, &mut state).unwrap();
        }
        for elapsed in [0_u64, 31, CONTROL_TICKS_PER_SIMULATION_TICK - 1] {
            let mut before = state.clone();
            for _ in 0..elapsed {
                assert!(
                    advance_control_tick(&registries, &mut before)
                        .unwrap()
                        .simulation()
                        .is_none()
                );
            }
            let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &before)).unwrap();
            let mut control = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
                .unwrap()
                .into_state(&registries)
                .unwrap();
            let mut fast = before;
            let expected = advance_tick(&registries, &mut fast).unwrap();
            let mut actual = None;
            for _ in elapsed..CONTROL_TICKS_PER_SIMULATION_TICK {
                let frame = advance_control_tick(&registries, &mut control).unwrap();
                if let Some(simulation) = frame.simulation() {
                    actual = Some(simulation.clone());
                }
            }
            assert_eq!(
                actual.as_ref(),
                Some(&expected),
                "scout outcome parity at tick {due_before}, elapsed {elapsed}"
            );
            assert_eq!(
                fast, control,
                "scout state parity at tick {due_before}, elapsed {elapsed}"
            );
            assert_eq!(
                calculate_matter_accounting(&fast).unwrap().total(),
                baseline
            );
            assert_eq!(validate_loaded_state(&registries, &fast), Ok(()));
            if due_before == 184 {
                assert_ne!(
                    fast.surface().get(cargo).unwrap().position(),
                    junction,
                    "the real returning scout must have carried the finite drop home"
                );
            }
        }
    }
}

#[test]
fn generated_woodland_water_and_senescence_match_neutral_control_replay() {
    use crate::fluid::{calculate_fluid_volume_accounting, decide_natural_water_cycle};
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
        // Require a front that actually transfers finite surface water into
        // spring-soil custody, rather than merely exercising an inert spring.
        let Some(water_front) = (1..=100_u64)
            .map(|step| SimulationTick::new(step * WEATHER_FRONT_TICKS))
            .find(|&front| {
                decide_natural_water_cycle(&registries, &natural, front)
                    .unwrap()
                    .is_some()
            })
        else {
            continue;
        };
        // Move this source through one real drying exchange, then select an
        // actual returning front for stored soil water. Each prospective
        // return must be observable at its persisted pool, not a no-op front.
        let mut dried = natural.clone();
        apply_clock_advance(&mut dried, water_front);
        let drying = decide_natural_water_cycle(&registries, &dried, water_front).unwrap();
        assert!(drying.is_some());
        crate::fluid::apply_natural_water_cycle(&mut dried, drying);
        assert_eq!(validate_loaded_state(&registries, &dried), Ok(()));
        let returning = (water_front.value() / WEATHER_FRONT_TICKS + 1..=150_u64)
            .map(|step| SimulationTick::new(step * WEATHER_FRONT_TICKS))
            .find_map(|front| {
                dried.fluid().spring_soil().find_map(|(store, soil)| {
                    (!soil.soaked.is_zero()
                        && crate::world::spring_moisture_flux(
                            &registries,
                            &dried,
                            soil.position,
                            front,
                        ) == crate::world::SpringMoistureFlux::Returning)
                        .then_some((front, store))
                })
            });
        let Some((return_front, return_store)) = returning else {
            continue;
        };
        exercised += 1;
        let baseline = calculate_matter_accounting(&natural).unwrap().total();
        let water_baseline = calculate_fluid_volume_accounting(&natural).unwrap();
        // Also test first fruit senescence, initial drying and later return.
        for (initial, at, return_pool) in [
            (&natural, WEATHER_FRONT_TICKS - 1, None),
            (&natural, 2_399, None),
            (&natural, water_front.value() - 1, None),
            (&dried, return_front.value() - 1, Some(return_store)),
        ] {
            for elapsed in [0_u64, 37, CONTROL_TICKS_PER_SIMULATION_TICK - 1] {
                let mut before = initial.clone();
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
                assert_eq!(
                    calculate_fluid_volume_accounting(&fast).unwrap(),
                    water_baseline,
                    "spring water must remain conserved across fast and control paths"
                );
                if at + 1 == water_front.value() {
                    assert_ne!(
                        fast.fluid(),
                        natural.fluid(),
                        "the selected front must actually exchange natural water"
                    );
                }
                if let Some(pool) = return_pool {
                    assert!(
                        fast.fluid().get_store(pool).unwrap().stored_volume()
                            > initial.fluid().get_store(pool).unwrap().stored_volume(),
                        "a returning front must restore real water to the selected pool"
                    );
                }
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
