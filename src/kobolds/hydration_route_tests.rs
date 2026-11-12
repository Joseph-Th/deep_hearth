//! Water destination choice must agree with the exact serving the fluid owner can withdraw.

use super::*;
use crate::content::{FORM_FOOD, MATERIAL_WILD_NUTS, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::fluid::{
    GeneratedFluidStoreSpec, calculate_fluid_volume_accounting, validate_generate_fluid_store,
};
use crate::logistics::{add_traversable_edge_for_test, validate_place_fluid_store};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

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

#[test]
fn thirsty_hungry_scout_takes_reachable_water_before_competing_forage() {
    let (registries, mut state, [_, junction, _, water_side]) = water_route();
    let food_side = VoxelCoord::new(junction.x(), junction.y(), junction.z() - 1);
    add_traversable_edge_for_test(&mut state, junction, food_side);
    let water = place_water(&registries, &mut state, water_side, SCOUT_DRINK_VOLUME);

    // Hunger is established by the first missed den meal, while thirst becomes
    // due later. Publish a fresh food source just before that deadline so
    // neither choice is hypothetical or based on a stale target.
    for _ in 0..979 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    let offered_food = Mass::from_milligrams(300_000);
    let food = SurfaceResourceSpec::new(
        SurfaceResourceKind::Opportunity,
        food_side,
        CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
        offered_food,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_WILD_NUTS),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [food])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let water_before = calculate_fluid_volume_accounting(&state).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut first_branch = None;
    let mut drank = false;
    for _ in 0..150 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
        let colony = state.kobold_state().colony.as_ref().unwrap();
        let position = colony.scout_position();
        if first_branch.is_none() && (position == water_side || position == food_side) {
            first_branch = Some(position);
        }
        if colony.last_drink_at.is_some() {
            drank = true;
            break;
        }
    }
    assert_eq!(first_branch, Some(water_side));
    assert!(drank, "a water expedition must end with an actual drink");
    assert_eq!(
        state, replay,
        "saved continuation must choose the same physical route"
    );
    assert_eq!(
        state.fluid().get_store(water).unwrap().stored_volume(),
        Volume::ZERO
    );
    assert_eq!(
        state
            .surface()
            .available_resources_at(Some(food_side))
            .next()
            .unwrap()
            .remaining_mass(),
        offered_food,
        "unforaged food must remain in world custody"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        water_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
