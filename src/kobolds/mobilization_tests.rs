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
fn opening_a_stone_screen_exposes_an_intact_far_part_over_near_split_copper() {
    use crate::content::{BLOCK_STATE_STONE, ENERGY_ELECTRICAL_BUFFER, build_registries};
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut visible = AppState::new();
    let route = [
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(1, 0, 0),
    ];
    for (start, end) in [
        (route[0], route[1]),
        (route[1], route[2]),
        (route[1], route[3]),
    ] {
        add_traversable_edge_for_test(&mut visible, start, end);
    }
    visible
        .kobold_state_mut()
        .apply_opening_colony([route[0], route[1], route[2]]);
    let machine = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut visible,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut visible, machine, route[1]);
    advance_to(&registries, &mut visible, 579);
    let first = VoxelCoord::new(0, 0, -1);
    let far = VoxelCoord::new(0, 0, -2);
    add_traversable_edge_for_test(&mut visible, route[1], first);
    add_traversable_edge_for_test(&mut visible, first, far);
    let mut chunks = Vec::new();
    for position in [route[0], route[1], route[2], route[3], first, far] {
        let chunk = ChunkCoord::containing(position);
        if !chunks.contains(&chunk) {
            validate_generate_chunk(
                &registries,
                &visible,
                WorldGenerationSeed::new(7),
                chunk,
                BLOCK_STATE_STONE,
            )
            .unwrap()
            .commit(&mut visible)
            .unwrap();
            chunks.push(chunk);
        }
    }
    for position in [route[0], route[1], route[2], route[3], first, far] {
        set_generated_block_for_test(
            registries.world(),
            &mut visible,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut visible,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    let copper = |position, amount| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
            Mass::from_milligrams(amount),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &visible,
        [copper(route[3], 700_000), copper(far, 120_000)],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut visible).unwrap();
    let mut screened = visible.clone();
    set_generated_block_for_test(registries.world(), &mut screened, first, BLOCK_STATE_STONE);
    assert!(super::super::visibility::scout_visible_from(
        &registries,
        &visible,
        route[1],
        far
    ));
    assert!(!super::super::visibility::scout_visible_from(
        &registries,
        &screened,
        route[1],
        far
    ));
    let open_total = calculate_matter_accounting(&visible).unwrap().total();
    let screened_total = calculate_matter_accounting(&screened).unwrap().total();
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &visible)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    advance_to(&registries, &mut visible, 585);
    advance_to(&registries, &mut screened, 585);
    advance_to(&registries, &mut replay, 585);
    assert_eq!(visible, replay);
    assert_eq!(
        visible
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        first,
        "a real clear corridor reveals intact copper two edges out"
    );
    assert_eq!(
        visible.kobold_state().colony.as_ref().unwrap().detour_goal,
        Some(far)
    );
    assert_eq!(
        screened
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[3],
        "a stone screen blocks the distant target, leaving only the nearer split source"
    );
    advance_to(&registries, &mut visible, 605);
    advance_to(&registries, &mut replay, 605);
    assert_eq!(visible, replay);
    assert_eq!(visible.surface().get(ids[1]).unwrap().position(), route[0]);
    assert_eq!(
        visible.surface().get(ids[1]).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        visible.surface().get(ids[0]).unwrap().remaining_mass(),
        Mass::from_milligrams(700_000)
    );
    assert_eq!(screened.surface().get(ids[1]).unwrap().position(), far);
    assert_eq!(
        calculate_matter_accounting(&visible).unwrap().total(),
        open_total
    );
    assert_eq!(
        calculate_matter_accounting(&screened).unwrap().total(),
        screened_total
    );
    assert_eq!(validate_loaded_state(&registries, &visible), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &screened), Ok(()));
}

