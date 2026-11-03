//! Natural pools change without player action while retaining exact water custody.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::calculate_explicit_energy_accounting;
use crate::fluid::calculate_fluid_volume_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::world::{
    ChunkCoord, WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};

fn actual_natural_pool() -> (Registries, AppState, super::super::FluidStoreId, VoxelCoord) {
    let registries = build_registries();
    for seed_number in 0..80_u64 {
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
        let surface = (crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE - 1)
            .find(|y| {
                state.world().block_at(VoxelCoord::new(0, *y, 0))
                    == Some(crate::world::BlockStateId::AIR)
                    && state.world().block_at(VoxelCoord::new(0, *y - 1, 0))
                        == Some(BLOCK_STATE_STONE)
            })
            .unwrap();
        let spawn = VoxelCoord::new(0, surface, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let candidate = state.fluid().spring_soil().next();
        if let Some((id, soil)) = candidate {
            return (registries, state, id, soil.position);
        }
    }
    panic!("sparse local wilderness never generated a natural water pocket");
}

#[test]
fn seasonal_fronts_change_actual_pool_volume_without_changing_world_water() {
    let (registries, mut state, id, _) = actual_natural_pool();
    let baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let original_heat = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap()
        .fluid_material_thermal();
    let original = state.fluid().get_store(id).unwrap().stored_volume();
    let mut saw_loss = false;
    let mut saw_return = false;
    let mut previous = original;
    for _ in 0..6_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let visible = state.fluid().get_store(id).unwrap().stored_volume();
        if visible < previous {
            saw_loss = true;
        } else if visible > previous {
            saw_return = true;
        }
        previous = visible;
        if saw_loss && saw_return {
            break;
        }
    }
    assert!(
        saw_loss,
        "dry fronts should move actual surface water into the catchment"
    );
    assert!(
        saw_return,
        "cool calm fronts should return previously retained water"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        baseline,
        "surface-to-soil exchange must not create or delete water"
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap()
            .fluid_material_thermal(),
        original_heat,
        "natural exchange must preserve the exact sensible and latent heat of soil water"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let serialized = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&serialized)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..440 {
        let _ = advance_tick(&registries, &mut loaded).unwrap();
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(loaded, state);
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), baseline);
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap()
            .fluid_material_thermal(),
        original_heat
    );
}

