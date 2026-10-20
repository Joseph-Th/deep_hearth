//! Mobilization follows witnessed industry, persistent memory, and finite physical work.

use super::*;
use crate::content::{
    FORM_CHIP, FORM_ELECTRICAL_WINDING, FORM_FOOD, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WILD_NUTS, make_test_registries_with_energy_store,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::validate_loaded_state;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store_with_initial_for_fixture,
};
use crate::kobolds::available_nearby_kobold_scouts;
use crate::logistics::{add_traversable_edge_for_test, place_energy_store_for_fixture};
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

fn scenario(
    carrier: EnergyCarrier,
    machine_at: Option<VoxelCoord>,
) -> (Registries, AppState, [VoxelCoord; 4]) {
    let machine = EnergyStoreDefinitionId::new(930_607);
    let registries =
        make_test_registries_with_energy_store(EnergyStoreDefinition::new_with_transfer_limits(
            machine,
            "installed machine",
            carrier,
            Energy::from_nanojoules(1_000),
            Power::ZERO,
            Power::from_microwatts(10),
        ));
    let mut state = AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(1, 0, 0),
    ];
    for (a, b) in [
        (route[0], route[1]),
        (route[1], route[2]),
        (route[1], route[3]),
    ] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([route[0], route[1], route[2]]);
    if let Some(position) = machine_at {
        let store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            machine,
            Energy::ZERO,
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, store, position);
    }
    (registries, state, route)
}

fn put_resource(
    registries: &Registries,
    state: &mut AppState,
    position: VoxelCoord,
    edible: bool,
) -> crate::surface::SurfaceResourceId {
    let (kind, commodity, mass) = if edible {
        (
            SurfaceResourceKind::Opportunity,
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(500_000),
        )
    } else {
        (
            SurfaceResourceKind::Drop,
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(750_000),
        )
    };
    let source = SurfaceResourceSpec::new(
        kind,
        position,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(commodity.material()),
    )
    .unwrap();
    let planned = validate_create_surface_resources(registries, state, [source]).unwrap();
    let id = planned.first_resource_id();
    planned.commit(state).unwrap();
    id
}

fn advance_to(registries: &Registries, state: &mut AppState, tick: u64) {
    while state.tick().value() < tick {
        if state.logistics().player().is_some() {
            let _ = advance_control_tick(registries, state).unwrap();
        } else {
            let _ = advance_tick(registries, state).unwrap();
        }
    }
}