#[test]
fn far_raider_exchanges_inert_cargo_for_food_on_a_real_intermediate_edge() {
    let (registries, mut state, route) = scenario(EnergyCarrier::Mechanical, None);
    let far = VoxelCoord::new(2, 0, 0);
    add_traversable_edge_for_test(&mut state, route[3], far);
    advance_to(&registries, &mut state, 579);
    let create = |position, kind, material, form, mass| {
        SurfaceResourceSpec::new(
            kind,
            position,
            CommodityKey::new(material, form),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(material),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [create(
            far,
            SurfaceResourceKind::Drop,
            MATERIAL_STONE,
            FORM_CHIP,
            200_000,
        )],
    )
    .unwrap();
    let inert = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    advance_to(&registries, &mut state, 595);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[3]
    );
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        Some(inert)
    );
    assert_eq!(state.surface().get(inert).unwrap().position(), route[3]);

    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [create(
            route[3],
            SurfaceResourceKind::Opportunity,
            MATERIAL_WILD_NUTS,
            FORM_FOOD,
            300_000,
        )],
    )
    .unwrap();
    let patch = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 600);
    advance_to(&registries, &mut replay, 600);
    assert_eq!(state, replay);
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1]
    );
    assert_eq!(
        state.surface().get(inert).unwrap().position(),
        route[3],
        "the discarded scrap retains its precise side-branch location"
    );
    assert_eq!(
        state.surface().get(patch).unwrap().remaining_mass(),
        Mass::from_milligrams(200_000)
    );
    let carried_food = state
        .surface()
        .available_resources_at(Some(route[1]))
        .find(|record| record.commodity().form() == FORM_FOOD)
        .unwrap()
        .id();
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().scavenged_drop,
        Some(carried_food)
    );
    advance_to(&registries, &mut state, 610);
    advance_to(&registries, &mut replay, 610);
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(carried_food).unwrap().position(),
        route[0]
    );
    assert_eq!(
        state.surface().get(carried_food).unwrap().remaining_mass(),
        Mass::from_milligrams(80_000)
    );
    assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);

    let recovered = validate_admit_player(&registries, &state, route[3])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let pickup = advance_control_tick(&registries, &mut state)
        .unwrap()
        .surface_drop_pickup()
        .expect("the abandoned parcel must be available for ordinary player pickup");
    assert_eq!(pickup.resource(), inert);
    assert_eq!(pickup.picked_mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovered.carried_stockpile())
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn hungry_returning_scout_switches_from_scrap_to_fresh_local_food() {
    let (registries, mut carrying, route) = scenario(EnergyCarrier::Mechanical, None);
    advance_to(&registries, &mut carrying, 579);
    let stone = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        route[2],
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &carrying, [stone]).unwrap();
    let inert = plan.first_resource_id();
    plan.commit(&mut carrying).unwrap();
    advance_to(&registries, &mut carrying, 590);
    assert_eq!(
        carrying
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1]
    );
    assert_eq!(
        carrying
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scavenged_drop,
        Some(inert)
    );
    assert!(carrying.kobold_state().colony.as_ref().unwrap().hungry);
    assert_eq!(carrying.surface().get(inert).unwrap().position(), route[1]);

    let mut loose = carrying.clone();
    let mut wild = carrying.clone();
    let mut oversized = carrying.clone();
    let unchanged = carrying.clone();
    let food = |kind, mass| {
        SurfaceResourceSpec::new(
            kind,
            route[1],
            CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_WILD_NUTS),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &loose,
        [food(SurfaceResourceKind::Drop, 60_000)],
    )
    .unwrap();
    let loose_id = plan.first_resource_id();
    plan.commit(&mut loose).unwrap();
    let plan = validate_create_surface_resources(
        &registries,
        &wild,
        [food(SurfaceResourceKind::Opportunity, 500_000)],
    )
    .unwrap();
    let patch = plan.first_resource_id();
    plan.commit(&mut wild).unwrap();
    let plan = validate_create_surface_resources(
        &registries,
        &oversized,
        [food(SurfaceResourceKind::Drop, 700_000)],
    )
    .unwrap();
    let large = plan.first_resource_id();
    plan.commit(&mut oversized).unwrap();
    let totals = [
        calculate_matter_accounting(&unchanged).unwrap().total(),
        calculate_matter_accounting(&loose).unwrap().total(),
        calculate_matter_accounting(&wild).unwrap().total(),
        calculate_matter_accounting(&oversized).unwrap().total(),
    ];
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &wild)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut carrying, 595);
    advance_to(&registries, &mut loose, 595);
    advance_to(&registries, &mut wild, 595);
    advance_to(&registries, &mut oversized, 595);
    advance_to(&registries, &mut replay, 595);
    assert_eq!(wild, replay);

    assert_eq!(
        carrying.surface().get(inert).unwrap().position(),
        route[0],
        "without food, a scout must continue to drag the same inert parcel"
    );
    for state in [&loose, &wild, &oversized] {
        assert_eq!(
            state.surface().get(inert).unwrap().position(),
            route[1],
            "switching to nearby food leaves the original scrap recoverable here"
        );
        assert_eq!(
            state
                .kobold_state()
                .colony
                .as_ref()
                .unwrap()
                .scout_position(),
            route[0]
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
    }
    assert_eq!(loose.surface().get(loose_id).unwrap().position(), route[0]);
    assert_eq!(
        loose.surface().get(loose_id).unwrap().remaining_mass(),
        Mass::from_milligrams(60_000)
    );
    assert_eq!(
        wild.surface().get(patch).unwrap().remaining_mass(),
        Mass::from_milligrams(400_000)
    );
    assert!(
        wild.surface()
            .available_resources_at(Some(route[0]))
            .any(|resource| resource.commodity().form() == FORM_FOOD
                && resource.remaining_mass() == Mass::from_milligrams(100_000))
    );
    assert_eq!(
        oversized.surface().get(large).unwrap().remaining_mass(),
        Mass::from_milligrams(450_000)
    );
    assert!(
        oversized
            .surface()
            .available_resources_at(Some(route[0]))
            .any(|resource| resource.commodity().form() == FORM_FOOD
                && resource.remaining_mass() == Mass::from_milligrams(250_000))
    );

    advance_to(&registries, &mut loose, 600);
    advance_to(&registries, &mut wild, 600);
    advance_to(&registries, &mut oversized, 600);
    advance_to(&registries, &mut replay, 600);
    assert_eq!(wild, replay);
    for (state, expected) in [
        (&loose, totals[1]),
        (&wild, totals[2]),
        (&oversized, totals[3]),
    ] {
        assert!(!state.kobold_state().colony.as_ref().unwrap().hungry);
        assert_eq!(
            calculate_matter_accounting(state)
                .unwrap()
                .consumed()
                .milligrams(),
            20_000
        );
        assert_eq!(
            calculate_matter_accounting(state).unwrap().total(),
            expected
        );
        assert_eq!(validate_loaded_state(&registries, state), Ok(()));
    }
    assert_eq!(
        calculate_matter_accounting(&carrying).unwrap().total(),
        totals[0]
    );
    assert_eq!(
        calculate_matter_accounting(&unchanged).unwrap().total(),
        totals[0]
    );
}

