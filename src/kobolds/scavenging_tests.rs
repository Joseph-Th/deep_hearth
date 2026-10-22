//! Bounded scout cache search and physical parcel-selection priorities.

use crate::content::{FORM_CHIP, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
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
fn scout_forages_the_largest_recoverable_serving_not_the_oldest_crumb() {
    use crate::content::MATERIAL_WILD_NUTS;

    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    state
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while state.tick().value() < 580 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction
    );

    let forage = |position, milligrams| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(milligrams),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            forage(path.side_food, 1_000),
            forage(path.side_stone, 2_000),
            forage(path.side_stone, 150_000),
        ],
    )
    .unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut state).unwrap();
    let maximum = super::super::SCOUT_FORAGE_BATCH;
    assert_eq!(
        super::reachable_side_food(
            &registries,
            &state,
            path.junction,
            path.den,
            path.tip,
            None,
            state.tick(),
            maximum,
        ),
        Some(path.side_stone),
        "a nearly depleted patch must not win the forage cooldown"
    );
    assert_eq!(
        super::best_local_forage(&registries, &state, path.side_stone, state.tick(), maximum),
        Some((maximum, ids[2])),
        "the physical forage must select the abundant parcel at the chosen site"
    );

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while state.tick().value() < 590 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(1_000)
    );
    assert_eq!(
        state.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000)
    );
    assert_eq!(
        state.surface().get(ids[2]).unwrap().remaining_mass(),
        Mass::from_milligrams(50_000)
    );
    while state.tick().value() < 600 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn hungry_loose_food_raids_prefer_a_complete_meal_over_old_fragments() {
    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            drop(path.side_food, true, 1_000),
            drop(path.side_stone, true, 2_000),
            drop(path.far_food, true, 100_000),
        ],
    )
    .unwrap();
    creation.commit(&mut state).unwrap();
    let capacity = Mass::from_milligrams(500_000);
    let choose = |state: &AppState| {
        super::reachable_detour_drop(
            &registries,
            state,
            path.junction,
            path.den,
            path.tip,
            None,
            capacity,
            state.tick(),
            true,
        )
    };
    assert_eq!(
        choose(&state),
        Some((path.side_food, Some(path.far_food))),
        "the complete distant ration must outrank nearby inadequate food"
    );

    let adequate = validate_create_surface_resources(
        &registries,
        &state,
        [drop(path.side_stone, true, 30_000)],
    )
    .unwrap();
    let full_local = adequate.first_resource_id();
    adequate.commit(&mut state).unwrap();
    assert_eq!(
        choose(&state),
        Some((path.side_stone, None)),
        "an adequate nearby serving avoids the unnecessary extra route edge"
    );
    assert_eq!(
        super::local_portable_source(
            &registries,
            &state,
            path.side_stone,
            capacity,
            state.tick(),
            true,
            false,
        ),
        Some(full_local),
        "the actual local transfer must not choose the older inadequate fragment"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn distant_forage_ranking_respects_recoverable_capacity_and_stable_ties() {
    use crate::content::MATERIAL_WILD_NUTS;

    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    let forage = |position, milligrams| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(milligrams),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    validate_create_surface_resources(
        &registries,
        &state,
        [
            forage(path.far_food, 5_000),
            forage(path.far_stone, 150_000),
            forage(path.far_food, 150_000),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let pick = |capacity| {
        super::reachable_far_food(
            &registries,
            &state,
            path.junction,
            path.den,
            path.tip,
            None,
            state.tick(),
            capacity,
        )
    };
    assert_eq!(
        pick(super::super::SCOUT_FORAGE_BATCH),
        Some((path.side_stone, path.far_stone)),
        "a full serving wins over a small old patch, then the earlier full patch wins ties"
    );
    assert_eq!(
        pick(Mass::from_milligrams(1_000)),
        Some((path.side_food, path.far_food)),
        "equal recoverable quantities use the original stable resource ID"
    );
    assert_eq!(pick(Mass::ZERO), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn fresh_intermediate_forage_shortens_a_hungry_scouts_whole_food_raid() {
    use crate::content::MATERIAL_WILD_NUTS;
    use crate::core::time::SimulationTick;

    let registries = build_registries();
    let mut ready = AppState::new();
    let path = route(&mut ready);
    ready
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while ready.tick().value() < 580 {
        let _ = advance_tick(&registries, &mut ready).unwrap();
    }
    let edible = |kind, position, mass| {
        SurfaceResourceSpec::new(
            kind,
            position,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &ready,
        [edible(SurfaceResourceKind::Drop, path.far_food, 120_000)],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut ready).unwrap();
    while ready.tick().value() < 585 {
        let _ = advance_tick(&registries, &mut ready).unwrap();
    }
    assert_eq!(
        ready
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.side_food
    );
    assert_eq!(
        ready.kobold_state().colony.as_ref().unwrap().detour_goal,
        Some(path.far_food)
    );
    let plan = validate_create_surface_resources(
        &registries,
        &ready,
        [edible(
            SurfaceResourceKind::Opportunity,
            path.side_food,
            300_000,
        )],
    )
    .unwrap();
    let wild_id = plan.first_resource_id();
    plan.commit(&mut ready).unwrap();
    assert!(ready.kobold_state().colony.as_ref().unwrap().hungry);
    assert!(
        super::super::feeding::scout_wants_food_opportunity(
            &registries,
            ready.surface().get(wild_id).unwrap(),
            SimulationTick::new(590),
        ),
        "the newly placed patch must be fresh when the scout returns"
    );
    assert!(super::has_local_scout_interest(
        &registries,
        &ready,
        path.side_food,
        Mass::from_milligrams(500_000),
        true,
        SimulationTick::new(590),
        true,
    ));
    let mut cooldown = ready.clone();
    // The scout has a real persisted forage-clock eligibility check, so a
    // finite visible patch alone must not override a reachable intact drop.
    cooldown
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .next_forage_at = Some(SimulationTick::new(680));
    let conserved = calculate_matter_accounting(&ready).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &ready)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut ready).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
        let _ = advance_tick(&registries, &mut cooldown).unwrap();
    }
    assert_eq!(ready, replay);
    assert_eq!(ready.tick().value(), 590);
    assert_eq!(
        ready
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction,
        "a ready nearby forage patch is preferable to walking an additional edge for a ration"
    );
    assert_eq!(
        ready.surface().get(wild_id).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        ready.surface().get(ids[0]).unwrap().position(),
        path.far_food
    );
    assert_eq!(
        cooldown
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.far_food,
        "forage still on cooldown must not displace the planned intact food"
    );
    assert_eq!(
        cooldown.surface().get(wild_id).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000)
    );
    while ready.tick().value() < 600 {
        assert_eq!(
            advance_tick(&registries, &mut ready).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(ready, replay);
    assert!(!ready.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        ready.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        calculate_matter_accounting(&ready)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000
    );
    assert_eq!(
        calculate_matter_accounting(&ready).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&cooldown).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &ready), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &cooldown), Ok(()));
}

#[test]
fn hungry_scout_hauls_whole_distant_food_instead_of_splitting_a_nearer_cache() {
    use crate::content::MATERIAL_WILD_NUTS;

    let registries = build_registries();
    let mut far = AppState::new();
    let path = route(&mut far);
    far.kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while far.tick().value() < 580 {
        let _ = advance_tick(&registries, &mut far).unwrap();
    }
    assert!(far.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        far.kobold_state().colony.as_ref().unwrap().scout_position(),
        path.junction
    );
    let edible = |position, mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let plan =
        validate_create_surface_resources(&registries, &far, [edible(path.side_food, 2_000_000)])
            .unwrap();
    let bulk = plan.first_resource_id();
    plan.commit(&mut far).unwrap();
    let mut only_near = far.clone();
    let plan =
        validate_create_surface_resources(&registries, &far, [edible(path.far_food, 120_000)])
            .unwrap();
    let whole = plan.first_resource_id();
    plan.commit(&mut far).unwrap();
    let far_total = calculate_matter_accounting(&far).unwrap().total();
    let near_total = calculate_matter_accounting(&only_near).unwrap().total();
    assert_eq!(
        super::reachable_detour_drop(
            &registries,
            &far,
            path.junction,
            path.den,
            path.tip,
            None,
            Mass::from_milligrams(500_000),
            far.tick(),
            true,
        ),
        Some((path.side_food, Some(path.far_food))),
        "a complete distant food parcel is less handling than splitting a nearby bulk pile"
    );
    assert_eq!(
        super::reachable_detour_drop(
            &registries,
            &only_near,
            path.junction,
            path.den,
            path.tip,
            None,
            Mass::from_milligrams(500_000),
            only_near.tick(),
            true,
        ),
        Some((path.side_food, None)),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &far)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..30 {
        assert_eq!(
            advance_tick(&registries, &mut far).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
        let _ = advance_tick(&registries, &mut only_near).unwrap();
    }
    assert_eq!(far, replay);
    assert_eq!(far.tick().value(), 610);
    assert_eq!(far.surface().get(whole).unwrap().position(), path.den);
    assert_eq!(
        far.surface().get(whole).unwrap().remaining_mass(),
        Mass::from_milligrams(100_000),
        "an actual 20 g ration is consumed after arrival"
    );
    assert_eq!(
        far.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(2_000_000),
        "the bulky alternative stays untouched"
    );
    assert!(!far.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        only_near.surface().get(bulk).unwrap().remaining_mass(),
        Mass::from_milligrams(1_500_000),
        "two ordinary physical return trips each split exactly 250 g when no whole parcel exists"
    );
    assert_eq!(
        calculate_matter_accounting(&far).unwrap().total(),
        far_total
    );
    assert_eq!(
        calculate_matter_accounting(&only_near).unwrap().total(),
        near_total
    );
    assert_eq!(validate_loaded_state(&registries, &far), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &only_near), Ok(()));
}

#[test]
fn intercepted_far_food_restores_local_split_without_inventing_the_missing_haul() {
    use crate::content::MATERIAL_WILD_NUTS;
    use crate::logistics::relocate_player_for_test;

    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    state
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while state.tick().value() < 580 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let near = drop(path.side_food, false, 2_000_000);
    let far_food = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        path.far_food,
        CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
        Mass::from_milligrams(120_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_WILD_NUTS),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [near, far_food]).unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    while state.tick().value() < 585 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let scout = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(scout.scout_position(), path.side_food);
    assert_eq!(scout.detour_goal, Some(path.far_food));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let player = validate_admit_player(&registries, &state, path.far_food)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .expect("ordinary player pickup can intercept the planned target");
    assert_eq!(pickup.resource(), ids[1]);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(120_000));
    relocate_player_for_test(&mut state, VoxelCoord::new(99, 0, 99));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    while state.tick().value() < 595 {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.den
    );
    assert_eq!(
        state.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(1_750_000),
        "after the food is picked up, the scout physically splits the nearby inert pile"
    );
    assert_eq!(
        state.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(path.den))
            .any(
                |resource| resource.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
                    && resource.remaining_mass() == Mass::from_milligrams(250_000)
            )
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(player.carried_stockpile())
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn nearby_whole_food_still_beats_a_whole_distant_parcel() {
    use crate::content::MATERIAL_WILD_NUTS;
    let registries = build_registries();
    let mut state = AppState::new();
    let path = route(&mut state);
    let edible = |position, mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [
            edible(path.side_food, 100_000),
            edible(path.far_food, 120_000),
        ],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    state
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    let initial = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        super::reachable_detour_drop(
            &registries,
            &state,
            path.junction,
            path.den,
            path.tip,
            None,
            Mass::from_milligrams(500_000),
            state.tick(),
            true,
        ),
        Some((path.side_food, None))
    );
    while state.tick().value() < 200 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.surface().get(ids[0]).unwrap().position(), path.den);
    assert_eq!(
        state.surface().get(ids[1]).unwrap().position(),
        path.far_food
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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

#[test]
fn visible_player_guarding_food_redirects_scout_to_safe_scrap_until_player_withdraws() {
    use crate::logistics::relocate_player_for_test;

    let registries = build_registries();
    let mut guarded = AppState::new();
    let path = route(&mut guarded);
    let plan = validate_create_surface_resources(
        &registries,
        &guarded,
        [
            drop(path.side_food, true, 250_000),
            drop(path.side_stone, false, 250_000),
        ],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut guarded).unwrap();
    guarded
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    for _ in 0..180 {
        let _ = advance_tick(&registries, &mut guarded).unwrap();
    }
    let mut unguarded = guarded.clone();
    let player_guarding_food = VoxelCoord::new(2, 0, 0);
    let player_not_guarding = VoxelCoord::new(4, 0, 0);
    validate_admit_player(&registries, &guarded, player_guarding_food)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    validate_admit_player(&registries, &unguarded, player_not_guarding)
        .unwrap()
        .commit(&mut unguarded)
        .unwrap();
    let matter_before = calculate_matter_accounting(&guarded).unwrap().total();
    let space = Mass::from_milligrams(500_000);
    assert_eq!(
        reachable_side_drop(
            &registries,
            &guarded,
            path.junction,
            path.den,
            path.tip,
            Some(player_guarding_food),
            space,
            guarded.tick(),
            false,
        ),
        Some(path.side_stone),
        "visible food protection redirects the actual local candidate search"
    );
    assert_eq!(
        reachable_side_drop(
            &registries,
            &unguarded,
            path.junction,
            path.den,
            path.tip,
            Some(player_not_guarding),
            space,
            unguarded.tick(),
            false,
        ),
        Some(path.side_food),
        "without a threat the preferred fresh food is the first target"
    );
    for state in [&mut guarded, &mut unguarded] {
        while state.tick().value() < 185 {
            let _ = advance_control_tick(&registries, state).unwrap();
        }
    }
    let position = |state: &AppState| {
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position()
    };
    assert_eq!(position(&guarded), path.side_stone);
    assert_eq!(position(&unguarded), path.side_food);
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &guarded)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while guarded.tick().value() < 195 {
        let a = advance_control_tick(&registries, &mut guarded).unwrap();
        let b = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(a, b);
    }
    assert_eq!(guarded, replay);
    assert_eq!(position(&guarded), path.den);
    assert_eq!(
        guarded.surface().get(ids[0]).unwrap().position(),
        path.side_food
    );
    assert_eq!(guarded.surface().get(ids[1]).unwrap().position(), path.den);
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        matter_before
    );
    // The player gives up the food-guarding post; the same scout can take the
    // source it previously avoided, with no route or inventory reset.
    relocate_player_for_test(&mut guarded, player_not_guarding);
    relocate_player_for_test(&mut replay, player_not_guarding);
    while guarded.tick().value() < 215 {
        let a = advance_control_tick(&registries, &mut guarded).unwrap();
        let b = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(a, b);
    }
    assert_eq!(guarded, replay);
    assert_eq!(guarded.surface().get(ids[0]).unwrap().position(), path.den);
    assert_eq!(guarded.surface().get(ids[1]).unwrap().position(), path.den);
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn player_guards_homeward_junction_and_forces_scout_to_drop_its_haul() {
    use crate::logistics::relocate_player_for_test;

    let registries = build_registries();
    let mut guarded = AppState::new();
    let path = route(&mut guarded);
    let created = validate_create_surface_resources(
        &registries,
        &guarded,
        [drop(path.far_stone, false, 250_000)],
    )
    .unwrap();
    let parcel = created.first_resource_id();
    created.commit(&mut guarded).unwrap();
    guarded
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while guarded.tick().value() < 195 {
        let _ = advance_tick(&registries, &mut guarded).unwrap();
    }
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        Some(parcel)
    );
    assert_eq!(
        guarded.surface().get(parcel).unwrap().position(),
        path.side_stone
    );

    let mut unattended = guarded.clone();
    validate_admit_player(&registries, &guarded, path.tip)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    // From the side branch, the player is two cells away and can be seen,
    // but only the *next* return cell is in direct deterrence range.
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &guarded,
        path.side_stone
    ));
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &guarded,
        path.junction
    ));
    let conserved = calculate_matter_accounting(&guarded).unwrap().total();
    while guarded.tick().value() < 200 {
        let _ = advance_control_tick(&registries, &mut guarded).unwrap();
    }
    while unattended.tick().value() < 200 {
        let _ = advance_tick(&registries, &mut unattended).unwrap();
    }
    let colony = guarded.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.scout_position(), path.side_stone);
    assert_eq!(colony.scavenged_drop, None);
    assert_eq!(
        colony.wary_until,
        Some(crate::core::time::SimulationTick::new(240))
    );
    assert_eq!(
        guarded.surface().get(parcel).unwrap().position(),
        path.side_stone,
        "a frightened scout must release its existing parcel instead of dragging it toward the player"
    );
    assert_eq!(
        unattended
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction
    );
    assert_eq!(
        unattended.surface().get(parcel).unwrap().position(),
        path.junction
    );

    // When the guard withdraws, the same scout reclaims its real loose
    // parcel and finishes the return route without cargo teleportation.
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &guarded)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for state in [&mut guarded, &mut replay] {
        relocate_player_for_test(state, VoxelCoord::new(30, 0, 30));
    }
    while guarded.tick().value() < 210 {
        assert_eq!(
            advance_control_tick(&registries, &mut guarded).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        if guarded.tick().value() == 205 {
            assert_eq!(
                guarded.surface().get(parcel).unwrap().position(),
                path.junction
            );
            assert_eq!(
                guarded
                    .kobold_state()
                    .colony
                    .as_ref()
                    .unwrap()
                    .scavenged_drop,
                Some(parcel)
            );
        }
    }
    assert_eq!(guarded, replay);
    assert_eq!(guarded.surface().get(parcel).unwrap().position(), path.den);
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        None
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn returning_scout_does_not_anticipate_a_hidden_guard_around_stone() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut hidden = AppState::new();
    let path = route(&mut hidden);
    // Generate terrain on all three affected chunk sides, so path clearance
    // and opacity come from the real world owner rather than authored graph
    // traversal alone.
    for coord in [
        ChunkCoord::containing(path.den),
        ChunkCoord::containing(path.junction),
        ChunkCoord::containing(path.side_stone),
    ] {
        validate_generate_chunk(
            &registries,
            &hidden,
            WorldGenerationSeed::new(7),
            coord,
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut hidden)
        .unwrap();
    }
    let observer = VoxelCoord::new(1, 0, 0);
    let blind_corner = VoxelCoord::new(1, 0, -1);
    for cell in [
        path.den,
        path.junction,
        path.tip,
        path.side_stone,
        path.far_stone,
        observer,
        blind_corner,
    ] {
        set_generated_block_for_test(
            registries.world(),
            &mut hidden,
            VoxelCoord::new(cell.x(), -1, cell.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut hidden,
                VoxelCoord::new(cell.x(), y, cell.z()),
                BlockStateId::AIR,
            );
        }
    }
    let created = validate_create_surface_resources(
        &registries,
        &hidden,
        [drop(path.far_stone, false, 250_000)],
    )
    .unwrap();
    let parcel = created.first_resource_id();
    created.commit(&mut hidden).unwrap();
    hidden
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while hidden.tick().value() < 195 {
        let _ = advance_tick(&registries, &mut hidden).unwrap();
    }
    assert_eq!(
        hidden.surface().get(parcel).unwrap().position(),
        path.side_stone
    );
    let mut exposed = hidden.clone();
    set_generated_block_for_test(
        registries.world(),
        &mut hidden,
        blind_corner,
        BLOCK_STATE_STONE,
    );
    for state in [&mut hidden, &mut exposed] {
        validate_admit_player(&registries, state, observer)
            .unwrap()
            .commit(state)
            .unwrap();
    }
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        path.side_stone,
        observer
    ));
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &exposed,
        path.side_stone,
        observer
    ));
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &hidden,
        path.junction
    ));
    let matter_hidden = calculate_matter_accounting(&hidden).unwrap().total();
    let matter_exposed = calculate_matter_accounting(&exposed).unwrap().total();
    while hidden.tick().value() < 200 {
        let _ = advance_control_tick(&registries, &mut hidden).unwrap();
        let _ = advance_control_tick(&registries, &mut exposed).unwrap();
    }
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction,
        "a scout cannot react to danger it has not yet seen from its actual cell"
    );
    assert_eq!(
        hidden.surface().get(parcel).unwrap().position(),
        path.junction
    );
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        Some(parcel),
        "the occluded scout carries its original parcel across the real edge"
    );
    assert_eq!(
        exposed
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction,
        "an already frightened scout can still flee along its actual return edge"
    );
    assert_eq!(
        exposed.surface().get(parcel).unwrap().position(),
        path.side_stone
    );
    assert_eq!(
        exposed
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        None,
        "seeing the player up close forces the scout to abandon the same parcel"
    );
    assert!(
        exposed
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .wary_until
            .is_some()
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &exposed)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(exposed, replay);
    assert_eq!(
        calculate_matter_accounting(&hidden).unwrap().total(),
        matter_hidden
    );
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        matter_exposed
    );
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));
}

