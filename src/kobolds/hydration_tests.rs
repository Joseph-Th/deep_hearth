//! End-to-end resident water visits, depleted sources, local cues and trusted replay.

use super::*;

use crate::content::build_registries;
use crate::core::state::validate_loaded_state;
use crate::fluid::calculate_fluid_volume_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::world::WorldGenerationSeed;
use crate::world_generation::validate_generate_initial_wilderness;

fn opening_water() -> (Registries, AppState, FluidStoreId, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_initial_wilderness(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4B4F_424F_4C44),
        VoxelCoord::new(8, 2, 8),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let (id, position) = state.logistics().fluid_store_locations().next().unwrap();
    (registries, state, id, position)
}

#[test]
fn thirsty_scout_seeks_real_water_and_its_drink_survives_replay() {
    let (registries, mut state, source, position) = opening_water();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    let mut drank = false;
    for _ in 0..1_550 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .is_some()
        {
            drank = true;
            break;
        }
    }
    assert!(
        drank,
        "the unwatched, thirsty resident must reach its existing opening water branch"
    );
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .any(|track| {
                track.position() == position
                    && track.observed_activity() == super::super::KoboldTrailKind::Drinking
            }),
        "a real drink must leave a distinguishable physical departure trace"
    );
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original.checked_sub(SCOUT_DRINK_VOLUME).unwrap()
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        state.logistics().fluid_store_position(source),
        Some(position)
    );
    let json = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&json)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..120 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut restored).unwrap();
    }
    assert_eq!(state, restored);
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
}

#[test]
fn no_water_is_minted_if_scout_cannot_reach_the_source() {
    let (registries, mut state, source, water) = opening_water();
    let start_total = calculate_fluid_volume_accounting(&state).unwrap();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    crate::player::validate_admit_player(&registries, &state, water)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..1_250 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.kobold_state().consumed_fluids.len(), 0);
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        start_total
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_drained_pocket_does_not_regenerate_water_or_satisfy_another_thirst() {
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let water = VoxelCoord::new(1, 0, 0);
    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let generated = validate_generate_fluid_store(
        &registries,
        &state,
        GeneratedFluidStoreSpec::new(
            SCOUT_DRINK_VOLUME,
            FLUID_WATER,
            SCOUT_DRINK_VOLUME,
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap();
    let store = generated.commit(&mut state).unwrap();
    validate_place_fluid_store(&state, store, water)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let original = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_300 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    for _ in 0..1_000 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.kobold_state().colony.as_ref().unwrap().next_drink_at <= state.tick(),
        "a missed drink remains a genuine unresolved need"
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn nearby_observer_hears_one_real_drink_without_guarding_the_source() {
    use crate::kobolds::{KoboldFootfallKind, available_nearby_kobold_footfalls};
    let (registries, mut state, source, water) = opening_water();
    let original = state.fluid().get_store(source).unwrap().stored_volume();
    let junction = state.kobold_state().colony.as_ref().unwrap().route[1];
    let approach = state
        .logistics()
        .traversable_destinations(water)
        .find(|position| state.logistics().has_traversal_edge(*position, junction))
        .unwrap();
    let observer = VoxelCoord::new(
        water.x() + 2 * (water.x() - approach.x()),
        water.y(),
        water.z() + 2 * (water.z() - approach.z()),
    );
    crate::player::validate_admit_player(&registries, &state, observer)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut heard = false;
    for _ in 0..1_500 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            heard = available_nearby_kobold_footfalls(&registries, &state)
                .iter()
                .any(|event| event.kind() == KoboldFootfallKind::Splashing);
            break;
        }
    }
    assert!(
        heard,
        "a real nearby water visit produces an audible splash cue"
    );
    assert_eq!(
        state.fluid().get_store(source).unwrap().stored_volume(),
        original.checked_sub(SCOUT_DRINK_VOLUME).unwrap()
    );
    for _ in 0..3 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        available_nearby_kobold_footfalls(&registries, &state)
            .iter()
            .all(|cue| cue.kind() != KoboldFootfallKind::Splashing),
        "splashing must fade rather than become a permanent noise source"
    );
}