#[test]
fn intact_far_precision_part_beats_near_split_and_preserves_den_stock() {
    let (registries, mut with_far, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut with_far, 579);
    let first = VoxelCoord::new(0, 0, -1);
    let far = VoxelCoord::new(-1, 0, -1);
    add_traversable_edge_for_test(&mut with_far, route[1], first);
    add_traversable_edge_for_test(&mut with_far, first, far);
    let spec = |at, material, form, mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            at,
            CommodityKey::new(material, form),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(material),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &with_far,
        [
            spec(route[0], MATERIAL_STONE, FORM_CHIP, 375_000),
            spec(route[3], MATERIAL_COPPER, FORM_ELECTRICAL_WINDING, 700_000),
        ],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut with_far).unwrap();
    let mut only_near = with_far.clone();
    let plan = validate_create_surface_resources(
        &registries,
        &with_far,
        [spec(far, MATERIAL_COPPER, FORM_ELECTRICAL_WINDING, 120_000)],
    )
    .unwrap();
    let whole = plan.first_resource_id();
    plan.commit(&mut with_far).unwrap();
    let total_far = calculate_matter_accounting(&with_far).unwrap().total();
    let total_near = calculate_matter_accounting(&only_near).unwrap().total();
    let capacity = Mass::from_milligrams(125_000);
    assert_eq!(
        super::super::scavenging::reachable_inherited_salvage(
            &registries,
            &with_far,
            super::super::scavenging::ScavengeRoute::new([route[0], route[1], route[2]], None),
            route[0],
            capacity,
            with_far.tick(),
        ),
        Some((first, Some(far))),
        "an intact winding one authorized extra edge away wins over splitting 700 g of side copper"
    );
    assert_eq!(
        super::super::scavenging::reachable_inherited_salvage(
            &registries,
            &only_near,
            super::super::scavenging::ScavengeRoute::new([route[0], route[1], route[2]], None),
            route[0],
            capacity,
            only_near.tick(),
        ),
        Some((route[3], None)),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &with_far)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut with_far, 580);
    advance_to(&registries, &mut replay, 580);
    advance_to(&registries, &mut only_near, 580);
    assert_eq!(with_far, replay);
    assert_eq!(
        with_far.surface().get(ids[0]).unwrap().position(),
        route[0],
        "a full visible component fits 125 g of already-free den space"
    );
    assert_eq!(
        only_near.surface().get(ids[0]).unwrap().position(),
        route[1],
        "without an intact alternative, the scout can still free finite space for a split"
    );

    advance_to(&registries, &mut with_far, 605);
    advance_to(&registries, &mut replay, 605);
    assert_eq!(with_far, replay);
    assert_eq!(with_far.surface().get(whole).unwrap().position(), route[0]);
    assert_eq!(
        with_far.surface().get(whole).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(with_far.surface().get(ids[0]).unwrap().position(), route[0]);
    assert_eq!(with_far.surface().get(ids[1]).unwrap().position(), route[3]);
    assert_eq!(
        calculate_matter_accounting(&with_far).unwrap().total(),
        total_far
    );
    assert_eq!(
        calculate_matter_accounting(&only_near).unwrap().total(),
        total_near
    );
    assert_eq!(validate_loaded_state(&registries, &with_far), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &only_near), Ok(()));
}

