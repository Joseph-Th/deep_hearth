//! Water destination choice must agree with the exact serving the fluid owner can withdraw.

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

fn water_route() -> (Registries, AppState, [VoxelCoord; 4]) {
    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    let side = VoxelCoord::new(0, 0, 1);
    for (from, to) in [(den, junction), (junction, tip), (junction, side)] {
        add_traversable_edge_for_test(&mut state, from, to);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    (registries, state, [den, junction, tip, side])
}

fn place_water(
    registries: &Registries,
    state: &mut AppState,
    at: VoxelCoord,
    volume: Volume,
) -> FluidStoreId {
    let store = validate_generate_fluid_store(
        registries,
        state,
        GeneratedFluidStoreSpec::new(
            SCOUT_DRINK_VOLUME,
            FLUID_WATER,
            volume,
            Temperature::from_millikelvin(293_150),
        ),
    )
    .unwrap()
    .commit(state)
    .unwrap();
    validate_place_fluid_store(state, store, at)
        .unwrap()
        .commit(state)
        .unwrap();
    store
}

fn observed_route(
    registries: &Registries,
    state: &AppState,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    reachable_water_from_junction(
        registries,
        state,
        state.kobold_state().colony.as_ref().unwrap(),
        None,
    )
}

#[test]
fn scout_chooses_a_full_side_drink_over_one_microliter_on_its_regular_patrol() {
    let (registries, mut state, [_, _, tip, side]) = water_route();
    let tiny = Volume::from_microliters(1);
    let tip_source = place_water(&registries, &mut state, tip, tiny);
    let full_source = place_water(&registries, &mut state, side, SCOUT_DRINK_VOLUME);
    assert_eq!(
        observed_route(&registries, &state),
        Some((side, None)),
        "a real one-edge complete serving must outrank a token drop on the regular route"
    );
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(observed_route(&registries, &replay), Some((side, None)));
    for _ in 0..1_400 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
        if state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .last_drink_at
            .is_some()
        {
            break;
        }
    }
    assert_eq!(state, replay);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert!(
        colony.last_drink_at.is_some(),
        "the scout must finish a real hydration need"
    );
    assert_eq!(colony.partial_drink, Volume::ZERO);
    assert_eq!(
        state.fluid().get_store(tip_source).unwrap().stored_volume(),
        tiny
    );
    assert_eq!(
        state
            .fluid()
            .get_store(full_source)
            .unwrap()
            .stored_volume(),
        Volume::ZERO
    );
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn full_two_edge_water_beats_a_tiny_nearby_serving_without_hidden_search() {
    let (registries, mut state, [_, _, tip, side]) = water_route();
    let far = VoxelCoord::new(side.x(), side.y(), side.z() + 1);
    add_traversable_edge_for_test(&mut state, side, far);
    let _near = place_water(&registries, &mut state, side, Volume::from_microliters(1));
    let _far = place_water(&registries, &mut state, far, SCOUT_DRINK_VOLUME);
    assert_eq!(
        observed_route(&registries, &state),
        Some((side, Some(far))),
        "a complete locally sighted serving warrants one additional traversable edge"
    );
    // Adding a complete source on the regular tip restores the cheaper route.
    let _tip = place_water(&registries, &mut state, tip, SCOUT_DRINK_VOLUME);
    assert_eq!(observed_route(&registries, &state), Some((tip, None)));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_retains_its_patrol_tip_when_two_water_sources_supply_equal_full_drinks() {
    let (registries, mut state, [_, _, tip, side]) = water_route();
    let _tip_source = place_water(&registries, &mut state, tip, SCOUT_DRINK_VOLUME);
    let _side_source = place_water(&registries, &mut state, side, SCOUT_DRINK_VOLUME);
    assert_eq!(observed_route(&registries, &state), Some((tip, None)));
}
