//! Controlled-source contracts for the executable portion of the wilderness opening.
//!
//! Terrain/ecology generation is still absent, so the fixture creates only finite local source
//! matter before actor admission. Every acquisition, fabrication, assembly, storage, consumption,
//! locality, survival, and time consequence after admission uses its canonical runtime owner.

use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_surface_resource};
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, FLUID_WATER, FORM_FOOD, FORM_LOG, FORM_LUMP,
    MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER, SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
};
use deep_hearth::core::quantity::{Mass, Temperature, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::SimulationTick;
use deep_hearth::crafting::{
    ManualCraftStartRequest, plan_manual_craft_from_stockpile, resolve_manual_craft,
    validate_start_manual_craft,
};
use deep_hearth::equipment::{EquipmentDefinitionId, EquipmentId, validate_assemble_equipment};
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId, validate_build_storage_enclosure};
use deep_hearth::labor::{PlayerWork, SurfaceGatheringMethodId};
use deep_hearth::logistics::{
    validate_allocate_player_ground_stockpile, validate_drop_to_ground,
    validate_initialize_player_logistics, validate_place_fluid_store,
};
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::surface::{
    SurfaceGatheringRequest, SurfaceResourceId, validate_start_surface_gathering,
};
use deep_hearth::survival::{
    assess_survival, available_local_drink_sources, initialize_player_survival, validate_eat,
};

use super::exact_local_runtime::{STATIONARY_PLAYER_ORIGIN, assert_exact_local_runtime_ready};
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn advance_to(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
    allowance: TickEventAllowance<'_>,
    context: &str,
) {
    while state.tick() < completes_at {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("wilderness {context} tick failed: {error}"));
        assert_tick_events_within(&outcome, allowance, context);
    }
}

fn local_surface_resource(state: &AppState, commodity: CommodityKey) -> SurfaceResourceId {
    let matches = state
        .available_surface_resources()
        .filter(|resource| resource.commodity() == commodity)
        .map(|resource| resource.id())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "controlled wilderness opening requires exactly one visible local source for commodity {}",
        commodity.value()
    );
    matches[0]
}

fn local_equipment_by_definition(
    state: &AppState,
    definition: EquipmentDefinitionId,
    context: &str,
) -> EquipmentId {
    let matches = state
        .available_local_equipment()
        .filter(|record| record.definition() == definition)
        .map(|record| record.id())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "controlled wilderness opening requires exactly one locally observable {context}"
    );
    matches[0]
}

fn local_drinkable_store(
    registries: &Registries,
    state: &AppState,
) -> deep_hearth::fluid::FluidStoreId {
    let matches = available_local_drink_sources(registries, state)
        .map(|source| source.store())
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "controlled wilderness opening requires exactly one locally observable drinkable fluid store"
    );
    matches[0]
}

fn gather(
    registries: &Registries,
    state: &mut AppState,
    method: SurfaceGatheringMethodId,
    resource: SurfaceResourceId,
    destination: StockpileId,
    mass: Mass,
    context: &str,
) {
    validate_start_surface_gathering(
        registries,
        state,
        SurfaceGatheringRequest::new(method, resource, destination, mass),
    )
    .unwrap_or_else(|error| panic!("wilderness {context} gathering admission failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("wilderness {context} gathering commit failed: {error}"));
    let completes_at = match state.player_work().active() {
        Some(PlayerWork::SurfaceGathering { work }) => work.completes_at(),
        other => panic!("wilderness {context} gathering did not own player work: {other:?}"),
    };
    advance_to(
        registries,
        state,
        completes_at,
        TickEventAllowance {
            surface_resources: &[resource],
            ..TickEventAllowance::default()
        },
        context,
    );
    assert_eq!(state.player_work().active(), None);
}

fn craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: ProcessId,
    source: StockpileId,
    destination: StockpileId,
    batches: u64,
    equipment: Option<EquipmentId>,
    context: &str,
) {
    let batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("wilderness {context} batch count must be nonzero"));
    let request = plan_manual_craft_from_stockpile(registries, state, process, source, batches)
        .unwrap_or_else(|error| panic!("wilderness {context} input planning failed: {error}"));
    let request = match equipment {
        Some(equipment) => request.with_equipment(equipment),
        None => request,
    };
    let start = if source == destination {
        ManualCraftStartRequest::in_place(request)
    } else {
        ManualCraftStartRequest::new(request, destination)
    };
    let job = validate_start_manual_craft(registries, state, start)
        .unwrap_or_else(|error| panic!("wilderness {context} craft admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("wilderness {context} craft commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at())
        .unwrap_or_else(|| panic!("wilderness {context} craft job disappeared after admission"));
    advance_to(
        registries,
        state,
        completes_at,
        TickEventAllowance {
            production_jobs: &[job],
            ..TickEventAllowance::default()
        },
        context,
    );
    assert!(state.production().get_job(job).is_none());
    assert_eq!(state.player_work().active(), None);
}

fn assemble_tool(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
    source: StockpileId,
    context: &str,
) -> EquipmentId {
    let equipment = validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("wilderness {context} assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("wilderness {context} assembly commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.definition()),
        Some(definition)
    );
    assert_eq!(
        state.logistics().equipment_position(equipment),
        Some(STATIONARY_PLAYER_ORIGIN),
        "newly assembled wilderness tool must remain in the actor's local custody"
    );
    equipment
}