#[test]
fn nearer_whole_precision_part_still_wins_over_farther_whole() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut state, 579);
    let first = VoxelCoord::new(0, 0, -1);
    let far = VoxelCoord::new(-1, 0, -1);
    add_traversable_edge_for_test(&mut state, route[1], first);
    add_traversable_edge_for_test(&mut state, first, far);
    let spec = |position, amount| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            position,
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
            Mass::from_milligrams(amount),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap()
    };
    validate_create_surface_resources(
        &registries,
        &state,
        [spec(route[3], 100_000), spec(far, 120_000)],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        super::super::scavenging::reachable_inherited_salvage(
            &registries,
            &state,
            super::super::scavenging::ScavengeRoute::new([route[0], route[1], route[2]], None),
            route[1],
            Mass::from_milligrams(125_000),
            state.tick(),
        ),
        Some((route[3], None)),
        "an intact component next door is cheaper to haul than an intact two-edge alternative"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
fn mobilized_scout_keeps_far_copper_goal_when_raw_scrap_appears_on_its_route() {
    let (registries, mut world, route) =
        scenario(EnergyCarrier::Electrical, Some(route_machinery()));
    advance_to(&registries, &mut world, 195);
    let far = VoxelCoord::new(2, 0, 0);
    add_traversable_edge_for_test(&mut world, route[3], far);
    let far_id = copper_winding(&registries, &mut world, far);
    advance_to(&registries, &mut world, 205);
    assert_eq!(
        world
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[3]
    );
    assert_eq!(
        world.kobold_state().colony.as_ref().unwrap().detour_goal,
        Some(far)
    );
    let mut scrap = world.clone();
    let mut precision = world.clone();
    let local = |material, form| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[3],
            CommodityKey::new(material, form),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(material),
        )
        .unwrap()
    };
    let plan =
        validate_create_surface_resources(&registries, &scrap, [local(MATERIAL_STONE, FORM_CHIP)])
            .unwrap();
    let scrap_id = plan.first_resource_id();
    plan.commit(&mut scrap).unwrap();
    let plan = validate_create_surface_resources(
        &registries,
        &precision,
        [local(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING)],
    )
    .unwrap();
    let near_prec = plan.first_resource_id();
    plan.commit(&mut precision).unwrap();
    let conserved_scrap = calculate_matter_accounting(&scrap).unwrap().total();
    let conserved_prec = calculate_matter_accounting(&precision).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &scrap)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut scrap, 210);
    advance_to(&registries, &mut precision, 210);
    advance_to(&registries, &mut replay, 210);
    assert_eq!(scrap, replay);
    assert_eq!(
        scrap
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        far,
        "ordinary newly exposed raw stone does not supersede witnessed copper"
    );
    assert_eq!(scrap.surface().get(scrap_id).unwrap().position(), route[3]);
    assert_eq!(
        precision
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        route[1],
        "a newly exposed intact precision part is the better, closer pickup"
    );
    assert_eq!(
        precision.surface().get(near_prec).unwrap().position(),
        route[1]
    );
    advance_to(&registries, &mut scrap, 225);
    advance_to(&registries, &mut replay, 225);
    advance_to(&registries, &mut precision, 225);
    assert_eq!(scrap, replay);
    assert_eq!(scrap.surface().get(far_id).unwrap().position(), route[0]);
    assert_eq!(
        precision.surface().get(near_prec).unwrap().position(),
        route[0]
    );
    assert_eq!(precision.surface().get(far_id).unwrap().position(), far);
    assert_eq!(
        calculate_matter_accounting(&scrap).unwrap().total(),
        conserved_scrap
    );
    assert_eq!(
        calculate_matter_accounting(&precision).unwrap().total(),
        conserved_prec
    );
    assert_eq!(validate_loaded_state(&registries, &scrap), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &precision), Ok(()));
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
fn a_partly_free_industrial_den_only_evicts_if_intact_salvage_does_not_fit() {
    let (registries, mut short, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut short, 579);
    let raw = |mass| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[0],
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(mass),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(&registries, &short, [raw(250_000), raw(225_000)])
        .unwrap();
    let parcels = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut short).unwrap();
    let short_copper = copper_winding(&registries, &mut short, route[3]);
    let mut spacious = scenario(EnergyCarrier::Mechanical, Some(route_machinery())).1;
    advance_to(&registries, &mut spacious, 579);
    let plan = validate_create_surface_resources(&registries, &spacious, [raw(375_000)]).unwrap();
    let spacious_stone = plan.first_resource_id();
    plan.commit(&mut spacious).unwrap();
    let spacious_copper = copper_winding(&registries, &mut spacious, route[3]);
    let short_total = calculate_matter_accounting(&short).unwrap().total();
    let spacious_total = calculate_matter_accounting(&spacious).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &short)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    advance_to(&registries, &mut short, 580);
    advance_to(&registries, &mut spacious, 580);
    advance_to(&registries, &mut replay, 580);
    assert_eq!(short, replay);
    assert_eq!(
        short.surface().get(parcels[0]).unwrap().position(),
        route[0]
    );
    assert_eq!(
        short.surface().get(parcels[1]).unwrap().position(),
        route[1],
        "25 g was already free, so the smaller 225 g parcel can make enough room for 120 g copper"
    );
    assert_eq!(
        spacious.surface().get(spacious_stone).unwrap().position(),
        route[0],
        "125 g of free space already fits an intact winding and must not trigger eviction"
    );
    advance_to(&registries, &mut short, 595);
    advance_to(&registries, &mut spacious, 595);
    advance_to(&registries, &mut replay, 595);
    assert_eq!(short, replay);
    assert_eq!(
        short.surface().get(short_copper).unwrap().position(),
        route[0]
    );
    assert_eq!(
        spacious.surface().get(spacious_copper).unwrap().position(),
        route[0]
    );
    assert_eq!(
        short.surface().get(short_copper).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        spacious
            .surface()
            .get(spacious_copper)
            .unwrap()
            .remaining_mass(),
        Mass::from_milligrams(120_000)
    );
    assert_eq!(
        calculate_matter_accounting(&short).unwrap().total(),
        short_total
    );
    assert_eq!(
        calculate_matter_accounting(&spacious).unwrap().total(),
        spacious_total
    );
    assert_eq!(validate_loaded_state(&registries, &short), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &spacious), Ok(()));
}