#[test]
fn finite_junction_light_intercepts_homeward_cargo_then_dimming_reopens_the_route() {
    use crate::content::{
        BLOCK_STATE_STONE, CAMPFIRE_CHIP_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LUMP,
        MATERIAL_STONE, MATERIAL_WOOD,
    };
    use crate::energy::{
        assess_local_illumination, validate_assemble_energy_store, validate_combust_selected_fuel,
    };
    use crate::inventory::deposit_bulk_for_test;
    use crate::logistics::relocate_player_for_test;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut lit = AppState::new();
    let path = route(&mut lit);
    for position in [path.den, path.junction, path.side_stone] {
        validate_generate_chunk(
            &registries,
            &lit,
            WorldGenerationSeed::new(7),
            ChunkCoord::containing(position),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut lit)
        .unwrap();
    }
    for position in [
        path.den,
        path.junction,
        path.tip,
        path.side_food,
        path.side_stone,
        path.far_food,
        path.far_stone,
    ] {
        set_generated_block_for_test(
            registries.world(),
            &mut lit,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut lit,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    let plan = validate_create_surface_resources(
        &registries,
        &lit,
        [drop(path.far_stone, false, 250_000)],
    )
    .unwrap();
    let cargo = plan.first_resource_id();
    plan.commit(&mut lit).unwrap();
    lit.kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while lit.tick().value() < 195 {
        let _ = advance_tick(&registries, &mut lit).unwrap();
    }
    assert_eq!(
        lit.surface().get(cargo).unwrap().position(),
        path.side_stone
    );
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        Some(cargo)
    );
    let mut dark = lit.clone();
    validate_admit_player(&registries, &lit, path.junction)
        .unwrap()
        .commit(&mut lit)
        .unwrap();
    let stockpile = lit.logistics().player().unwrap().carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
            CAMPFIRE_CHIP_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut lit, stockpile, commodity, mass).unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &lit, ENERGY_STONE_RING_CAMPFIRE, stockpile)
            .unwrap()
            .commit(&mut lit)
            .unwrap();
    validate_combust_selected_fuel(
        &registries,
        &lit,
        fire,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
    )
    .unwrap()
    .commit(&mut lit)
    .unwrap();
    // Admission, assembly and fueling use production owners. Move the player
    // out of this particular showdown before the scout's next slow tick.
    relocate_player_for_test(&mut lit, path.far_food);
    assert_eq!(
        assess_local_illumination(&registries, &lit, path.junction).level(),
        3
    );
    assert_eq!(
        assess_local_illumination(&registries, &lit, path.side_stone).level(),
        2,
        "one real edge away the fueled campfire is below the deterrence threshold"
    );
    let conserved = calculate_matter_accounting(&lit).unwrap().total();
    let tracks_before = lit.kobold_state().colony.as_ref().unwrap().tracks.len();
    while lit.tick().value() < 200 {
        let _ = advance_tick(&registries, &mut lit).unwrap();
        let _ = advance_tick(&registries, &mut dark).unwrap();
    }
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().scout_position(),
        path.side_stone,
        "a calm scavenger must not walk into newly bright terrain with cargo"
    );
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        None
    );
    assert_eq!(
        lit.surface().get(cargo).unwrap().position(),
        path.side_stone
    );
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().tracks.len(),
        tracks_before,
        "standing still in bright terrain must not fabricate any physical footprint"
    );
    assert_eq!(
        dark.kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        path.junction
    );
    assert_eq!(dark.surface().get(cargo).unwrap().position(), path.junction);
    assert_eq!(
        calculate_matter_accounting(&lit).unwrap().total(),
        conserved
    );

    // A player can recover the abandoned real parcel while the campfire still
    // guards the junction. The same production control-tick pickup changes
    // custody, not the underlying identity or global matter balance.
    let mut recovered = lit.clone();
    let player_stockpile = recovered.logistics().player().unwrap().carried_stockpile();
    let stock_before = recovered
        .inventory()
        .get_stockpile(player_stockpile)
        .unwrap()
        .stored_mass();
    relocate_player_for_test(&mut recovered, path.side_stone);
    let pickup = advance_control_tick(&registries, &mut recovered)
        .unwrap()
        .surface_drop_pickup()
        .expect("the abandoned scout cargo must remain an ordinary local pickup");
    assert_eq!(pickup.resource(), cargo);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(250_000));
    assert_eq!(
        recovered
            .inventory()
            .get_stockpile(player_stockpile)
            .unwrap()
            .stored_mass(),
        stock_before
            .checked_add(Mass::from_milligrams(250_000))
            .unwrap()
    );
    relocate_player_for_test(&mut recovered, path.far_food);
    assert_eq!(
        calculate_matter_accounting(&recovered).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &recovered), Ok(()));

    // The same represented energy visibly dims on the normal simulation
    // clock. There is no AI-only burn timer or instantaneous light switch.
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &lit)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while lit.tick().value() < 550 && lit.surface().get(cargo).unwrap().position() != path.den {
        assert_eq!(
            advance_tick(&registries, &mut lit).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(lit, replay);
    assert!(
        lit.tick().value() < 550,
        "the finite fire must dim enough for a bounded real return"
    );
    assert!(
        assess_local_illumination(&registries, &lit, path.junction).level() < 3,
        "finite energy loss must extinguish the deterrence threshold"
    );
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().scout_position(),
        path.den
    );
    assert_eq!(lit.surface().get(cargo).unwrap().position(), path.den);
    assert!(
        lit.kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .any(|track| {
                track.position() == path.side_stone
                    && track.toward() == path.junction
                    && track.observed_activity() == crate::kobolds::KoboldTrailKind::Hauling
            }),
        "recovered cargo reaches the den only after a new real hauling edge"
    );
    assert_eq!(
        lit.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        None
    );
    assert_eq!(
        calculate_matter_accounting(&lit).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &lit), Ok(()));
}

