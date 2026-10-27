//! Finite scout foraging respects the actual freshness of wild source custody.

use crate::content::{
    FORM_CHIP, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WILD_NUTS, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::inventory::{MaterialLotProfile, MaterialLotProvenance, STORAGE_AGE_PARTS_PER_TICK};
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources,
};

struct ForageRoute {
    den: VoxelCoord,
    junction: VoxelCoord,
    tip: VoxelCoord,
    side: VoxelCoord,
}

fn setup() -> (Registries, AppState, ForageRoute) {
    let registries = build_registries();
    let mut state = AppState::new();
    let route = ForageRoute {
        den: VoxelCoord::new(-1, 0, 0),
        junction: VoxelCoord::new(0, 0, 0),
        tip: VoxelCoord::new(0, 0, 1),
        side: VoxelCoord::new(1, 0, 0),
    };
    for (a, b) in [
        (route.den, route.junction),
        (route.junction, route.tip),
        (route.junction, route.side),
    ] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([route.den, route.junction, route.tip]);
    advance_to(&registries, &mut state, 579);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.den
    );
    (registries, state, route)
}

fn advance_to(registries: &Registries, state: &mut AppState, tick: u64) {
    while state.tick().value() < tick {
        let _ = advance_tick(registries, state).unwrap();
    }
}

fn forage_spec(
    registries: &Registries,
    position: VoxelCoord,
    material: crate::material::MaterialId,
    mass: Mass,
    ticks_before_spoil: Option<u64>,
) -> SurfaceResourceSpec {
    let commodity = CommodityKey::new(material, FORM_FOOD);
    let temperature = Temperature::from_millikelvin(293_150);
    let composition = MaterialComposition::pure(material);
    if let Some(remaining) = ticks_before_spoil {
        let profile = MaterialLotProfile::from_spec(
            &MaterialLotSpec::with_composition(commodity, mass, temperature, composition).unwrap(),
        );
        let shelf = registries
            .survival()
            .get_food(commodity)
            .unwrap()
            .shelf_life();
        let elapsed = shelf.value() - remaining;
        SurfaceResourceSpec::from_existing_material(
            SurfaceResourceKind::Opportunity,
            position,
            mass,
            profile,
            MaterialLotProvenance::single(SimulationTick::ZERO),
            u128::from(elapsed) * STORAGE_AGE_PARTS_PER_TICK,
        )
        .unwrap()
    } else {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            mass,
            temperature,
            composition,
        )
        .unwrap()
    }
}

fn seed(
    registries: &Registries,
    state: &mut AppState,
    sources: impl IntoIterator<Item = SurfaceResourceSpec>,
) -> Vec<SurfaceResourceId> {
    let created = validate_create_surface_resources(registries, state, sources).unwrap();
    let ids = created.resource_ids().collect();
    created.commit(state).unwrap();
    ids
}