#[test]
fn industrial_den_frees_just_enough_raw_stock_for_an_intact_component() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut state, 579);
    let raw = |milligrams| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[0],
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(milligrams),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [raw(5_000), raw(175_000), raw(320_000)],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    let copper = copper_winding(&registries, &mut state, route[3]);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let snapshot = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 595);
    advance_to(&registries, &mut replay, 595);
    assert_eq!(state, replay);
    assert_eq!(state.surface().get(ids[0]).unwrap().position(), route[0]);
    assert_eq!(
        state.surface().get(ids[1]).unwrap().position(),
        route[1],
        "the 175 g raw parcel can fit the full 120 g winding in one return"
    );
    assert_eq!(
        state.surface().get(ids[2]).unwrap().position(),
        route[0],
        "unnecessary 320 g eviction would sacrifice too much local material"
    );
    assert_eq!(state.surface().get(copper).unwrap().position(), route[0]);
    assert_eq!(
        state.surface().get(copper).unwrap().remaining_mass(),
        Mass::from_milligrams(120_000),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn oversized_engineered_salvage_uses_the_largest_recoverable_den_space() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(route_machinery()));
    advance_to(&registries, &mut state, 579);
    let raw = |milligrams| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[0],
            CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
            Mass::from_milligrams(milligrams),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()
    };
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [raw(5_000), raw(175_000), raw(320_000)],
    )
    .unwrap();
    let ids = plan.resource_ids().collect::<Vec<_>>();
    plan.commit(&mut state).unwrap();
    let plan = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Drop,
            route[3],
            CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING),
            Mass::from_milligrams(700_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_COPPER),
        )
        .unwrap()],
    )
    .unwrap();
    let copper = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    let total = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 595);
    assert_eq!(state.surface().get(ids[0]).unwrap().position(), route[0]);
    assert_eq!(state.surface().get(ids[1]).unwrap().position(), route[0]);
    assert_eq!(
        state.surface().get(ids[2]).unwrap().position(),
        route[1],
        "when no stored parcel fits a whole copper drop, freeing more space enables a useful split"
    );
    assert_eq!(
        state.surface().get(copper).unwrap().remaining_mass(),
        Mass::from_milligrams(380_000),
        "the original oversized source keeps the exact mass not physically hauled",
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn industrial_salvage_eviction_splits_oversized_raw_den_cache() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut state, 579);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .mobilized_at
            .is_some()
    );
    let source = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        route[0],
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(1_200_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    let plan = validate_create_surface_resources(&registries, &state, [source]).unwrap();
    let stone = plan.first_resource_id();
    plan.commit(&mut state).unwrap();
    let copper = copper_winding(&registries, &mut state, route[3]);
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        advance_tick(&registries, &mut state).unwrap(),
        advance_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(
        state.surface().get(stone).unwrap().remaining_mass(),
        Mass::from_milligrams(950_000),
        "industrial salvage needs a split when raw den cargo exceeds the carry limit"
    );
    assert!(
        state
            .surface()
            .available_resources_at(Some(route[1]))
            .any(|item| {
                item.kind() == SurfaceResourceKind::Drop
                    && item.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
                    && item.remaining_mass() == Mass::from_milligrams(250_000)
            })
    );
    assert_eq!(state.surface().get(copper).unwrap().position(), route[3]);
    assert_eq!(state, replay);
    let midpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    replay = serde_json::from_slice::<LoadedSaveEnvelope>(&midpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    while state.tick().value() < 1_010 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(state.surface().get(stone).unwrap().position(), route[1]);
    assert_eq!(
        state.surface().get(copper).unwrap().position(),
        route[0],
        "the eventual empty den must admit the reachable precision component"
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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

#[test]
fn excavating_a_corner_exposes_industrial_salvage_to_the_resident_den() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::world::{
        BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
        validate_generate_chunk,
    };

    let registries = crate::content::build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(0, 0, 1);
    let side = VoxelCoord::new(0, 0, -1);
    let corner = VoxelCoord::new(-1, 0, -1);
    for (a, b) in [(den, junction), (junction, tip), (junction, side)] {
        add_traversable_edge_for_test(&mut state, a, b);
    }
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    let machine = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        crate::content::ENERGY_ELECTRICAL_BUFFER,
        Energy::ZERO,
    )
    .unwrap();
    place_energy_store_for_fixture(&mut state, machine, junction);
    advance_to(&registries, &mut state, 579);
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .mobilized_at
            .is_some()
    );
    for chunk in [
        ChunkCoord::containing(den),
        ChunkCoord::containing(junction),
        ChunkCoord::containing(side),
        ChunkCoord::containing(corner),
    ] {
        validate_generate_chunk(
            &registries,
            &state,
            WorldGenerationSeed::new(7),
            chunk,
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    }
    for position in [den, junction, tip, side, corner] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(position.x(), -1, position.z()),
            BLOCK_STATE_STONE,
        );
        for y in 0..=1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(position.x(), y, position.z()),
                BlockStateId::AIR,
            );
        }
    }
    let stone = cache_stone(&registries, &mut state, den);
    let copper = copper_winding(&registries, &mut state, side);
    let mut hidden = state.clone();
    set_generated_block_for_test(registries.world(), &mut hidden, corner, BLOCK_STATE_STONE);
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &state,
        den,
        side,
    ));
    assert!(!crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        den,
        side,
    ));
    assert!(crate::kobolds::visibility::scout_visible_from(
        &registries,
        &hidden,
        junction,
        side,
    ));
    let visible_total = calculate_matter_accounting(&state).unwrap().total();
    let hidden_total = calculate_matter_accounting(&hidden).unwrap().total();
    advance_to(&registries, &mut state, 580);
    advance_to(&registries, &mut hidden, 580);
    assert_eq!(state.surface().get(stone).unwrap().position(), junction);
    assert_eq!(hidden.surface().get(stone).unwrap().position(), den);
    assert_eq!(state.surface().get(copper).unwrap().position(), side);
    assert_eq!(hidden.surface().get(copper).unwrap().position(), side);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        visible_total
    );
    assert_eq!(
        calculate_matter_accounting(&hidden).unwrap().total(),
        hidden_total
    );
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));

    // Terrain changes before the den-care tick, not global source indexing,
    // explain why a resident chooses to free its local cache.
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &hidden)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for resident in [&mut hidden, &mut loaded] {
        set_generated_block_for_test(registries.world(), resident, corner, BlockStateId::AIR);
    }
    // The next standard meal deadline is still the only industry-clearance
    // occasion; merely exposing a hidden source does not grant an AI tick.
    advance_to(&registries, &mut hidden, 980);
    advance_to(&registries, &mut loaded, 980);
    assert_eq!(hidden, loaded);
    assert_eq!(hidden.surface().get(stone).unwrap().position(), junction);
    assert_eq!(
        calculate_matter_accounting(&hidden).unwrap().total(),
        visible_total
    );
    assert_eq!(validate_loaded_state(&registries, &hidden), Ok(()));
}

