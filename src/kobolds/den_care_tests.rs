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
fn a_side_food_patch_guarded_by_a_visible_player_does_not_displace_den_cargo() {
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
    let _ = advance_tick(&registries, &mut guarded).unwrap();
    let _ = advance_tick(&registries, &mut unguarded).unwrap();
    assert_eq!(guarded.surface().get(inert).unwrap().position(), den);
    assert_eq!(unguarded.surface().get(inert).unwrap().position(), junction);
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