#[test]
fn controlled_wilderness_opening_builds_useful_tool_storage_and_provisions_before_copper() {
    let registries = build_registries();
    let mut state = AppState::new();
    for (commodity, mass, material) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(3_000_000),
            MATERIAL_STONE,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(8_000_000),
            MATERIAL_WOOD,
        ),
        (
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
            Mass::from_milligrams(500_000),
            MATERIAL_BERRIES,
        ),
    ] {
        seed_surface_resource(
            &registries,
            &mut state,
            STATIONARY_PLAYER_ORIGIN,
            commodity,
            mass,
            ROOM_TEMPERATURE,
            MaterialComposition::pure(material),
        );
    }
    {
        // The controlled fixture supplies only the still-missing hydrology/world-source edge. Its
        // store identity stays inside setup; ordinary play must rediscover the local finite water
        // through the actor-safe exact-voxel observation surface below.
        let fixture_water = seed_fluid_store(
            &registries,
            &mut state,
            Volume::from_microliters(2_000_000),
            FLUID_WATER,
            Volume::from_microliters(1_500_000),
            ROOM_TEMPERATURE,
        );
        validate_place_fluid_store(&state, fixture_water, STATIONARY_PLAYER_ORIGIN)
            .unwrap_or_else(|error| panic!("wilderness water placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wilderness water placement commit failed: {error}"));
    }

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("wilderness survival admission failed: {error}"));
    let carried = validate_initialize_player_logistics(
        &state,
        STATIONARY_PLAYER_ORIGIN,
        Mass::from_milligrams(10_000_000),
    )
    .unwrap_or_else(|error| panic!("wilderness carrying admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness carrying commit failed: {error}"))
    .carried_stockpile();
    assert_exact_local_runtime_ready(&registries, &state, "controlled wilderness opening");
    assert!(
        state.available_local_ground_stockpiles().next().is_none(),
        "the opening should not begin with an empty ground staging pile masquerading as player storage"
    );
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        1,
        "the controlled opening should expose its finite local water opportunity from the admitted player's first frame"
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial matter audit failed: {error}"))
        .total();
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial fluid audit failed: {error}"))
        .total();
    let started_at = state.tick();
    let minute = 60_u128 * 1_000_000;

    let stone = local_surface_resource(&state, CommodityKey::new(MATERIAL_STONE, FORM_LUMP));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        carried,
        Mass::from_milligrams(1_000_000),
        "loose stone for first tool",
    );
    let timber = local_surface_resource(&state, CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_TIMBER,
        timber,
        carried,
        Mass::from_milligrams(1_000_000),
        "fallen timber for first tool",
    );
    craft_batches(
        &registries,
        &mut state,
        PROCESS_KNAP_STONE_TOOL,
        carried,
        carried,
        1,
        None,
        "first stone tool head",
    );
    craft_batches(
        &registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        carried,
        carried,
        1,
        None,
        "first wood handle",
    );
    let _ = assemble_tool(
        &registries,
        &mut state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        carried,
        "stone adze",
    );
    let local_tool_definitions = state
        .available_local_equipment()
        .map(|record| record.definition())
        .collect::<Vec<_>>();
    assert_eq!(
        local_tool_definitions,
        vec![EQUIPMENT_STONE_WOODWORKING_ADZE],
        "the opening actor should build the useful woodworking tool it has an immediate job for rather than a checklist of unused specializations"
    );
    let adze = local_equipment_by_definition(
        &state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        "stone woodworking adze",
    );
    let adze_ready_ticks = state
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("wilderness first-tool time reversed"))
        .value();
    let adze_ready_microseconds = u128::from(adze_ready_ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness first-tool physical duration overflowed"));
    assert!(
        (5 * minute..=15 * minute).contains(&adze_ready_microseconds),
        "the first useful camp tool should require real acquisition/fabrication work without consuming the whole first quarter-hour"
    );

    // Food is an immediate camp problem, not a reward for finishing storage first. Forage and eat
    // one serving while the opening is still inside the first-quarter-hour target; keep the second
    // serving in carried custody until there is actually a field box worth using.
    let berries = local_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        berries,
        carried,
        Mass::from_milligrams(500_000),
        "early berry forage",
    );
    let berry_commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let meal_lot = state
        .inventory()
        .lot_ids(carried)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == berry_commodity)
        })
        .unwrap_or_else(|| panic!("foraged opening meal disappeared from carried custody"));
    let before_meal = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared before early meal"));
    let physiology = registries.survival().physiology();
    assert!(
        before_meal.metabolic_energy() > physiology.hungry_below()
            && before_meal.hydration() > physiology.thirsty_below(),
        "a rested wilderness opening should create provisioning pressure without forcing the player across hunger or thirst warnings"
    );
    assert!(
        before_meal.metabolic_energy() < physiology.maximum_metabolic_energy()
            && before_meal.hydration() < physiology.maximum_hydration(),
        "opening acquisition and tool work should spend enough reserve for ordinary forage to have a real physiological consequence"
    );
    let meal = validate_eat(
        &registries,
        &state,
        carried,
        &[MaterialLotSelection::new(
            meal_lot,
            Mass::from_milligrams(250_000),
        )],
    )
    .unwrap_or_else(|error| panic!("wilderness early berry meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness early berry meal commit failed: {error}"));
    advance_to(
        &registries,
        &mut state,
        meal.completes_at(),
        TickEventAllowance::default(),
        "early berry meal",
    );
    let after_meal = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared after early meal"));
    assert!(
        after_meal.metabolic_energy() > before_meal.metabolic_energy(),
        "foraged opening food must replenish some of the energy spent acquiring the first tool"
    );
    assert!(
        after_meal.hydration() > before_meal.hydration(),
        "water-rich forage should contribute its authored hydration instead of making a separate drink automatically mandatory"
    );
    let first_meal_ticks = state
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("wilderness first-meal time reversed"))
        .value();
    let first_meal_microseconds = u128::from(first_meal_ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness first-meal physical duration overflowed"));
    assert!(
        (8 * minute..=15 * minute).contains(&first_meal_microseconds),
        "a normal first serving should be acquired and eaten inside the first-quarter-hour opening rather than waiting for storage construction"
    );

    // Once the adze exists, gather only the timber needed for a project that actually repays it.
    // The remaining local stone and timber stay visible for later pick/shovel work when those jobs
    // become relevant instead of being consumed just to complete an opening checklist.
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_TIMBER,
        timber,
        carried,
        Mass::from_milligrams(4_000_000),
        "fallen timber project load",
    );
    assert_eq!(
        state
            .available_surface_resources()
            .map(|resource| (resource.commodity(), resource.remaining_mass()))
            .collect::<Vec<_>>(),
        vec![
            (
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                Mass::from_milligrams(2_000_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(3_000_000),
            ),
        ],
        "the opening should leave finite local stone/timber for later specialized tools instead of exhausting every source before provisioning"
    );

    // Four board batches are enough work for the authored stone adze to be a real investment rather
    // than a checklist item. The field box consumes only part of the output; useful prepared timber
    // remains for the next camp project instead of manufacturing exactly one recipe's ingredients.
    let board_batches = NonZeroU64::new(4).unwrap_or_else(|| unreachable!("four is nonzero"));
    let bare_board_request = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        carried,
        board_batches,
    )
    .unwrap_or_else(|error| panic!("wilderness board comparison planning failed: {error}"));
    let bare_board_work = resolve_manual_craft(&registries, &state, &bare_board_request)
        .unwrap_or_else(|error| panic!("wilderness bare board comparison failed: {error}"));
    let adze_board_work = resolve_manual_craft(
        &registries,
        &state,
        &bare_board_request.clone().with_equipment(adze),
    )
    .unwrap_or_else(|error| panic!("wilderness adze board comparison failed: {error}"));
    assert!(
        adze_board_work.duration() < bare_board_work.duration(),
        "the opening adze must immediately improve a real camp woodworking project"
    );
    craft_batches(
        &registries,
        &mut state,
        PROCESS_SHAPE_WOOD_BOARDS,
        carried,
        carried,
        4,
        Some(adze),
        "camp project boards",
    );
    let board_commodity = CommodityKey::new(MATERIAL_WOOD, deep_hearth::content::FORM_BOARD);
    let boards_before_box = state
        .inventory()
        .get_stockpile(carried)
        .map(|stockpile| stockpile.get_mass(board_commodity))
        .unwrap_or_else(|| panic!("wilderness carried inventory disappeared before field box"));
    let field_box_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("wilderness field-box joinery disappeared"));
    assert!(boards_before_box > field_box_joinery.input_mass());
    craft_batches(
        &registries,
        &mut state,
        PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
        carried,
        carried,
        1,
        None,
        "field-box body",
    );
    let boards_after_box = state
        .inventory()
        .get_stockpile(carried)
        .map(|stockpile| stockpile.get_mass(board_commodity))
        .unwrap_or_else(|| panic!("wilderness carried inventory disappeared after field box"));
    assert_eq!(
        boards_after_box,
        boards_before_box
            .checked_sub(field_box_joinery.input_mass())
            .unwrap_or_else(|| unreachable!("field-box input was checked above"))
    );
    assert!(
        !boards_after_box.is_zero(),
        "wilderness camp project should leave useful prepared timber after building storage"
    );
    let provisions =
        validate_allocate_player_ground_stockpile(&state, Mass::from_milligrams(10_000_000))
            .unwrap_or_else(|error| {
                panic!("wilderness provisions stockpile allocation failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness provisions stockpile commit failed: {error}")
            });
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        provisions,
        carried,
    )
    .unwrap_or_else(|error| panic!("wilderness field-box construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness field-box construction commit failed: {error}"));
    let built_provisions = provisions;
    let provisions = state
        .available_local_ground_stockpiles()
        .find(|record| {
            record
                .enclosure()
                .is_some_and(|enclosure| enclosure.definition() == STORAGE_ROUGH_TIMBER_FIELD_BOX)
        })
        .map(|record| record.id())
        .unwrap_or_else(|| panic!("wilderness actor could not rediscover its local field box"));
    assert_eq!(
        provisions, built_provisions,
        "local storage observation must rediscover the field box the actor just built"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(provisions)
            .and_then(|record| record.enclosure())
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_ROUGH_TIMBER_FIELD_BOX)
    );
    let surplus_berries = state
        .inventory()
        .lot_ids(carried)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == berry_commodity)
        })
        .unwrap_or_else(|| panic!("surplus opening berries disappeared before storage"));
    assert_eq!(
        state
            .inventory()
            .get_lot(surplus_berries)
            .map(|record| record.mass()),
        Some(Mass::from_milligrams(250_000)),
        "eating one serving should leave one carried serving for later storage"
    );
    validate_drop_to_ground(
        &registries,
        &state,
        provisions,
        &[MaterialLotSelection::new(
            surplus_berries,
            Mass::from_milligrams(250_000),
        )],
    )
    .unwrap_or_else(|error| panic!("wilderness surplus berry storage failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness surplus berry storage commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(provisions)
            .map(|record| record.get_mass(berry_commodity)),
        Some(Mass::from_milligrams(250_000)),
        "the rediscovered field box must preserve the surplus serving rather than forcing an immediate retrieval just to prove storage access"
    );

    let water = local_drinkable_store(&registries, &state);
    let water_before = state
        .fluid()
        .get_store(water)
        .map(|store| store.stored_volume())
        .unwrap_or_else(|| panic!("locally observed wilderness water disappeared"));
    let water_decision = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared before water decision"));
    let minimum_cup = physiology.direct_consumption().minimum_drink_volume();
    let proactive_drink_below = physiology
        .maximum_hydration()
        .checked_sub(minimum_cup)
        .unwrap_or_else(|| unreachable!("minimum drink fits inside maximum hydration"));
    assert!(
        water_decision.hydration() >= proactive_drink_below,
        "the early water-rich forage meal should leave the rested opening too hydrated to justify forcing another minimum cup solely to top off reserves"
    );
    assert!(
        water_decision.hydration() > physiology.thirsty_below(),
        "the controlled wilderness opening must remain comfortably above the authored thirst warning after its foraged meal"
    );
    assert_eq!(
        state
            .fluid()
            .get_store(water)
            .map(|store| store.stored_volume()),
        Some(water_before),
        "actor-safe water discovery must not consume the finite source when current reserves do not justify a drink"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final fluid audit failed: {error}"))
            .total(),
        fluid_before,
        "declining an unnecessary drink must leave represented fluid exactly conserved"
    );

    let elapsed_ticks = state
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("wilderness opening elapsed time reversed"))
        .value();
    let elapsed_microseconds = u128::from(elapsed_ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness physical duration overflowed"));
    assert!(
        (15 * minute..=35 * minute).contains(&elapsed_microseconds),
        "controlled acquisition/adze/storage/food/water opening should occupy a substantial first-session slice without forcing unused tool construction before missing fire, shelter, travel, or ordinary world-source discovery are counted"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final matter audit failed: {error}"))
            .total(),
        matter_before,
        "wilderness opening must conserve represented matter across gathering, crafting, equipment, storage, and eating"
    );
    assert_exact_local_runtime_ready(
        &registries,
        &state,
        "completed controlled wilderness opening",
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("wilderness final state invalid: {error}"));
}
