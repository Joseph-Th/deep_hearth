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
fn scout_senses_guarded_next_clearing_and_retreats_without_entering_it() {
    use crate::kobolds::{KoboldTrailKind, available_nearby_kobold_tracks};
    use crate::logistics::relocate_player_for_test;

    let (registries, mut guarded, route) = scenario(EnergyCarrier::Mechanical, None);
    let food_or_scrap = put_resource(&registries, &mut guarded, route[2], false);
    let mut distant = guarded.clone();
    validate_admit_player(&registries, &guarded, VoxelCoord::new(0, 0, 2))
        .unwrap()
        .commit(&mut guarded)
        .unwrap();
    validate_admit_player(&registries, &distant, VoxelCoord::new(0, 0, 3))
        .unwrap()
        .commit(&mut distant)
        .unwrap();
    let initial_matter = calculate_matter_accounting(&guarded).unwrap().total();
    advance_to(&registries, &mut guarded, 180);
    advance_to(&registries, &mut distant, 180);
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1]
    );
    assert!(
        !crate::kobolds::patrol::living_player_deters_scout(&registries, &guarded, route[1]),
        "the player must not be in the scout's current direct fear radius"
    );
    advance_to(&registries, &mut guarded, 184);
    advance_to(&registries, &mut distant, 184);
    let guarded_event = advance_tick(&registries, &mut guarded).unwrap();
    let distant_event = advance_tick(&registries, &mut distant).unwrap();
    let watched = guarded.kobold_state().colony.as_ref().unwrap();
    assert_eq!(watched.scout_position(), route[0]);
    assert_eq!(watched.wary_until, Some(SimulationTick::new(225)));
    assert_eq!(
        guarded
            .surface()
            .get(food_or_scrap)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(750_000),
        "preemptive fear must not collect the unattended resource while fleeing"
    );
    assert_eq!(
        distant
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[2]
    );
    assert!(distant_event.kobold_scout().is_some());
    assert!(
        guarded_event.kobold_scout().is_none(),
        "retreat into the den must not leak a remote unobserved event"
    );
    assert!(
        available_nearby_kobold_tracks(&registries, &guarded).any(|trail| {
            trail.position() == route[1]
                && trail.toward() == route[0]
                && trail.observed_activity() == KoboldTrailKind::Fleeing
        })
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &guarded)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    // Remove the player after the warning. The subsequent caution must come
    // from the resident's stored experience, not a repeated proximity query.
    relocate_player_for_test(&mut guarded, VoxelCoord::new(0, 0, 4));
    relocate_player_for_test(&mut replay, VoxelCoord::new(0, 0, 4));
    advance_to(&registries, &mut guarded, 220);
    advance_to(&registries, &mut replay, 220);
    advance_to(&registries, &mut distant, 220);
    assert_eq!(guarded, replay);
    assert_eq!(
        guarded
            .surface()
            .get(food_or_scrap)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(750_000)
    );
    assert!(
        distant
            .surface()
            .get(food_or_scrap)
            .unwrap()
            .remaining_mass()
            < Mass::from_milligrams(750_000),
        "the unguarded comparison must actually salvage finite world matter"
    );
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[0]
    );
    advance_to(&registries, &mut guarded, 225);
    advance_to(&registries, &mut replay, 225);
    assert_eq!(guarded, replay);
    assert_eq!(
        guarded
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1]
    );
    assert_eq!(
        calculate_matter_accounting(&guarded).unwrap().total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &guarded), Ok(()));
}