#[test]
fn scout_skips_older_spoiled_patch_and_forages_fresh_food_at_same_junction() {
    let (registries, mut state, route) = setup();
    let portion = Mass::from_milligrams(300_000);
    let sources = seed(
        &registries,
        &mut state,
        [
            forage_spec(
                &registries,
                route.junction,
                MATERIAL_BERRIES,
                portion,
                Some(0),
            ),
            forage_spec(
                &registries,
                route.junction,
                MATERIAL_WILD_NUTS,
                portion,
                None,
            ),
        ],
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut restored = decoded.into_state(&registries).unwrap();
    for _ in 0..16 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(
        state.surface().get(sources[0]).unwrap().remaining_mass(),
        portion
    );
    assert_eq!(
        state.surface().get(sources[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    let den_food = state
        .surface()
        .available_resources_at(Some(route.den))
        .find(|record| record.kind() == SurfaceResourceKind::Drop)
        .unwrap();
    assert_eq!(
        den_food.commodity(),
        CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD)
    );
    assert_eq!(den_food.remaining_mass(), Mass::from_milligrams(80_000));
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
fn hungry_scout_seeks_a_full_visible_ration_instead_of_a_junction_crumb() {
    let (registries, mut state, route) = setup();
    let sources = seed(
        &registries,
        &mut state,
        [
            forage_spec(
                &registries,
                route.junction,
                MATERIAL_BERRIES,
                Mass::from_milligrams(1_000),
                None,
            ),
            forage_spec(
                &registries,
                route.side,
                MATERIAL_WILD_NUTS,
                Mass::from_milligrams(300_000),
                None,
            ),
        ],
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.side,
        "a hungry scout should not spend its full forage cooldown on a 1 g crumb with a visible meal nearby"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..15 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(sources[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(1_000)
    );
    assert_eq!(
        state.surface().get(sources[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_sufficient_junction_meal_takes_priority_over_a_larger_side_patch() {
    let (registries, mut state, route) = setup();
    let sources = seed(
        &registries,
        &mut state,
        [
            forage_spec(
                &registries,
                route.junction,
                MATERIAL_BERRIES,
                Mass::from_milligrams(25_000),
                None,
            ),
            forage_spec(
                &registries,
                route.side,
                MATERIAL_WILD_NUTS,
                Mass::from_milligrams(300_000),
                None,
            ),
        ],
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.den,
        "a complete nearby ration should avoid the extra side-trip"
    );
    assert_eq!(
        state.surface().get(sources[0]).unwrap().remaining_mass(),
        Mass::ZERO
    );
    assert_eq!(
        state.surface().get(sources[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000)
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries),
        Ok(state.clone())
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn spoiled_junction_food_does_not_hide_fresh_neighboring_forage() {
    let (registries, mut state, route) = setup();
    let portion = Mass::from_milligrams(300_000);
    let sources = seed(
        &registries,
        &mut state,
        [
            forage_spec(
                &registries,
                route.junction,
                MATERIAL_BERRIES,
                portion,
                Some(0),
            ),
            forage_spec(&registries, route.side, MATERIAL_WILD_NUTS, portion, None),
        ],
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.side
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut restored = decoded.into_state(&registries).unwrap();
    for _ in 0..11 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut restored).unwrap()
        );
    }
    assert_eq!(state, restored);
    assert_eq!(
        state.surface().get(sources[0]).unwrap().remaining_mass(),
        portion
    );
    assert_eq!(
        state.surface().get(sources[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(route.den))
            .any(|record| {
                record.kind() == SurfaceResourceKind::Drop
                    && record.commodity().material() == MATERIAL_WILD_NUTS
                    && record.remaining_mass() == Mass::from_milligrams(100_000)
            })
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn food_expiring_while_scout_walks_to_patch_is_not_harvested() {
    let (registries, mut state, route) = setup();
    let portion = Mass::from_milligrams(300_000);
    let [source] = seed(
        &registries,
        &mut state,
        [forage_spec(
            &registries,
            route.side,
            MATERIAL_BERRIES,
            portion,
            Some(8),
        )],
    )
    .try_into()
    .unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.side
    );
    advance_to(&registries, &mut state, 595);
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        portion
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.den
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(route.den))
            .next()
            .is_none()
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn loose_stone(position: VoxelCoord) -> SurfaceResourceSpec {
    SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(150_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap()
}

#[test]
fn missed_meal_redirects_scout_from_inert_cargo_to_fresh_side_forage_and_catchup_meal() {
    let (registries, mut state, route) = setup();
    let sources = seed(
        &registries,
        &mut state,
        [
            loose_stone(route.junction),
            forage_spec(
                &registries,
                route.side,
                MATERIAL_WILD_NUTS,
                Mass::from_milligrams(300_000),
                None,
            ),
        ],
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert!(colony.hungry);
    assert_eq!(colony.next_meal_at, SimulationTick::new(980));
    assert_eq!(colony.scout_position(), route.side);
    assert_eq!(
        state.surface().get(sources[0]).unwrap().position(),
        route.junction
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..15 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(sources[0]).unwrap().position(),
        route.junction
    );
    assert_eq!(
        state.surface().get(sources[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
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
fn successful_first_meal_preserves_inert_scavenging_and_unearned_hunger_is_rejected() {
    let (registries, mut state, route) = setup();
    let provisions = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        route.den,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_BERRIES),
    )
    .unwrap();
    let sources = seed(
        &registries,
        &mut state,
        [
            provisions,
            loose_stone(route.junction),
            forage_spec(
                &registries,
                route.side,
                MATERIAL_WILD_NUTS,
                Mass::from_milligrams(300_000),
                None,
            ),
        ],
    );
    let total = calculate_matter_accounting(&state).unwrap().total();
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut invalid = valid.clone();
    invalid["state"]["systems"]["kobolds"]["colony"]["hungry"] = serde_json::json!(true);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(invalid).unwrap();
    assert!(decoded.into_state(&registries).is_err());
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("hungry");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());

    advance_to(&registries, &mut state, 585);
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        state.surface().get(sources[1]).unwrap().position(),
        route.den
    );
    assert_eq!(
        state.surface().get(sources[2]).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000)
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
fn two_edge_wild_food_is_carried_home_through_every_real_route_step() {
    let (registries, mut state, route) = setup();
    let far = VoxelCoord::new(2, 0, 0);
    add_traversable_edge_for_test(&mut state, route.side, far);
    let source = seed(
        &registries,
        &mut state,
        [forage_spec(
            &registries,
            far,
            MATERIAL_WILD_NUTS,
            Mass::from_milligrams(300_000),
            None,
        )],
    )[0];
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 590);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.scout_position(), far);
    assert_eq!(colony.detour_goal, Some(far));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    let catch = state
        .surface()
        .available_resources_at(Some(route.side))
        .find(|r| r.kind() == SurfaceResourceKind::Drop)
        .expect("foraging must materialize a recoverable parcel at the adjacent edge")
        .id();
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        Some(catch)
    );
    for _ in 0..10 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(state.surface().get(catch).unwrap().position(), route.den);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .detour_goal
            .is_none()
    );
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop
            .is_none()
    );
    advance_to(&registries, &mut state, 610);
    assert_eq!(
        state.surface().get(catch).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap()
            .consumed()
            .milligrams(),
        20_000
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn far_food_expiring_during_actual_walk_cancels_the_trip_without_depletion() {
    let (registries, mut state, route) = setup();
    let far = VoxelCoord::new(2, 0, 0);
    add_traversable_edge_for_test(&mut state, route.side, far);
    let mass = Mass::from_milligrams(300_000);
    let source = seed(
        &registries,
        &mut state,
        [forage_spec(
            &registries,
            far,
            MATERIAL_BERRIES,
            mass,
            Some(8),
        )],
    )[0];
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().detour_goal,
        Some(far)
    );
    advance_to(&registries, &mut state, 590);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.junction
    );
    assert_eq!(state.surface().get(source).unwrap().remaining_mass(), mass);
    advance_to(&registries, &mut state, 600);
    assert_eq!(
        state
            .surface()
            .available_resources_at(Some(route.den))
            .count(),
        0
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn hungry_scout_hauls_inert_cache_if_no_edible_food_is_reachable() {
    let (registries, mut state, route) = setup();
    let stone = seed(&registries, &mut state, [loose_stone(route.side)])[0];
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 585);
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route.side
    );
    advance_to(&registries, &mut state, 595);
    assert_eq!(state.surface().get(stone).unwrap().position(), route.den);
    assert!(state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(150_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