#[test]
fn filling_the_den_mid_haul_leaves_real_cargo_at_the_approach_for_player_recovery() {
    let registries = build_registries();
    let mut blocked = AppState::new();
    let path = route(&mut blocked);
    let creation = validate_create_surface_resources(
        &registries,
        &blocked,
        [drop(path.far_stone, false, 250_000)],
    )
    .unwrap();
    let parcel = creation.first_resource_id();
    creation.commit(&mut blocked).unwrap();
    blocked
        .kobold_state_mut()
        .apply_opening_colony([path.den, path.junction, path.tip]);
    while blocked.tick().value() < 195 {
        let _ = advance_tick(&registries, &mut blocked).unwrap();
    }
    assert_eq!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        Some(parcel)
    );
    assert_eq!(
        blocked.surface().get(parcel).unwrap().position(),
        path.side_stone
    );

    // The scout is already carrying material when another physical surface
    // transaction fills the den. The parcel must not teleport to the den,
    // vanish, or drop prematurely at the distant cache branch.
    let mut open_den = blocked.clone();
    let blocker_plan =
        validate_create_surface_resources(&registries, &blocked, [drop(path.den, false, 500_000)])
            .unwrap();
    let blocker = blocker_plan.first_resource_id();
    blocker_plan.commit(&mut blocked).unwrap();
    let conserved = calculate_matter_accounting(&blocked).unwrap().total();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &blocked)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    for state in [&mut blocked, &mut replay, &mut open_den] {
        while state.tick().value() < 200 {
            let _ = advance_tick(&registries, state).unwrap();
        }
        assert_eq!(
            state.surface().get(parcel).unwrap().position(),
            path.junction
        );
        assert_eq!(
            state.kobold_state().colony.as_ref().unwrap().scavenged_drop,
            Some(parcel),
            "the creature still carries its original parcel, despite the newly full den"
        );
    }
    assert_eq!(blocked, replay);
    for state in [&mut blocked, &mut replay, &mut open_den] {
        while state.tick().value() < 205 {
            let _ = advance_tick(&registries, state).unwrap();
        }
        assert_eq!(
            state
                .kobold_state()
                .colony
                .as_ref()
                .unwrap()
                .scout_position(),
            path.den
        );
        assert_eq!(
            state.kobold_state().colony.as_ref().unwrap().scavenged_drop,
            None
        );
    }
    assert_eq!(blocked, replay);
    assert_eq!(
        blocked.surface().get(parcel).unwrap().position(),
        path.junction,
        "cargo too large for the remaining space stays recoverable at the last real route cell"
    );
    assert_eq!(blocked.surface().get(blocker).unwrap().position(), path.den);
    assert_eq!(open_den.surface().get(parcel).unwrap().position(), path.den);
    assert_eq!(
        calculate_matter_accounting(&blocked).unwrap().total(),
        conserved
    );

    // The player can actually recover the abandoned parcel through the
    // ordinary control-tick pickup operation, not by editing den inventory.
    for state in [&mut blocked, &mut replay] {
        validate_admit_player(&registries, state, path.junction)
            .unwrap()
            .commit(state)
            .unwrap();
    }
    let picked = advance_control_tick(&registries, &mut blocked)
        .unwrap()
        .surface_drop_pickup()
        .expect("the abandoned parcel is a normal recoverable world item");
    assert_eq!(picked.resource(), parcel);
    assert_eq!(picked.picked_mass(), Mass::from_milligrams(250_000));
    let second = advance_control_tick(&registries, &mut replay)
        .unwrap()
        .surface_drop_pickup()
        .expect("the same item remains recoverable after loading");
    assert_eq!(second, picked);
    assert_eq!(blocked, replay);
    assert_eq!(
        calculate_matter_accounting(&blocked).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &blocked), Ok(()));
}
