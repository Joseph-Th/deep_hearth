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