#[test]
fn scout_cannot_anticipate_a_player_hidden_around_an_opaque_corner() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = crate::content::build_registries();
    let mut hidden = AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
    ];
    for (from, to) in [(route[0], route[1]), (route[1], route[2])] {
        add_traversable_edge_for_test(&mut hidden, from, to);
    }
    hidden.kobold_state_mut().apply_opening_colony(route);
    // The fixed actor-owned route straddles the negative-x chunk boundary.
    // Both chunks must exist so the real movement validator can check every
    // step without treating absent chunk truth as a collision.
    for coord in [
        ChunkCoord::containing(route[0]),
        ChunkCoord::containing(route[1]),
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
    let observer = VoxelCoord::new(1, 0, 1);
    let blind_corner = VoxelCoord::new(1, 0, 0);
    for cell in [route[0], route[1], route[2], observer, blind_corner] {
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
    let mut visible = hidden.clone();
    set_generated_block_for_test(
        registries.world(),
        &mut hidden,
        blind_corner,
        BLOCK_STATE_STONE,
    );
    advance_to(&registries, &mut hidden, 180);
    advance_to(&registries, &mut visible, 180);
    for state in [&mut hidden, &mut visible] {
        validate_admit_player(&registries, state, observer)
            .unwrap()
            .commit(state)
            .unwrap();
    }
    assert!(crate::kobolds::patrol::living_player_deters_scout(
        &registries,
        &hidden,
        route[2]
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        route[1],
        observer
    ));
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &visible,
        route[1],
        observer
    ));

    advance_to(&registries, &mut hidden, 185);
    advance_to(&registries, &mut visible, 185);
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[2],
        "the scout must not react to an unseen person behind the corner"
    );
    assert_eq!(
        visible
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[0],
        "an identical exposed flank should force a preemptive retreat"
    );
    advance_to(&registries, &mut hidden, 190);
    assert_eq!(
        hidden
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1],
        "the scout reacts once it actually reaches visual contact"
    );
    assert_eq!(
        crate::kobolds::validate_loaded_kobolds(&registries, &hidden),
        Ok(())
    );
    assert_eq!(
        crate::kobolds::validate_loaded_kobolds(&registries, &visible),
        Ok(())
    );
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

fn cache_stone(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
) -> crate::surface::SurfaceResourceId {
    let resource = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        den,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(registries, state, [resource]).unwrap();
    let id = plan.first_resource_id();
    plan.commit(state).unwrap();
    id
}

#[test]
fn alerted_colony_releases_low_value_cargo_and_recovers_actual_precision_copper() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    let (_, mut ordinary, _) = scenario(EnergyCarrier::Mechanical, None);
    advance_to(&registries, &mut alerted, 579);
    advance_to(&registries, &mut ordinary, 579);
    let old = cache_stone(&registries, &mut alerted, route[0]);
    let copper = copper_winding(&registries, &mut alerted, route[3]);
    let ordinary_old = cache_stone(&registries, &mut ordinary, route[0]);
    let ordinary_copper = copper_winding(&registries, &mut ordinary, route[3]);
    let conserved = calculate_matter_accounting(&alerted).unwrap().total();

    advance_to(&registries, &mut alerted, 580);
    advance_to(&registries, &mut ordinary, 580);
    assert_eq!(alerted.surface().get(old).unwrap().position(), route[1]);
    assert_eq!(
        ordinary.surface().get(ordinary_old).unwrap().position(),
        route[0]
    );
    assert_eq!(
        alerted.surface().get(old).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000),
        "cache clearing must not delete stone or pay an abstract upgrade price"
    );
    assert!(
        !alerted
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .noticed_player_drop,
        "an expelled parcel is not a newly discovered cache"
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &alerted)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut resumed = decoded.into_state(&registries).unwrap();
    for _ in 0..15 {
        assert_eq!(
            advance_tick(&registries, &mut alerted).unwrap(),
            advance_tick(&registries, &mut resumed).unwrap()
        );
    }
    assert_eq!(alerted, resumed);
    assert_eq!(alerted.surface().get(old).unwrap().position(), route[1]);
    assert_eq!(alerted.surface().get(copper).unwrap().position(), route[0]);
    assert_eq!(
        alerted.surface().get(copper).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        ordinary.surface().get(ordinary_copper).unwrap().position(),
        route[3]
    );
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        conserved
    );
    assert_eq!(
        calculate_matter_accounting(&ordinary).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &ordinary), Ok(()));
}

#[test]
fn empty_industrial_opportunity_and_a_player_occupied_den_exit_prevent_eviction() {
    let (registries, mut alerted, route) =
        scenario(EnergyCarrier::Electrical, Some(route_machinery()));
    advance_to(&registries, &mut alerted, 579);
    let stone = cache_stone(&registries, &mut alerted, route[0]);
    let mut no_target = alerted.clone();
    let winding = copper_winding(&registries, &mut alerted, route[3]);
    let conserved = calculate_matter_accounting(&alerted).unwrap().total();
    validate_admit_player(&registries, &alerted, route[1])
        .unwrap()
        .commit(&mut alerted)
        .unwrap();
    advance_to(&registries, &mut alerted, 580);
    advance_to(&registries, &mut no_target, 580);
    assert_eq!(alerted.surface().get(stone).unwrap().position(), route[0]);
    assert_eq!(no_target.surface().get(stone).unwrap().position(), route[0]);
    assert_eq!(alerted.surface().get(winding).unwrap().position(), route[3]);
    assert_eq!(
        calculate_matter_accounting(&alerted).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &alerted), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &no_target), Ok(()));
}

#[path = "watchman_tests.rs"]
mod watchman_tests;
