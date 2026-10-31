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