#[test]
fn den_cache_eviction_requires_scout_sight_but_far_salvage_is_found_on_patrol() {
    let (registries, mut packed, route) =
        scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut packed, 579);
    let (_, far) = add_salvage_routes(&mut packed, route);
    let stone = cache_stone(&registries, &mut packed, route[0]);

    let mut near = packed.clone();
    let nearby_copper = copper_winding(&registries, &mut near, route[3]);
    let mut distant = packed;
    let far_copper = copper_winding(&registries, &mut distant, far);
    assert_eq!(
        crate::kobolds::scavenging::reachable_inherited_salvage(
            &registries,
            &distant,
            super::super::scavenging::ScavengeRoute::new([route[0], route[1], route[2]], None),
            route[1],
            Mass::from_milligrams(500_000),
            distant.tick(),
        ),
        Some((route[3], Some(far))),
        "a scout at the junction can actually see the far copper"
    );
    assert_eq!(
        crate::kobolds::scavenging::reachable_inherited_salvage(
            &registries,
            &distant,
            super::super::scavenging::ScavengeRoute::new([route[0], route[1], route[2]], None),
            route[0],
            Mass::from_milligrams(500_000),
            distant.tick(),
        ),
        None,
        "the same material is outside the homebound scout's two-voxel view"
    );
    let near_matter = calculate_matter_accounting(&near).unwrap().total();
    let distant_matter = calculate_matter_accounting(&distant).unwrap().total();
    advance_to(&registries, &mut near, 580);
    advance_to(&registries, &mut distant, 580);
    assert_eq!(near.surface().get(stone).unwrap().position(), route[1]);
    assert_eq!(
        distant.surface().get(stone).unwrap().position(),
        route[0],
        "the homebound scout cannot evict custody for an unobserved distant cache"
    );
    assert_eq!(
        near.surface().get(nearby_copper).unwrap().position(),
        route[3]
    );
    assert_eq!(distant.surface().get(far_copper).unwrap().position(), far);
    assert_eq!(
        calculate_matter_accounting(&near).unwrap().total(),
        near_matter
    );
    assert_eq!(
        calculate_matter_accounting(&distant).unwrap().total(),
        distant_matter
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &distant)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..25 {
        assert_eq!(
            advance_tick(&registries, &mut distant).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(distant, replay);
    assert_eq!(distant.surface().get(stone).unwrap().position(), route[0]);
    assert_eq!(distant.surface().get(far_copper).unwrap().position(), far);
    assert_eq!(validate_loaded_state(&registries, &distant), Ok(()));

    // Without the packed den, a real scout can discover the same distant
    // copper when it walks out to the junction, and haul it home by each edge.
    let (_, mut open, _) = scenario(EnergyCarrier::Mechanical, Some(route_machinery()));
    advance_to(&registries, &mut open, 579);
    add_salvage_routes(&mut open, route);
    let recovered = copper_winding(&registries, &mut open, far);
    let conserved = calculate_matter_accounting(&open).unwrap().total();
    advance_to(&registries, &mut open, 605);
    assert_eq!(open.surface().get(recovered).unwrap().position(), route[0]);
    assert_eq!(
        calculate_matter_accounting(&open).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &open), Ok(()));
}

#[path = "watchman_tests.rs"]
mod watchman_tests;