fn assert_returning_soil_water_mixes_exactly(
    warm_temperature: crate::core::quantity::Temperature,
    returned: Volume,
    expected_temperature: crate::core::quantity::Temperature,
) {
    use crate::content::FLUID_WATER;
    use crate::core::quantity::Temperature;
    use crate::core::state::apply_clock_advance;
    use crate::fluid::{
        add_fluid_store, add_fluid_store_with_contents_for_fixture,
        validate_transfer_fluid_between_local_stores,
    };
    use crate::logistics::{relocate_player_for_test, validate_place_fluid_store};

    let (registries, mut state, pool, position) = actual_natural_pool();
    let next_front_with = |state: &AppState, direction| {
        let current_front = state.tick().value() / WEATHER_FRONT_TICKS;
        (1..=500)
            .map(|step| SimulationTick::new((current_front + step) * WEATHER_FRONT_TICKS))
            .find(|&at| spring_moisture_flux(&registries, state, position, at) == direction)
            .expect("a regional weather front with the required moisture exchange exists")
    };
    let dry = next_front_with(&state, SpringMoistureFlux::Drying);
    apply_clock_advance(&mut state, dry);
    let plan = decide_natural_water_cycle(&registries, &state, dry).unwrap();
    apply_natural_water_cycle(&mut state, plan);
    let soil = state.fluid().spring_soil.get(&pool).unwrap();
    assert_eq!(soil.soaked, WATER_PER_FRONT);
    assert_eq!(
        soil.soaked_temperature,
        Some(Temperature::from_millikelvin(293_150))
    );

    // Decant cool surface water first, then replenish the empty pool with warm
    // water through the canonical player transfer. Groundwater stays cooler.
    relocate_player_for_test(&mut state, position);
    let remaining = state.fluid().get_store(pool).unwrap().stored_volume();
    let holding = add_fluid_store(&mut state, remaining).unwrap();
    validate_place_fluid_store(&state, holding, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ =
        validate_transfer_fluid_between_local_stores(&registries, &state, pool, holding, remaining)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert_eq!(
        state.fluid().get_store(pool).unwrap().stored_volume(),
        Volume::ZERO
    );

    let warm = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        WATER_PER_FRONT,
        FLUID_WATER,
        WATER_PER_FRONT,
        warm_temperature,
    )
    .unwrap();
    validate_place_fluid_store(&state, warm, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let _ = validate_transfer_fluid_between_local_stores(
        &registries,
        &state,
        warm,
        pool,
        WATER_PER_FRONT,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state
            .fluid()
            .get_store(pool)
            .unwrap()
            .contents()
            .unwrap()
            .temperature(),
        warm_temperature
    );
    let volume_before = calculate_fluid_volume_accounting(&state).unwrap();
    let heat_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap()
        .fluid_material_thermal();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let return_front = next_front_with(&state, SpringMoistureFlux::Returning);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for actor in [&mut state, &mut replay] {
        apply_clock_advance(actor, return_front);
        let plan = decide_natural_water_cycle(&registries, actor, return_front).unwrap();
        apply_natural_water_cycle(actor, plan);
    }
    assert_eq!(
        state, replay,
        "restored catchment temperature must determine the same return"
    );
    let after = state.fluid().get_store(pool).unwrap().contents().unwrap();
    assert_eq!(
        after.volume(),
        WATER_PER_FRONT.checked_add(returned).unwrap()
    );
    assert_eq!(after.temperature(), expected_temperature);
    let soil = state.fluid().spring_soil.get(&pool).unwrap();
    let residual = WATER_PER_FRONT.checked_sub(returned).unwrap();
    assert_eq!(soil.soaked, residual);
    assert_eq!(
        soil.soaked_temperature,
        (!residual.is_zero()).then_some(Temperature::from_millikelvin(293_150))
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        volume_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap()
            .fluid_material_thermal(),
        heat_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    if !residual.is_zero() {
        // The same mismatched water must also admit a finite, exact return to
        // the soil on a later drying front. Neither direction can round heat.
        let dry_front = next_front_with(&state, SpringMoistureFlux::Drying);
        for actor in [&mut state, &mut replay] {
            apply_clock_advance(actor, dry_front);
            let plan = decide_natural_water_cycle(&registries, actor, dry_front).unwrap();
            apply_natural_water_cycle(actor, plan);
        }
        assert_eq!(state, replay);
        let water = state.fluid().get_store(pool).unwrap().contents().unwrap();
        assert_eq!(water.volume(), WATER_PER_FRONT);
        assert_eq!(water.temperature(), expected_temperature);
        let soil = state.fluid().spring_soil.get(&pool).unwrap();
        assert_eq!(soil.soaked, WATER_PER_FRONT);
        assert_eq!(
            soil.soaked_temperature,
            Some(Temperature::from_millikelvin(293_151))
        );
        assert_eq!(
            calculate_fluid_volume_accounting(&state).unwrap(),
            volume_before
        );
        assert_eq!(
            calculate_explicit_energy_accounting(&registries, &state)
                .unwrap()
                .fluid_material_thermal(),
            heat_before
        );
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn returning_soil_water_mixes_with_warmer_replenished_pool_without_creating_heat() {
    assert_returning_soil_water_mixes_exactly(
        crate::core::quantity::Temperature::from_millikelvin(303_150),
        WATER_PER_FRONT,
        crate::core::quantity::Temperature::from_millikelvin(298_150),
    );
}

#[test]
fn mismatched_spring_temperatures_transfer_exact_partial_batches_in_both_directions() {
    assert_returning_soil_water_mixes_exactly(
        crate::core::quantity::Temperature::from_millikelvin(293_153),
        Volume::from_microliters(125_000),
        crate::core::quantity::Temperature::from_millikelvin(293_152),
    );
}

#[test]
fn spring_exchange_chooses_largest_exact_batch_with_bounded_arithmetic() {
    use crate::core::quantity::Temperature;

    let cold = Temperature::from_millikelvin(293_150);
    for stored in 1..=32_u64 {
        for delta in 1..=32_u32 {
            let warmer = Temperature::from_millikelvin(cold.millikelvin() + delta);
            for limit in 1..=32_u64 {
                let expected = (1..=limit)
                    .rev()
                    .find(|&amount| {
                        (u128::from(stored) * u128::from(delta))
                            .is_multiple_of(u128::from(stored + amount))
                    })
                    .map(Volume::from_microliters);
                assert_eq!(
                    largest_exact_mixing_exchange(
                        (Volume::from_microliters(stored), cold),
                        warmer,
                        Volume::from_microliters(limit),
                    ),
                    expected,
                    "stored={stored}, delta={delta}, limit={limit}"
                );
            }
        }
    }
    assert_eq!(
        largest_exact_mixing_exchange(
            (Volume::from_microliters(4), cold),
            Temperature::from_millikelvin(293_153),
            Volume::from_microliters(3),
        ),
        Some(Volume::from_microliters(2)),
    );
    assert_eq!(
        largest_exact_mixing_exchange(
            (Volume::from_microliters(u64::MAX - 2), cold),
            Temperature::from_millikelvin(293_151),
            Volume::from_microliters(2),
        ),
        None,
    );
}

#[test]
fn tampered_catchment_coordinates_and_impossible_storage_are_rejected() {
    let (registries, state, id, _) = actual_natural_pool();
    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let key = id.value().to_string();
    let mut misplaced = save.clone();
    misplaced["state"]["systems"]["fluid"]["spring_soil"][&key]["position"] =
        serde_json::to_value(VoxelCoord::new(900, 4, 900)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(misplaced).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut nonexistent_heat = save.clone();
    nonexistent_heat["state"]["systems"]["fluid"]["spring_soil"][&key]["soaked_temperature"] =
        serde_json::json!(293_150);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(nonexistent_heat).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut overfilled = save.clone();
    overfilled["state"]["systems"]["fluid"]["spring_soil"][&key]["soaked"] =
        serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(overfilled).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut old_shape = save;
    old_shape["state"]["systems"]["fluid"]["spring_soil"][&key]
        .as_object_mut()
        .unwrap()
        .remove("soaked_temperature");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(old_shape).is_err());
}

#[test]
fn trusted_load_rejects_changed_seeded_spring_capacity() {
    use crate::core::state::StateValidationError;
    use crate::fluid::FluidValidationError;
    use crate::persistence::LoadError;

    let (registries, state, pool, position) = actual_natural_pool();
    let record = state.fluid().get_store(pool).unwrap();
    assert!(crate::world::seeded_spring_matches(
        state.world().generation_profile().unwrap(),
        position,
        record.capacity(),
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let key = pool.value().to_string();
    forged["state"]["systems"]["fluid"]["records"][&key]["capacity"] =
        serde_json::json!(record.capacity().microliters() + 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Fluid(
            FluidValidationError::InvalidNaturalSpring { store: pool }
        )))
    );
}

#[test]
fn trusted_load_rejects_second_natural_spring_in_same_seeded_district() {
    use crate::core::state::StateValidationError;
    use crate::fluid::{FluidValidationError, add_fluid_store};
    use crate::logistics::validate_place_fluid_store;
    use crate::persistence::LoadError;

    let (registries, mut state, pool, position) = actual_natural_pool();
    let capacity = state.fluid().get_store(pool).unwrap().capacity();
    let ordinary_vessel = add_fluid_store(&mut state, capacity).unwrap();
    validate_place_fluid_store(&state, ordinary_vessel, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let spring_key = pool.value().to_string();
    let ordinary_key = ordinary_vessel.value().to_string();
    let copied_soil = forged["state"]["systems"]["fluid"]["spring_soil"][&spring_key].clone();
    forged["state"]["systems"]["fluid"]["spring_soil"][&ordinary_key] = copied_soil;
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Fluid(
            FluidValidationError::InvalidNaturalSpring {
                store: ordinary_vessel,
            }
        )))
    );
}

#[test]
fn a_built_roof_stops_weather_exchange_without_erasing_the_water() {
    let (registries, mut state, id, position) = actual_natural_pool();
    let roof = VoxelCoord::new(position.x(), position.y() + 3, position.z());
    assert_eq!(
        state.world().block_at(roof),
        Some(crate::world::BlockStateId::AIR),
        "the generated pool needs a genuine sky-facing column"
    );
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        roof,
        BLOCK_STATE_STONE,
    );
    let standing = state.fluid().get_store(id).unwrap().stored_volume();
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_800 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(id).unwrap().stored_volume(),
        standing
    );
    assert_eq!(
        state.fluid().spring_soil.get(&id).unwrap().soaked,
        Volume::ZERO
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn natural_pool_is_ground_bound_not_an_attachable_vessel() {
    use crate::fluid::{FluidSupportError, validate_mount_fluid_store};
    use crate::structural::StructuralElementId;

    let (registries, mut state, id, position) = actual_natural_pool();
    crate::logistics::relocate_player_for_test(&mut state, position);
    let original = state.clone();
    assert_eq!(
        validate_mount_fluid_store(&registries, &state, id, StructuralElementId::new(1),).err(),
        Some(FluidSupportError::NaturalPoolIsNotAVessel { store: id }),
    );
    assert_eq!(state, original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn exhausted_spring_revision_rejects_whole_weather_front_without_moving_water_or_time() {
    use crate::core::state::apply_clock_advance;
    use crate::simulation::TickError;

    let (registries, mut state, pool, position) = actual_natural_pool();
    let dry = (1..=500)
        .map(|step| SimulationTick::new(step * WEATHER_FRONT_TICKS))
        .find(|&at| {
            spring_moisture_flux(&registries, &state, position, at) == SpringMoistureFlux::Drying
        })
        .expect("seeded spring must experience a drying front");
    apply_clock_advance(&mut state, SimulationTick::new(dry.value() - 1));

    // Construct the exceptional owner state through its serialized fields;
    // ordinary simulation still uses the canonical revision-checked path.
    let mut saved = serde_json::to_value(state.fluid()).unwrap();
    saved["revision"] = serde_json::json!(u64::MAX);
    let mut exhausted: crate::fluid::FluidState = serde_json::from_value(saved).unwrap();
    exhausted.rebuild_derived_indexes();
    *state.fluid_state_mut() = exhausted;

    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::FluidRevisionExhausted),
    );
    assert_eq!(
        state, before,
        "rejection must roll back every prior slow-tick owner"
    );
    assert_eq!(
        state.fluid().get_store(pool),
        before.fluid().get_store(pool)
    );
}
