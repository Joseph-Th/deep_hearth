//! Ordinary first-metal acquisition through stone tools, surveying, and finite ore dressing.
//! This helper uses only real player work and the generated deposit. It grants no fixture matter.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK, FORM_CRUSHED, FORM_HANDLE,
    FORM_NATIVE_METAL, FORM_ORE, FORM_TOOL, MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
    PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER, PROSPECTING_DETAILED_FIELD_SURVEY,
    PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::geology::{
    FieldProspectingRequest, player_prospecting_options, validate_start_field_prospecting,
};
use deep_hearth::inventory::project_material_stacks;
use deep_hearth::logistics::validate_pickup_equipment;
use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target, validate_start_mining};
use deep_hearth::ore_processing::{
    resolve_manual_comminution_from_stack, resolve_manual_constituent_separation_from_stack,
    validate_start_manual_comminution, validate_start_manual_constituent_separation,
};

#[derive(Clone, Copy)]
pub(super) struct Route {
    pub(super) junction: VoxelCoord,
    pub(super) forage: VoxelCoord,
    pub(super) water: VoxelCoord,
    pub(super) outcrop: VoxelCoord,
    pub(super) stone: VoxelCoord,
    pub(super) timber: VoxelCoord,
    pub(super) carried: StockpileId,
}

#[derive(Clone, Copy)]
pub(super) struct Outcome {
    pub(super) ore_mined: Mass,
    pub(super) native_recovered: Mass,
    pub(super) survey_ticks: u64,
    pub(super) tooling_ticks: u64,
    pub(super) mining_ticks: u64,
    pub(super) dressing_ticks: u64,
    pub(super) hardness_upper_pa: u64,
}

fn survey(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    method: deep_hearth::labor::ProspectingMethodId,
    equipment: Option<deep_hearth::equipment::EquipmentId>,
) -> u64 {
    let methods = player_prospecting_options(registries, state, region).unwrap();
    assert!(methods.iter().any(|option| option.method() == method));
    let request = equipment.map_or_else(
        || FieldProspectingRequest::new(method, region, MATERIAL_COPPER),
        |id| FieldProspectingRequest::new_with_equipment(method, region, MATERIAL_COPPER, id),
    );
    let started = state.tick();
    let job = validate_start_field_prospecting(registries, state, request)
        .unwrap_or_else(|error| panic!("first copper prospecting rejected: {error}"));
    let ends = job.work().completes_at();
    job.commit(state).unwrap();
    advance_to(
        registries,
        state,
        ends,
        TickEventAllowance {
            field_prospecting: true,
            ..TickEventAllowance::default()
        },
        "stone-era copper survey",
    );
    state.tick().value() - started.value()
}

fn dress_ore(registries: &Registries, state: &mut AppState, carried: StockpileId) -> Mass {
    let beginning = state.tick();
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    let crushed = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let feed = Mass::from_milligrams(100_000);
    let stack = project_material_stacks(registries, state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == ore && stack.mass() >= feed)
        .expect("first copper must use its genuinely mined ore");
    let recipe = resolve_manual_comminution_from_stack(
        registries,
        state,
        PROCESS_HAND_BREAK_ORE,
        &stack,
        feed,
    )
    .unwrap();
    let job = validate_start_manual_comminution(registries, state, &recipe, carried, carried)
        .unwrap()
        .commit(state)
        .unwrap();
    let ends = state.production().get_job(job).unwrap().completes_at();
    advance_to(
        registries,
        state,
        ends,
        TickEventAllowance {
            production_jobs: &[job],
            ..TickEventAllowance::default()
        },
        "stone-era manual ore breaking",
    );
    let stack = project_material_stacks(registries, state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == crushed && stack.mass() >= feed)
        .expect("first copper must separate its physically crushed ore");
    let recipe = resolve_manual_constituent_separation_from_stack(
        registries,
        state,
        PROCESS_HAND_SORT_NATIVE_COPPER,
        &stack,
        feed,
    )
    .unwrap();
    let recovered = recipe.target_mass();
    let job = validate_start_manual_constituent_separation(
        registries, state, &recipe, carried, carried, carried,
    )
    .unwrap()
    .commit(state)
    .unwrap();
    let ends = state.production().get_job(job).unwrap().completes_at();
    advance_to(
        registries,
        state,
        ends,
        TickEventAllowance {
            production_jobs: &[job],
            ..TickEventAllowance::default()
        },
        "stone-era manual copper sorting",
    );
    assert!(state.tick().value() > beginning.value());
    recovered
}

