//! Incomplete local water servings remain finite and cannot grant free hydration.

use super::*;
use crate::content::build_registries;
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::fluid::{
    GeneratedFluidStoreSpec, calculate_fluid_volume_accounting, validate_generate_fluid_store,
};
use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;

fn small_water_sources(
    servings: &[Volume],
) -> (Registries, AppState, VoxelCoord, Vec<FluidStoreId>) {
    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let stores = servings
        .iter()
        .map(|&volume| {
            let store = validate_generate_fluid_store(
                &registries,
                &state,
                GeneratedFluidStoreSpec::new(
                    SCOUT_DRINK_VOLUME,
                    FLUID_WATER,
                    volume,
                    Temperature::from_millikelvin(293_150),
                ),
            )
            .unwrap()
            .commit(&mut state)
            .unwrap();
            validate_place_fluid_store(&state, store, junction)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            store
        })
        .collect();
    (registries, state, junction, stores)
}

#[test]
fn two_partial_pools_complete_one_real_scout_drink_across_saved_patrols() {
    let half = Volume::from_microliters(SCOUT_DRINK_VOLUME.microliters() / 2);
    let (registries, mut state, water_position, stores) = small_water_sources(&[half, half]);
    assert_eq!(
        usable_water_at(&registries, &state, water_position),
        Some((stores[0], half)),
        "an accessible half serving is real drinking water"
    );
    let original = calculate_fluid_volume_accounting(&state).unwrap();
    let mut first_drink_at = None;
    for _ in 0..1_300 {
        let tick = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied()
            == Some(AggregateVolume::from_volume(half))
        {
            first_drink_at = Some(tick.tick());
            break;
        }
    }
    let first_drink_at = first_drink_at.expect("scout should drink the first finite half serving");
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(half))
    );
    let partial = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(partial.partial_drink, half);
    assert_eq!(partial.last_drink_at, None);
    assert!(
        partial.next_drink_at <= first_drink_at,
        "half a ration must not reset a full hydration deadline"
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    for invalid_credit in [0_u64, SCOUT_DRINK_VOLUME.microliters()] {
        let mut forged = saved.clone();
        forged["state"]["systems"]["kobolds"]["colony"]["partial_drink"] =
            serde_json::json!(invalid_credit);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Kobolds(
                    "kobold credited drinking does not match finite consumed water"
                )
            )),
            "a forged partial serving cannot claim water never consumed"
        );
    }
    let mut missing_credit = saved;
    missing_credit["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("partial_drink");
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(missing_credit).is_err(),
        "current-schema persistence requires the exact partial-water credit"
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut completed_at = None;
    for _ in 0..240 {
        let result = advance_tick(&registries, &mut state).unwrap();
        assert_eq!(result, advance_tick(&registries, &mut replay).unwrap());
        if state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied()
            == Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
        {
            completed_at = Some(result.tick());
            break;
        }
    }
    let completed_at = completed_at.expect("scout should finish its ration from the other pool");
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.partial_drink, Volume::ZERO);
    assert_eq!(colony.last_drink_at, Some(completed_at));
    assert_eq!(
        colony.next_drink_at.value(),
        completed_at.value() + colony.last_drink_interval_ticks.unwrap()
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), original);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn full_drink_wins_over_earlier_tiny_pool_without_wasting_a_patrol() {
    let tiny = Volume::from_microliters(1);
    let (registries, mut state, position, stores) =
        small_water_sources(&[tiny, SCOUT_DRINK_VOLUME]);
    assert_eq!(
        usable_water_at(&registries, &state, position),
        Some((stores[1], SCOUT_DRINK_VOLUME))
    );
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    for _ in 0..1_300 {
        let _ = advance_tick(&registries, &mut state).unwrap();
        if state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied()
            == Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
        {
            break;
        }
    }
    assert_eq!(
        state
            .kobold_state()
            .consumed_fluids
            .get(&FLUID_WATER)
            .copied(),
        Some(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))
    );
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        tiny,
        "the rejected tiny first vessel must retain all of its water"
    );
    assert_eq!(
        state.fluid().get_store(stores[1]).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
