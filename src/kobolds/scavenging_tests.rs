//! Bounded scout cache search and physical parcel-selection priorities.

use crate::content::{FORM_CHIP, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

use super::{reachable_far_drop, reachable_side_drop};

struct Route {
    den: VoxelCoord,
    junction: VoxelCoord,
    tip: VoxelCoord,
    side_food: VoxelCoord,
    side_stone: VoxelCoord,
    far_food: VoxelCoord,
    far_stone: VoxelCoord,
}

fn route(state: &mut AppState) -> Route {
    let route = Route {
        den: VoxelCoord::new(-1, 0, 0),
        junction: VoxelCoord::new(0, 0, 0),
        tip: VoxelCoord::new(0, 0, 1),
        side_food: VoxelCoord::new(1, 0, 0),
        side_stone: VoxelCoord::new(0, 0, -1),
        far_food: VoxelCoord::new(2, 0, 0),
        far_stone: VoxelCoord::new(0, 0, -2),
    };
    for (a, b) in [
        (route.den, route.junction),
        (route.junction, route.tip),
        (route.junction, route.side_food),
        (route.junction, route.side_stone),
        (route.side_food, route.far_food),
        (route.side_stone, route.far_stone),
    ] {
        add_traversable_edge_for_test(state, a, b);
    }
    route
}

fn drop(position: VoxelCoord, food: bool, mass: u64) -> SurfaceResourceSpec {
    let (material, form) = if food {
        (MATERIAL_BERRIES, FORM_FOOD)
    } else {
        (MATERIAL_STONE, FORM_CHIP)
    };
    SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(material, form),
        Mass::from_milligrams(mass),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(material),
    )
    .unwrap()
}

#[test]
fn side_cache_prefers_whole_cargo_over_older_bulk_food_but_splits_when_pack_space_is_small() {
    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    validate_create_surface_resources(
        &registries,
        &state,
        [
            drop(path.side_food, true, 2_000_000),
            drop(path.side_stone, false, 250_000),
            drop(VoxelCoord::new(99, 0, 99), true, 100_000),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let choose = |capacity| {
        reachable_side_drop(
            &registries,
            &state,
            path.junction,
            path.den,
            path.tip,
            None,
            capacity,
            state.tick(),
            false,
        )
    };
    assert_eq!(
        choose(Mass::from_milligrams(500_000)),
        Some(path.side_stone)
    );
    assert_eq!(choose(Mass::from_milligrams(50_000)), Some(path.side_food));
    assert_eq!(choose(Mass::ZERO), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn far_cache_search_uses_two_real_edges_and_prefers_whole_cargo() {
    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    validate_create_surface_resources(
        &registries,
        &state,
        [
            drop(path.far_food, true, 2_000_000),
            drop(path.far_stone, false, 250_000),
            drop(VoxelCoord::new(99, 0, 99), true, 100_000),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let choose = |capacity| {
        reachable_far_drop(
            &registries,
            &state,
            path.junction,
            path.den,
            path.tip,
            None,
            capacity,
            state.tick(),
            false,
        )
    };
    assert_eq!(
        choose(Mass::from_milligrams(500_000)),
        Some((path.side_stone, path.far_stone))
    );
    assert_eq!(
        choose(Mass::from_milligrams(50_000)),
        Some((path.side_food, path.far_food))
    );
    assert_eq!(choose(Mass::ZERO), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scout_physically_hauls_portable_branch_cache_before_splitting_bulk_food() {
    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            drop(path.side_food, true, 2_000_000),
            drop(path.side_stone, false, 250_000),
        ],
    )
    .unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut state).unwrap();
    state
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..185 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.side_stone
    );
    assert_eq!(
        state.surface().get(ids[0]).unwrap().position(),
        path.side_food
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(state.surface().get(ids[1]).unwrap().position(), path.den);
    assert_eq!(
        state.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
