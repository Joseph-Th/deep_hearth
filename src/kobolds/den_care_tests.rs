//! A resident must choose food over inert hoarding using real finite local custody.

use crate::content::{FORM_CHIP, FORM_FOOD, MATERIAL_STONE, MATERIAL_WILD_NUTS, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

fn setup() -> (crate::Registries, AppState, [VoxelCoord; 4]) {
    let registries = build_registries();
    let mut state = AppState::new();
    let [den, junction, tip, side] = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(1, 0, 0),
    ];
    for (a, b) in [(den, junction), (junction, tip), (junction, side)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    while state.tick().value() < 579 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        den
    );
    (registries, state, [den, junction, tip, side])
}

fn resource(position: VoxelCoord, kind: SurfaceResourceKind, food: bool) -> SurfaceResourceSpec {
    let (material, form, mass) = if food {
        (
            MATERIAL_WILD_NUTS,
            FORM_FOOD,
            Mass::from_milligrams(300_000),
        )
    } else {
        (MATERIAL_STONE, FORM_CHIP, Mass::from_milligrams(500_000))
    };
    SurfaceResourceSpec::new(
        kind,
        position,
        CommodityKey::new(material, form),
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(material),
    )
    .unwrap()
}

#[test]
fn hungry_den_releases_the_smallest_low_value_parcel_that_can_fit_a_meal() {
    use crate::content::{FORM_ELECTRICAL_WINDING, MATERIAL_COPPER};

    let (registries, mut state, [den, junction, _, _]) = setup();
    let stored = |material, form, grams_mg| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            den,
            CommodityKey::new(material, form),
            Mass::from_milligrams(grams_mg),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(material),
        )
        .unwrap()
    };
    // The first-created den parcel is the precious item. Stable ID order
    // alone must not decide which physical stock is thrown onto the junction.
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [
            stored(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING, 300_000),
            stored(MATERIAL_STONE, FORM_CHIP, 180_000),
            stored(MATERIAL_STONE, FORM_CHIP, 20_000),
            resource(junction, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..11 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(state.tick(), SimulationTick::new(590));
    assert_eq!(state.surface().get(ids[0]).unwrap().position(), den);
    assert_eq!(state.surface().get(ids[1]).unwrap().position(), den);
    assert_eq!(
        state.surface().get(ids[2]).unwrap().position(),
        junction,
        "the scout must sacrifice the smallest eligible raw parcel, not the earliest ID"
    );
    assert_eq!(
        state.surface().get(ids[3]).unwrap().remaining_mass(),
        Mass::from_milligrams(280_000),
        "new den capacity physically limits the next forage to exactly one ration",
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000,
    );
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn complete_ration_takes_priority_over_preserving_a_valuable_cache() {
    use crate::content::{FORM_ELECTRICAL_WINDING, MATERIAL_COPPER};

    let (registries, mut state, [den, junction, _, _]) = setup();
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [
            SurfaceResourceSpec::new(
                SurfaceResourceKind::Drop,
                den,
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(10_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap(),
            SurfaceResourceSpec::new(
                SurfaceResourceKind::Drop,
                den,
                CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
                Mass::from_milligrams(490_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_COPPER),
            )
            .unwrap(),
            resource(junction, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(
        state.surface().get(ids[0]).unwrap().position(),
        den,
        "ten grams of cheap scrap cannot free a complete twenty-gram meal"
    );
    assert_eq!(
        state.surface().get(ids[1]).unwrap().position(),
        junction,
        "survival takes precedence if only precious custody can fit one meal"
    );
    assert_eq!(
        state.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(490_000),
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn guarded_den_approach_blocks_cache_disposal_without_changing_real_supply() {
    let (registries, mut guarded, [den, junction, _, side]) = setup();
    let creation = validate_create_surface_resources(
        &registries,
        &guarded,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(junction, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut guarded).unwrap();
    let mut empty_approach = guarded.clone();
    let initial_total = calculate_matter_accounting(&guarded).unwrap().total();

    // At the side, the player is not near enough to frighten the den resident
    // directly. But the scout can actually see the guarded junction, and
    // should not place its stored parcel into that contested clearing.
    validate_admit_player(&registries, &guarded, side)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &guarded,
        den
    ));
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &guarded,
        junction
    ));
    while guarded.tick().value() < 580 {
        let _ = advance_control_tick(&registries, &mut guarded).unwrap();
    }
    let _ = advance_tick(&registries, &mut empty_approach).unwrap();
    assert_eq!(guarded.tick(), SimulationTick::new(580));
    assert_eq!(
        guarded.surface().get(ids[0]).unwrap().position(),
        den,
        "guarded approach cannot be used for spoiled disposal or food cache clearing"
    );
    assert_eq!(
        empty_approach.surface().get(ids[0]).unwrap().position(),
        junction,
        "the same hungry resident releases real inert custody when no player guards"
    );
    assert_eq!(
        guarded.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000),
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        initial_total
    );
    assert_eq!(
        calculate_matter_accounting(&empty_approach)
            .unwrap()
            .total(),
        initial_total
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &guarded)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, guarded);
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn hungry_colony_releases_one_real_stone_parcel_for_nearby_food_then_eats() {
    let (registries, mut state, [den, junction, _, _]) = setup();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(junction, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let ids = creation.resource_ids().collect::<Vec<_>>();
    creation.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(
        advance_tick(&registries, &mut state).unwrap(),
        advance_tick(&registries, &mut loaded).unwrap()
    );
    assert!(
        !state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .noticed_player_drop,
        "a scout cannot discover its own just-released parcel as an external cache"
    );
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(state.tick(), SimulationTick::new(590));
    assert_eq!(state.surface().get(ids[0]).unwrap().position(), junction);
    assert_eq!(
        state.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(
        state.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert!(state.surface().available_resources_at(Some(den)).any(|r| {
        r.kind() == SurfaceResourceKind::Drop
            && r.commodity() == CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD)
            && r.remaining_mass() == Mass::from_milligrams(80_000)
    }));
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().next_meal_at,
        SimulationTick::new(980)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn scant_den_food_does_not_count_as_a_complete_ration() {
    let (registries, mut hungry, [den, _, _, _]) = setup();
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let loose_food = |mass: u64| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            den,
            nuts,
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let crumbs =
        validate_create_surface_resources(&registries, &hungry, [loose_food(5_000)]).unwrap();
    let crumb_id = crumbs.first_resource_id();
    crumbs.commit(&mut hungry).unwrap();
    let mut provisioned = hungry.clone();
    let full =
        validate_create_surface_resources(&registries, &provisioned, [loose_food(30_000)]).unwrap();
    let full_id = full.first_resource_id();
    full.commit(&mut provisioned).unwrap();
    let scarce_total = calculate_matter_accounting(&hungry).unwrap().total();
    let provisioned_total = calculate_matter_accounting(&provisioned).unwrap().total();

    let _ = advance_tick(&registries, &mut hungry).unwrap();
    let _ = advance_tick(&registries, &mut provisioned).unwrap();
    assert!(hungry.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&hungry)
            .unwrap()
            .consumed()
            .milligrams(),
        5_000,
        "a starving resident eats the available fresh fragment"
    );
    assert_eq!(
        hungry.surface().get(crumb_id).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(
        provisioned
            .surface()
            .get(crumb_id)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(5_000),
        "a complete serving takes precedence over an inadequate fragment"
    );
    assert_eq!(
        provisioned.surface().get(full_id).unwrap().remaining_mass(),
        Mass::from_milligrams(10_000),
    );
    assert!(!provisioned.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&provisioned)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000,
    );
    assert_eq!(
        hungry.kobold_state().colony.as_ref().unwrap().next_meal_at,
        SimulationTick::new(980),
    );
    assert_eq!(
        calculate_matter_accounting(&hungry).unwrap().total(),
        scarce_total,
    );
    assert_eq!(
        calculate_matter_accounting(&provisioned).unwrap().total(),
        provisioned_total,
    );

    let later =
        validate_create_surface_resources(&registries, &hungry, [loose_food(25_000)]).unwrap();
    later.commit(&mut hungry).unwrap();
    let later_total = calculate_matter_accounting(&hungry).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &hungry)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while hungry.tick().value() < 600 {
        assert_eq!(
            advance_tick(&registries, &mut hungry).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(hungry, restored);
    assert!(
        !hungry.kobold_state().colony.as_ref().unwrap().hungry,
        "a full real den serving clears the persistent missed-meal hunger"
    );
    assert_eq!(
        calculate_matter_accounting(&hungry)
            .unwrap()
            .consumed()
            .milligrams(),
        25_000,
    );
    assert_eq!(
        calculate_matter_accounting(&hungry).unwrap().total(),
        later_total,
    );
    assert_eq!(validate_loaded_state(&registries, &hungry), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &provisioned), Ok(()));
}

#[test]
fn den_clears_cache_only_for_food_it_can_collect_on_this_visit() {
    let (registries, mut blocked, [den, junction, _, _]) = setup();
    // Model an outstanding real forage recovery deadline on the next scheduled
    // meal: the normal collection path has no permission to harvest before 630.
    blocked
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .next_forage_at = Some(SimulationTick::new(630));
    let stone_plan = validate_create_surface_resources(
        &registries,
        &blocked,
        [resource(den, SurfaceResourceKind::Drop, false)],
    )
    .unwrap();
    let stone = stone_plan.first_resource_id();
    stone_plan.commit(&mut blocked).unwrap();

    let mut loose = blocked.clone();
    let mut ready = blocked.clone();
    for (state, kind) in [
        (&mut blocked, SurfaceResourceKind::Opportunity),
        (&mut loose, SurfaceResourceKind::Drop),
        (&mut ready, SurfaceResourceKind::Opportunity),
    ] {
        let plan =
            validate_create_surface_resources(&registries, state, [resource(junction, kind, true)])
                .unwrap();
        plan.commit(state).unwrap();
    }
    ready
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap()
        .next_forage_at = Some(SimulationTick::new(580));
    for state in [&blocked, &loose, &ready] {
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    }
    let conserved = calculate_matter_accounting(&blocked).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &blocked)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    assert_eq!(
        advance_tick(&registries, &mut blocked).unwrap(),
        advance_tick(&registries, &mut replay).unwrap()
    );
    let _ = advance_tick(&registries, &mut loose).unwrap();
    let _ = advance_tick(&registries, &mut ready).unwrap();
    assert_eq!(blocked, replay);
    assert_eq!(
        blocked.surface().get(stone).unwrap().position(),
        den,
        "unavailable wild forage cannot justify expelling a real den parcel"
    );
    assert_eq!(
        loose.surface().get(stone).unwrap().position(),
        junction,
        "loose edible food can be recovered without the wild forage cooldown"
    );
    assert_eq!(
        ready.surface().get(stone).unwrap().position(),
        junction,
        "a currently harvestable patch justifies the same physical cache release"
    );
    assert_eq!(
        blocked.kobold_state().colony.as_ref().unwrap().next_meal_at,
        SimulationTick::new(980)
    );

    for _ in 0..60 {
        assert_eq!(
            advance_tick(&registries, &mut blocked).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(blocked, replay);
    assert_eq!(blocked.tick(), SimulationTick::new(640));
    assert_eq!(
        blocked.surface().get(stone).unwrap().position(),
        junction,
        "once forage becomes available a hungry scout clears the cache at a real den visit"
    );
    for state in [&blocked, &loose, &ready] {
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    }
}

#[test]
fn full_den_preserves_its_inert_cache_when_fresh_food_is_not_reachable() {
    let (registries, mut state, [den, _, _, _]) = setup();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(
                VoxelCoord::new(40, 0, 40),
                SurfaceResourceKind::Opportunity,
                true,
            ),
        ],
    )
    .unwrap();
    let id = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    for _ in 0..16 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.surface().get(id).unwrap().position(), den);
    assert_eq!(
        state.surface().get(id).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn locally_observed_cache_clearing_is_real_and_player_occupied_exit_stops_it() {
    let (registries, mut state, [den, junction, _, _]) = setup();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(junction, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let stone = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, junction)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    let _ = advance_tick(&registries, &mut guarded).unwrap();
    assert_eq!(guarded.surface().get(stone).unwrap().position(), den);
    assert!(guarded.kobold_state().colony.as_ref().unwrap().hungry);

    // An elevated witness can see the den and the scout's next step without
    // physically guarding either same-height clearing. The old side observer
    // now correctly prevents cache disposal into the threatened junction.
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(junction.x(), junction.y() + 1, junction.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let event = advance_tick(&registries, &mut state)
        .unwrap()
        .kobold_scout()
        .expect("a scout and its actual local den-clearance act should be visible");
    assert_eq!(event.cleared_cache(), Some(stone));
    assert!(!event.discovered_player_drop());
    assert_eq!(state.surface().get(stone).unwrap().position(), junction);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn hungry_scout_opens_a_full_den_for_new_food_before_the_next_meal_deadline() {
    let (registries, mut responding, [den, junction, _, _]) = setup();
    let placement = validate_create_surface_resources(
        &registries,
        &responding,
        [resource(den, SurfaceResourceKind::Drop, false)],
    )
    .unwrap();
    let inert = placement.first_resource_id();
    placement.commit(&mut responding).unwrap();
    while responding.tick().value() < 595 {
        let _ = advance_tick(&registries, &mut responding).unwrap();
    }
    let resident = responding.kobold_state().colony.as_ref().unwrap();
    assert!(resident.hungry);
    assert_eq!(resident.scout_position(), den);
    assert_eq!(resident.next_meal_at, SimulationTick::new(980));
    let mut unchanged = responding.clone();
    let food_plan = validate_create_surface_resources(
        &registries,
        &responding,
        [resource(junction, SurfaceResourceKind::Opportunity, true)],
    )
    .unwrap();
    let food = food_plan.first_resource_id();
    food_plan.commit(&mut responding).unwrap();
    let original_matter = calculate_matter_accounting(&responding).unwrap().total();
    let unchanged_matter = calculate_matter_accounting(&unchanged).unwrap().total();

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &responding)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut responding).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
        let _ = advance_tick(&registries, &mut unchanged).unwrap();
    }
    assert_eq!(responding, replay);
    assert_eq!(responding.tick(), SimulationTick::new(600));
    assert_eq!(
        responding.surface().get(inert).unwrap().position(),
        junction
    );
    assert_eq!(unchanged.surface().get(inert).unwrap().position(), den);
    assert_eq!(
        responding.surface().get(food).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000),
        "clearing space alone does not forage or consume remote food"
    );
    assert_eq!(
        responding
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .next_meal_at,
        SimulationTick::new(980),
        "hunger-driven response does not move the ordinary meal deadline"
    );
    assert_eq!(
        calculate_matter_accounting(&responding).unwrap().total(),
        original_matter
    );
    assert_eq!(
        calculate_matter_accounting(&unchanged).unwrap().total(),
        unchanged_matter
    );
    assert_eq!(validate_loaded_state(&registries, &responding), Ok(()));
}

#[test]
fn unseen_side_guard_cannot_be_sensed_from_den_but_blocks_the_next_raid() {
    let (registries, mut guarded, [den, junction, _, side]) = setup();
    let plan = validate_create_surface_resources(
        &registries,
        &guarded,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(side, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let inert = plan.first_resource_id();
    plan.commit(&mut guarded).unwrap();
    let mut unguarded = guarded.clone();
    let matter = calculate_matter_accounting(&guarded).unwrap().total();
    let player = VoxelCoord::new(side.x() + 1, side.y(), side.z());
    validate_admit_player(&registries, &guarded, player)
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    assert!(!crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &guarded,
        den,
    ));
    assert!(crate::kobolds::patrol::scout_sees_guarded_cell(
        &registries,
        &guarded,
        junction,
        side,
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &guarded,
        den,
        player,
    ));
    let _ = advance_tick(&registries, &mut guarded).unwrap();
    let _ = advance_tick(&registries, &mut unguarded).unwrap();
    assert_eq!(
        guarded.surface().get(inert).unwrap().position(),
        junction,
        "the homebound scout must not act on a future view from the junction"
    );
    assert_eq!(unguarded.surface().get(inert).unwrap().position(), junction);
    for _ in 0..5 {
        let _ = advance_tick(&registries, &mut guarded).unwrap();
        let _ = advance_tick(&registries, &mut unguarded).unwrap();
    }
    assert_ne!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        side,
        "the live guard is actually discovered at the junction before side entry"
    );
    assert_eq!(
        unguarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        side,
        "an unguarded fresh food patch remains physically accessible"
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        matter
    );
    assert_eq!(
        calculate_matter_accounting(&unguarded).unwrap().total(),
        matter
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &unguarded), Ok(()));
}

#[test]
fn solid_corner_conceals_food_from_den_cache_decisions_until_opened() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut starting = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let side = VoxelCoord::new(0, 0, -1);
    let corner = VoxelCoord::new(-1, 0, -1);
    for (a, b) in [(den, junction), (junction, tip), (junction, side)] {
        add_traversable_edge_for_test(&mut starting, a, b);
    }
    starting
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    for chunk in [
        ChunkCoord::containing(den),
        ChunkCoord::containing(junction),
        ChunkCoord::containing(side),
        ChunkCoord::containing(corner),
    ] {
        validate_generate_chunk(
            &registries,
            &starting,
            WorldGenerationSeed::new(7),
            chunk,
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut starting)
        .unwrap();
    }
    for position in [den, junction, tip, side, corner] {
        set_generated_block_for_test(
            registries.world(),
            &mut starting,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut starting,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    while starting.tick().value() < 579 {
        let _ = advance_tick(&registries, &mut starting).unwrap();
    }
    assert_eq!(
        starting
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        den
    );
    let creation = validate_create_surface_resources(
        &registries,
        &starting,
        [
            resource(den, SurfaceResourceKind::Drop, false),
            resource(side, SurfaceResourceKind::Opportunity, true),
        ],
    )
    .unwrap();
    let stone = creation.first_resource_id();
    creation.commit(&mut starting).unwrap();

    let mut exposed = starting.clone();
    let mut hidden = starting;
    set_generated_block_for_test(registries.world(), &mut hidden, corner, BLOCK_STATE_STONE);
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &exposed,
        den,
        side
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        den,
        side
    ));
    let exposed_total = calculate_matter_accounting(&exposed).unwrap().total();
    let hidden_total = calculate_matter_accounting(&hidden).unwrap().total();

    let _ = advance_tick(&registries, &mut exposed).unwrap();
    let _ = advance_tick(&registries, &mut hidden).unwrap();
    assert_eq!(exposed.surface().get(stone).unwrap().position(), junction);
    assert_eq!(hidden.surface().get(stone).unwrap().position(), den);
    assert_eq!(
        calculate_matter_accounting(&exposed).unwrap().total(),
        exposed_total
    );
    assert_eq!(
        calculate_matter_accounting(&hidden).unwrap().total(),
        hidden_total
    );

    // Opening the same real corner before a later hungry home visit changes
    // the den-care choice without changing the persistent meal schedule.
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &hidden)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for state in [&mut hidden, &mut replay] {
        set_generated_block_for_test(registries.world(), state, corner, BlockStateId::AIR);
    }
    for _ in 0..20 {
        assert_eq!(
            advance_tick(&registries, &mut hidden).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(hidden, replay);
    assert_eq!(hidden.surface().get(stone).unwrap().position(), junction);
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &exposed), Ok(()));
}

#[test]
fn hungry_home_visit_discards_spoiled_food_without_waiting_another_meal_cycle() {
    use crate::content::MATERIAL_BERRIES;
    use crate::inventory::{MaterialLotProfile, MaterialLotProvenance, STORAGE_AGE_PARTS_PER_TICK};
    use crate::material::MaterialLotSpec;

    let (registries, mut state, [den, junction, _, _]) = setup();
    while state.tick().value() < 595 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert!(colony.hungry);
    assert_eq!(colony.scout_position(), den);
    assert_eq!(colony.next_meal_at, SimulationTick::new(980));

    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let mass = Mass::from_milligrams(100_000);
    let profile = MaterialLotProfile::from_spec(
        &MaterialLotSpec::with_composition(
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_BERRIES),
        )
        .unwrap(),
    );
    let shelf = registries
        .survival()
        .get_food(commodity)
        .unwrap()
        .shelf_life();
    let creation = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::from_existing_material(
            SurfaceResourceKind::Drop,
            den,
            mass,
            profile,
            MaterialLotProvenance::single(SimulationTick::ZERO),
            u128::from(shelf.value()) * STORAGE_AGE_PARTS_PER_TICK,
        )
        .unwrap()],
    )
    .unwrap();
    let spoiled = creation.first_resource_id();
    creation.commit(&mut state).unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(state.surface().get(spoiled).unwrap().position(), junction);
    assert_eq!(state.surface().get(spoiled).unwrap().remaining_mass(), mass);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().next_meal_at,
        SimulationTick::new(980),
    );
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