/// Establish the stone era before pursuing copper, then produce the required native metal
/// from a fresh low-grade geological deposit. Returns to the real junction with physical stock.
pub(super) fn acquire(
    registries: &Registries,
    state: &mut AppState,
    route: Route,
    required: Mass,
) -> Outcome {
    assert_eq!(
        state.logistics().player().unwrap().position(),
        route.junction
    );
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let conserved = calculate_matter_accounting(state).unwrap().total();
    let acquired_before = state
        .inventory()
        .get_stockpile(route.carried)
        .unwrap()
        .get_mass(native);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let tools_started = state.tick();
    for (location, commodity, context) in [
        (route.stone, stone, "first copper stone heads"),
        (route.timber, timber, "first copper wooden handles"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, state, location, context);
        let resource = local_surface_resource(state, commodity);
        gather_in_batches(
            registries,
            state,
            resource,
            Mass::from_milligrams(2_000_000),
            context,
        );
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            route.junction,
            "return primitive tool materials",
        );
    }
    craft_in_place(
        registries,
        state,
        stone,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        2,
        "first copper stone tools",
    );
    craft_in_place(
        registries,
        state,
        timber,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        2,
        "first copper timber handles",
    );
    let assemble = |state: &mut AppState, definition| {
        let options = player_equipment_assembly_options(registries, state, route.carried).unwrap();
        let option = options
            .into_iter()
            .find(|candidate| candidate.definition() == definition)
            .expect("primitive tools must have a real assembly option");
        assert!(option.is_ready());
        let id = validate_assemble_player_equipment_option(registries, state, &option)
            .unwrap()
            .commit(state)
            .unwrap();
        validate_pickup_equipment(registries, state, id)
            .unwrap()
            .commit(state)
            .unwrap();
        id
    };
    let hammer = assemble(state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER);
    let pick = assemble(state, EQUIPMENT_STONE_PICK);
    let tooling_ticks = state.tick().value() - tools_started.value();

    for location in [route.forage, route.water, route.outcrop] {
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            location,
            "first copper outcrop expedition",
        );
    }
    assert!(
        state
            .available_surface_resources()
            .all(|resource| resource.commodity() != native),
        "the player cannot hand-collect any native copper beside the starting clay"
    );
    let region = VoxelBounds::new(
        route.outcrop,
        VoxelCoord::new(
            route.outcrop.x() + 1,
            route.outcrop.y() + 1,
            route.outcrop.z() + 1,
        ),
    )
    .unwrap();
    let mut survey_ticks = survey(
        registries,
        state,
        region,
        PROSPECTING_FIELD_INSPECTION,
        None,
    );
    assert!(
        resolve_mining_target(state, MiningTargetRequest::new(region, MATERIAL_COPPER)).is_err(),
        "a low-grade outcrop must not be exploitable on vague first impressions alone"
    );
    survey_ticks += survey(
        registries,
        state,
        region,
        PROSPECTING_DETAILED_FIELD_SURVEY,
        Some(hammer),
    );
    let hardness_upper_pa = state
        .geological_knowledge()
        .observations()
        .last()
        .unwrap()
        .excavation_hardness()
        .unwrap()
        .upper()
        .pascals();
    assert!(hardness_upper_pa <= 500_000_000);
    assert!(
        resolve_mining_target(state, MiningTargetRequest::new(region, MATERIAL_COPPER)).is_ok(),
        "precise field evidence must unlock the weathered ore"
    );

    // The authored 10%-grade seam yields 6.5 g recovered copper per 100 g of
    // hand-broken and sorted feed. One stone-pick mining batch yields at most 200 g.
    let dressing_batches = required.milligrams().div_ceil(6_500);
    let mining_batches = dressing_batches.div_ceil(2);
    let mining_started = state.tick();
    for _ in 0..mining_batches {
        let target =
            resolve_mining_target(state, MiningTargetRequest::new(region, MATERIAL_COPPER))
                .expect("acquired local copper evidence must remain valid after each mining batch");
        let job = validate_start_mining(
            registries,
            state,
            MINING_METHOD_HAND_PICK,
            target,
            route.carried,
            pick,
            Mass::from_milligrams(200_000),
        )
        .unwrap_or_else(|error| panic!("surveyed stone-pick copper mining rejected: {error}"))
        .commit(state)
        .unwrap();
        let ends = state.mining().get_job(job).unwrap().completes_at();
        advance_to(
            registries,
            state,
            ends,
            TickEventAllowance {
                mining_jobs: &[job],
                ..TickEventAllowance::default()
            },
            "stone-pick weathered copper extraction",
        );
    }
    let mining_ticks = state.tick().value() - mining_started.value();
    let dressing_started = state.tick();
    for batch in 0..dressing_batches {
        let recovered = dress_ore(registries, state, route.carried);
        assert_eq!(
            recovered,
            Mass::from_milligrams(6_500),
            "ore grade must not silently change"
        );
        if batch == 0 {
            assert!(
                state
                    .inventory()
                    .get_stockpile(route.carried)
                    .unwrap()
                    .get_mass(native)
                    < required,
                "first 100 g of weathered copper ore should not skip the primitive stage"
            );
        }
    }
    let dressing_ticks = state.tick().value() - dressing_started.value();
    for location in [route.water, route.forage, route.junction] {
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            location,
            "return recovered copper to the worksite",
        );
    }
    let native_recovered = state
        .inventory()
        .get_stockpile(route.carried)
        .unwrap()
        .get_mass(native)
        .checked_sub(acquired_before)
        .unwrap();
    assert!(native_recovered >= required);
    let ore_mined = Mass::from_milligrams(mining_batches * 200_000);
    assert_eq!(
        calculate_matter_accounting(state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(registries, state), Ok(()));
    Outcome {
        ore_mined,
        native_recovered,
        survey_ticks,
        tooling_ticks,
        mining_ticks,
        dressing_ticks,
        hardness_upper_pa,
    }
}
