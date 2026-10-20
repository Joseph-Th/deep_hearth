//! Finite scout foraging respects the actual freshness of wild source custody.

use crate::content::{FORM_FOOD, MATERIAL_BERRIES, MATERIAL_WILD_NUTS, build_registries};
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
    assert_eq!(den_food.remaining_mass(), Mass::from_milligrams(100_000));
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
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