#[test]
fn seeing_industry_mobilizes_larger_real_salvage_and_survives_save_replay() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut normal, _) = scenario(EnergyCarrier::Mechanical, None);
    advance_to(&registries, &mut alerted, 190);
    advance_to(&registries, &mut normal, 190);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .witnessed_machinery_at,
        Some(SimulationTick::new(180))
    );
    assert_eq!(
        alerted.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None,
        "knowledge from the scout must not alter the colony before a real return"
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&checkpoint).unwrap();
    let mut continuation = decoded.into_state(&registries).unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut alerted).unwrap(),
            advance_tick(&registries, &mut continuation).unwrap()
        );
    }
    assert_eq!(alerted, continuation);
    advance_to(&registries, &mut normal, 195);
    assert_eq!(
        alerted.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(195))
    );
    let a = put_resource(&registries, &mut alerted, route[3], false);
    let b = put_resource(&registries, &mut normal, route[3], false);
    let total = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 210);
    advance_to(&registries, &mut normal, 210);
    assert_eq!(
        normal.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None
    );
    assert_eq!(
        alerted.surface().get(a).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    assert_eq!(
        normal.surface().get(b).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    let haul = alerted
        .surface()
        .available_resources_at(Some(route[1]))
        .find(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .unwrap()
        .id();
    assert_eq!(
        alerted.surface().get(haul).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..5 {
        assert_eq!(
            advance_tick(&registries, &mut alerted).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(alerted, replay);
    assert_eq!(alerted.surface().get(haul).unwrap().position(), route[0]);
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        total
    );
    validate_admit_player(&registries, &alerted, route[2])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    assert!(
        available_nearby_kobold_scouts(&registries, &alerted)
            .next()
            .is_some_and(|sighting| sighting.is_mobilized())
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
}

#[test]
fn only_nearby_engineered_machinery_triggers_higher_food_provisioning() {
    for (carrier, position) in [
        (EnergyCarrier::Thermal, VoxelCoord::new(0, 0, 0)),
        (EnergyCarrier::Electrical, VoxelCoord::new(100, 0, 0)),
    ] {
        let (registries, mut state, _) = scenario(carrier, Some(position));
        advance_to(&registries, &mut state, 195);
        assert!(
            state
                .kobold_state()
                .colony
                .as_ref()
                .unwrap()
                .mobilized_at
                .is_none()
        );
    }
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut normal, _) = scenario(EnergyCarrier::Electrical, None);
    advance_to(&registries, &mut alerted, 195);
    advance_to(&registries, &mut normal, 195);
    let fresh = put_resource(&registries, &mut alerted, route[1], true);
    let baseline = put_resource(&registries, &mut normal, route[1], true);
    let total = calculate_matter_accounting(&alerted).unwrap().total();
    advance_to(&registries, &mut alerted, 215);
    advance_to(&registries, &mut normal, 215);
    assert_eq!(
        alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .next_forage_at,
        Some(SimulationTick::new(265))
    );
    assert_eq!(
        normal
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .next_forage_at,
        Some(SimulationTick::new(315))
    );
    assert_eq!(
        alerted.surface().get(fresh).unwrap().remaining_mass(),
        Mass::from_milligrams(300_000)
    );
    assert_eq!(
        normal.surface().get(baseline).unwrap().remaining_mass(),
        Mass::from_milligrams(400_000)
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        total
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &alerted)).unwrap();
    for fake in [179_u64, 181, 400] {
        let mut altered = saved.clone();
        altered["state"]["systems"]["kobolds"]["colony"]["mobilized_at"] = serde_json::json!(fake);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(altered).unwrap();
        assert!(decoded.into_state(&registries).is_err());
    }
    let mut missing = saved;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("mobilized_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
    let mut missing_witness =
        serde_json::to_value(SaveEnvelope::new(&registries, &alerted)).unwrap();
    missing_witness["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("witnessed_machinery_at");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing_witness).is_err());
}

#[test]
fn player_guarded_den_blocks_the_scouts_industrial_report() {
    // This machine cannot be seen from the den: only the scout's real walk
    // to the junction gives it information about the player's advancement.
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 2)));
    advance_to(&registries, &mut state, 190);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .witnessed_machinery_at,
        Some(SimulationTick::new(185))
    );
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1]
    );
    let mut guarded = state.clone();
    validate_admit_player(&registries, &guarded, route[0])
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&snapshot).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();

    advance_to(&registries, &mut state, 195);
    advance_to(&registries, &mut guarded, 195);
    advance_to(&registries, &mut replay, 195);
    assert_eq!(state, replay);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().mobilized_at,
        Some(SimulationTick::new(195))
    );
    assert_eq!(
        guarded.kobold_state().colony.as_ref().unwrap().mobilized_at,
        None
    );
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1],
        "an occupied den cannot receive a report from a scout still outside"
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));

    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged_early = valid.clone();
    forged_early["state"]["systems"]["kobolds"]["colony"]["mobilized_at"] = serde_json::json!(185);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged_early).unwrap();
    assert!(decoded.into_state(&registries).is_err());
    let mut forged_witness = valid;
    forged_witness["state"]["systems"]["kobolds"]["colony"]["witnessed_machinery_at"] =
        serde_json::json!(200);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged_witness).unwrap();
    assert!(decoded.into_state(&registries).is_err());
}

fn copper_winding(
    registries: &Registries,
    state: &mut AppState,
    at: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    let commodity = CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING);
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        at,
        commodity,
        Mass::from_milligrams(120_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap();
    let creation = validate_create_surface_resources(registries, state, [source]).unwrap();
    let id = creation.first_resource_id();
    creation.commit(state).unwrap();
    id
}