#[test]
fn trusted_load_rejects_forged_thirst_deadlines_and_drink_ledger() {
    let (registries, mut state, _, _) = opening_water();
    let initial = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged = initial.clone();
    forged["state"]["systems"]["kobolds"]["colony"]["next_drink_at"] =
        serde_json::to_value(SimulationTick::new(985)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    // A drinking track is evidence of an actual resident at a water source;
    // a fabricated label cannot establish a nonexistent consumption ledger.
    for _ in 0..185 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let mut false_tracks = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let trail = false_tracks["state"]["systems"]["kobolds"]["colony"]["tracks"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    trail["kind"] = serde_json::json!("Drinking");
    let decoded: LoadedSaveEnvelope = serde_json::from_value(false_tracks).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    for _ in 0..1_250 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let mut fake_volume = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let ledger = fake_volume["state"]["systems"]["kobolds"]["consumed_fluids"]
        .as_object_mut()
        .unwrap();
    assert_eq!(ledger.len(), 1, "resident must have consumed real water");
    *ledger.values_mut().next().unwrap() = serde_json::json!(61_000);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(fake_volume).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

/// A real generated clearing and a physical water source on an adjacent
/// traversable edge. The test does not synthesize climatic or fluid values.
fn generated_outdoor_source() -> (Registries, AppState, FluidStoreId, VoxelCoord) {
    use crate::content::BLOCK_STATE_STONE;
    use crate::core::quantity::Temperature;
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::world::{ChunkCoord, validate_generate_chunk};

    let registries = build_registries();
    for seed_number in 0..32 {
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
        for x in 3..11 {
            for z in 3..11 {
                for y in -4..=5 {
                    let den = VoxelCoord::new(x, y, z);
                    let junction = VoxelCoord::new(x + 1, y, z);
                    let tip = VoxelCoord::new(x + 2, y, z);
                    let water = VoxelCoord::new(x + 1, y, z + 1);
                    if ![den, junction, tip, water].iter().all(|&position| {
                        state.world().block_at(VoxelCoord::new(
                            position.x(),
                            position.y() - 1,
                            position.z(),
                        )) == Some(BLOCK_STATE_STONE)
                            && state
                                .world()
                                .has_standing_clearance(registries.world(), position)
                            && crate::world::exposed_air_at(
                                &registries,
                                &state,
                                position,
                                SimulationTick::new(1_000),
                            )
                            .is_some()
                    }) {
                        continue;
                    }
                    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
                        add_traversable_edge_for_test(&mut state, a, b);
                    }
                    state
                        .kobold_state_mut()
                        .apply_opening_colony([den, junction, tip]);
                    let generated = validate_generate_fluid_store(
                        &registries,
                        &state,
                        GeneratedFluidStoreSpec::new(
                            Volume::from_microliters(300_000),
                            FLUID_WATER,
                            Volume::from_microliters(300_000),
                            Temperature::from_millikelvin(293_150),
                        ),
                    )
                    .unwrap();
                    let store = generated.commit(&mut state).unwrap();
                    validate_place_fluid_store(&state, store, water)
                        .unwrap()
                        .commit(&mut state)
                        .unwrap();
                    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
                    return (registries, state, store, water);
                }
            }
        }
    }
    panic!("no generated clearing with physical scout access to water");
}

#[test]
fn local_weather_modulates_thirst_and_a_roof_removes_outdoor_exposure() {
    use crate::content::BLOCK_STATE_STONE;
    let (registries, mut state, _, water) = generated_outdoor_source();
    let calendar = registries.core().calendar();
    let season = u64::from(calendar.days_per_month())
        * u64::from(calendar.months_per_year() / 4)
        * calendar.ticks_per_day();
    let mut hot = None;
    let mut cold = None;
    let mut windy = None;
    for window in 0..30 {
        let summer = SimulationTick::new(season + calendar.ticks_per_day() / 4 + window * 200);
        let winter = SimulationTick::new(season * 3 + window * 200);
        let heat = crate::world::exposed_air_at(&registries, &state, water, summer).unwrap();
        if heat.ambient_temperature().millikelvin() >= 298_150 {
            hot = Some(summer);
        } else if heat.ambient_temperature().millikelvin() >= 293_150
            && heat.wind() == crate::world::WindCondition::Gusting
        {
            windy = Some(summer);
        }
        let cold_air = crate::world::exposed_air_at(&registries, &state, water, winter).unwrap();
        if cold_air.ambient_temperature().millikelvin() < 278_150 {
            cold = Some(winter);
        }
    }
    let hot = hot.expect("summer should have a hot weather front");
    let cold = cold.expect("winter should have a cold weather front");
    assert_eq!(
        drink_interval_at(&registries, &state, water, hot),
        SCOUT_DRINK_INTERVAL_TICKS / 2,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, cold),
        SCOUT_DRINK_INTERVAL_TICKS * 3 / 2,
    );
    if let Some(windy) = windy {
        assert_eq!(
            drink_interval_at(&registries, &state, water, windy),
            SCOUT_DRINK_INTERVAL_TICKS * 3 / 4,
        );
    }
    let roof = VoxelCoord::new(water.x(), water.y() + 3, water.z());
    crate::world::set_generated_block_for_test(
        registries.world(),
        &mut state,
        roof,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, hot),
        SCOUT_DRINK_INTERVAL_TICKS,
    );
    assert_eq!(
        drink_interval_at(&registries, &state, water, cold),
        SCOUT_DRINK_INTERVAL_TICKS,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn generated_world_scout_drinks_real_water_and_schedules_from_climate() {
    let (registries, mut state, store, water) = generated_outdoor_source();
    let baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let mut drank_at = None;
    for _ in 0..1_350 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            drank_at = Some(state.tick());
            break;
        }
    }
    let at = drank_at.expect("the actual scout should seek and reach exposed water");
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().next_drink_at,
        at.checked_add_span(TickSpan::new(drink_interval_at(
            &registries,
            &state,
            water,
            at,
        )))
        .unwrap(),
    );
    assert_eq!(
        state.fluid().get_store(store).unwrap().stored_volume(),
        Volume::from_microliters(240_000),
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), baseline);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..150 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut replay).unwrap();
    }
    assert_eq!(state, replay);
}