#[test]
fn mobilized_scout_prefers_shaped_copper_over_equally_portable_stone_at_one_stop() {
    let (registries, mut mobilized, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    let (_, mut ordinary, _) = scenario(EnergyCarrier::Mechanical, None);
    advance_to(&registries, &mut mobilized, 195);
    advance_to(&registries, &mut ordinary, 195);
    let mut ids = Vec::new();
    for state in [&mut mobilized, &mut ordinary] {
        let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
        let stone_spec = SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[2],
            stone_commodity,
            Mass::from_milligrams(120_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap();
        let plan = validate_create_surface_resources(&registries, state, [stone_spec]).unwrap();
        let stone = plan.first_resource_id();
        plan.commit(state).unwrap();
        let copper = copper_winding(&registries, state, route[2]);
        ids.push((stone, copper));
    }
    let conserved = calculate_matter_accounting(&mobilized).unwrap().total();
    advance_to(&registries, &mut mobilized, 210);
    advance_to(&registries, &mut ordinary, 210);
    assert_eq!(
        mobilized.surface().get(ids[0].0).unwrap().position(),
        route[2]
    );
    assert_eq!(
        mobilized.surface().get(ids[0].1).unwrap().position(),
        route[1]
    );
    assert_eq!(
        ordinary.surface().get(ids[1].0).unwrap().position(),
        route[1]
    );
    assert_eq!(
        ordinary.surface().get(ids[1].1).unwrap().position(),
        route[2]
    );
    advance_to(&registries, &mut mobilized, 215);
    advance_to(&registries, &mut ordinary, 215);
    assert_eq!(
        mobilized.surface().get(ids[0].1).unwrap().position(),
        route[0]
    );
    assert_eq!(
        ordinary.surface().get(ids[1].0).unwrap().position(),
        route[0]
    );
    assert_eq!(
        calculate_matter_accounting(&mobilized).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&ordinary).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &mobilized), Ok(()));
}

fn add_salvage_routes(state: &mut AppState, route: [VoxelCoord; 4]) -> (VoxelCoord, VoxelCoord) {
    let stone = VoxelCoord::new(0, 0, -1);
    let winding = VoxelCoord::new(2, 0, 0);
    add_traversable_edge_for_test(state, route[1], stone);
    add_traversable_edge_for_test(state, route[3], winding);
    (stone, winding)
}

#[test]
fn mobilized_scout_targets_far_engineered_winding_over_nearer_raw_stone() {
    let (registries, mut mobilized, route) =
        scenario(EnergyCarrier::Electrical, Some(route_machinery()));
    let (_, mut ordinary, _) = scenario(EnergyCarrier::Electrical, None);
    advance_to(&registries, &mut mobilized, 195);
    advance_to(&registries, &mut ordinary, 195);
    let (near, far) = add_salvage_routes(&mut mobilized, route);
    add_salvage_routes(&mut ordinary, route);
    let stone = put_resource(&registries, &mut mobilized, near, false);
    let winding = copper_winding(&registries, &mut mobilized, far);
    let ordinary_stone = put_resource(&registries, &mut ordinary, near, false);
    let ordinary_winding = copper_winding(&registries, &mut ordinary, far);
    let conserved = calculate_matter_accounting(&mobilized).unwrap().total();
    advance_to(&registries, &mut mobilized, 205);
    advance_to(&registries, &mut ordinary, 205);
    assert_eq!(
        mobilized
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[3]
    );
    assert_eq!(
        mobilized
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .detour_goal,
        Some(far)
    );
    assert_eq!(
        ordinary
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        near
    );
    advance_to(&registries, &mut mobilized, 210);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &mobilized)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..15 {
        assert_eq!(
            advance_tick(&registries, &mut mobilized).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(mobilized, replay);
    assert_eq!(
        mobilized.surface().get(winding).unwrap().position(),
        route[0]
    );
    assert_eq!(
        mobilized.surface().get(winding).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(mobilized.surface().get(stone).unwrap().position(), near);
    advance_to(&registries, &mut ordinary, 225);
    assert_eq!(
        ordinary.surface().get(ordinary_winding).unwrap().position(),
        far
    );
    assert_eq!(
        ordinary
            .surface()
            .get(ordinary_stone)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(
        calculate_matter_accounting(&mobilized).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&ordinary).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &mobilized), Ok(()));
}

fn route_machinery() -> VoxelCoord {
    VoxelCoord::new(0, 0, 0)
}

#[test]
fn player_intercepts_engineered_salvage_and_mobilized_raider_takes_raw_scraps() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(route_machinery()));
    advance_to(&registries, &mut state, 195);
    let (near, far) = add_salvage_routes(&mut state, route);
    let stone = put_resource(&registries, &mut state, near, false);
    let winding = copper_winding(&registries, &mut state, far);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    validate_admit_player(&registries, &state, far)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        near
    );
    assert_eq!(state.surface().get(winding).unwrap().position(), far);
    advance_to(&registries, &mut state, 215);
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(250_000)
    );
    // Ordinary player-local pickup, not secret kobold immunity, protects the
    // valuable part. It stays in exact player stockpile custody.
    assert_eq!(
        state.surface().get(winding).unwrap().remaining_mass(),
        Mass::ZERO
    );
    let carrier = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carrier)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING)),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