#[test]
fn trusted_load_requires_a_real_last_drink_clock_and_allowed_climate_interval() {
    let (registries, mut state, _, _) = opening_water();
    let fresh = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut invented = fresh.clone();
    invented["state"]["systems"]["kobolds"]["colony"]["last_drink_at"] = serde_json::json!(180);
    invented["state"]["systems"]["kobolds"]["colony"]["last_drink_interval_ticks"] =
        serde_json::json!(800);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(invented)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut missing = fresh;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("last_drink_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());

    for _ in 0..1_400 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .contains_key(&FLUID_WATER)
        {
            break;
        }
    }
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert!(colony.last_drink_at.is_some());
    assert_eq!(
        colony.next_drink_at,
        colony
            .last_drink_at
            .unwrap()
            .checked_add_span(TickSpan::new(colony.last_drink_interval_ticks.unwrap()))
            .unwrap(),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let trusted = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged_due = trusted.clone();
    forged_due["state"]["systems"]["kobolds"]["colony"]["next_drink_at"] =
        serde_json::to_value(SimulationTick::new(colony.next_drink_at.value() + 5)).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_due)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut forged_interval = trusted.clone();
    forged_interval["state"]["systems"]["kobolds"]["colony"]["last_drink_interval_ticks"] =
        serde_json::json!(700);
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_interval)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
    let mut forged_future = trusted;
    forged_future["state"]["systems"]["kobolds"]["colony"]["last_drink_at"] =
        serde_json::to_value(SimulationTick::new(state.tick().value() + 5)).unwrap();
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(forged_future)
            .unwrap()
            .into_state(&registries)
            .is_err()
    );
}

#[test]
fn prolonged_water_denial_stops_nonessential_salvage_until_scout_drinks_again() {
    use crate::content::{FORM_LUMP, MATERIAL_STONE};
    use crate::core::quantity::{Mass, Temperature};
    use crate::fluid::{GeneratedFluidStoreSpec, validate_generate_fluid_store};
    use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
    use crate::material::{CommodityKey, MaterialComposition};
    use crate::surface::{
        SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let water = VoxelCoord::new(1, 0, 0);
    for (a, b) in [(den, junction), (junction, tip), (junction, water)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let supply = |state: &mut AppState, microliters: u64| {
        let volume = Volume::from_microliters(microliters);
        let source = validate_generate_fluid_store(
            &registries,
            state,
            GeneratedFluidStoreSpec::new(
                volume,
                FLUID_WATER,
                volume,
                Temperature::from_millikelvin(293_150),
            ),
        )
        .unwrap()
        .commit(state)
        .unwrap();
        validate_place_fluid_store(state, source, water)
            .unwrap()
            .commit(state)
            .unwrap();
        source
    };
    let original = supply(&mut state, SCOUT_DRINK_VOLUME.microliters());
    for _ in 0..2_700 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.fluid().get_store(original).unwrap().stored_volume(),
        Volume::ZERO,
        "the first actual ration must have completely exhausted the source",
    );
    assert!(prolonged_water_deprivation(
        state.kobold_state().colony.as_ref().unwrap(),
        state.tick(),
    ));

    let cargo = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            tip,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let cargo_id = cargo.first_resource_id();
    cargo.commit(&mut state).unwrap();
    for _ in 0..100 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(cargo_id).unwrap().position(),
        tip,
        "a water-deprived resident must not start a new nonfood raid",
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let water_baseline = calculate_fluid_volume_accounting(&state).unwrap();
    let restored_water = supply(&mut state, 120_000);
    let with_supply = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(
        with_supply.total().microliters() - water_baseline.total().microliters(),
        120_000,
        "the recovered source is actual newly created fixture water",
    );
    let mut second_drink = false;
    let mut salvaged = false;
    for _ in 0..260 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        second_drink |= state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .is_some_and(|volume| volume.microliters() >= 120_000);
        if state.surface().get(cargo_id).unwrap().position() != tip {
            salvaged = true;
            break;
        }
    }
    assert!(
        second_drink,
        "recovery must require reaching an actual new water supply"
    );
    assert!(
        salvaged,
        "rehydrated resident should recover the previously ignored cargo"
    );
    assert!(!prolonged_water_deprivation(
        state.kobold_state().colony.as_ref().unwrap(),
        state.tick(),
    ));
    assert_eq!(
        state
            .fluid()
            .get_store(restored_water)
            .unwrap()
            .stored_volume(),
        Volume::from_microliters(60_000),
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        with_supply
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
