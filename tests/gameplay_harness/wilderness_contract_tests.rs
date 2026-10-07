//! Controlled-source contracts for the executable portion of the wilderness opening.
//!
//! Terrain/ecology generation is still absent, so the fixture creates only finite local source
//! matter before actor admission. Every acquisition, fabrication, assembly, storage, consumption,
//! locality, survival, and time consequence after admission uses its canonical runtime owner.

use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_surface_resource};
use deep_hearth::content::{
    EQUIPMENT_STONE_DIGGING_SHOVEL, EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_WOODWORKING_ADZE,
    FLUID_WATER, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE, STORAGE_ROUGH_TIMBER_FIELD_BOX,
    SURFACE_GATHERING_HAND_COLLECT_STONE, SURFACE_GATHERING_HAND_COLLECT_TIMBER,
    SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
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
    validate_allocate_ground_stockpile, validate_initialize_player_logistics,
    validate_place_fluid_store,
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
    assess_survival, available_local_drink_sources, initialize_player_survival,
    validate_drink_store_to_full, validate_eat,
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
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, destination),
    )
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
fn controlled_wilderness_opening_executes_gather_tools_storage_food_and_water_before_copper() {
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
    let components = validate_allocate_ground_stockpile(
        &state,
        STATIONARY_PLAYER_ORIGIN,
        Mass::from_milligrams(12_000_000),
    )
    .unwrap_or_else(|error| panic!("wilderness component stockpile allocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness component stockpile commit failed: {error}"));
    assert_exact_local_runtime_ready(&registries, &state, "controlled wilderness opening");
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial matter audit failed: {error}"))
        .total();
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial fluid audit failed: {error}"))
        .total();
    let started_at = state.tick();

    let stone = local_surface_resource(&state, CommodityKey::new(MATERIAL_STONE, FORM_LUMP));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_STONE,
        stone,
        carried,
        Mass::from_milligrams(3_000_000),
        "loose stone",
    );
    let timber = local_surface_resource(&state, CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_TIMBER,
        timber,
        carried,
        Mass::from_milligrams(5_000_000),
        "fallen timber first load",
    );
    craft_batches(
        &registries,
        &mut state,
        PROCESS_KNAP_STONE_TOOL,
        carried,
        components,
        3,
        None,
        "stone tool heads",
    );
    craft_batches(
        &registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        carried,
        components,
        4,
        None,
        "wood handles",
    );
    let _pick = assemble_tool(
        &registries,
        &mut state,
        EQUIPMENT_STONE_PICK,
        components,
        "stone pick",
    );
    let adze = assemble_tool(
        &registries,
        &mut state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        components,
        "stone adze",
    );
    let _shovel = assemble_tool(
        &registries,
        &mut state,
        EQUIPMENT_STONE_DIGGING_SHOVEL,
        components,
        "stone shovel",
    );

    // The first timber load was enough to establish the toolkit without exceeding carried capacity.
    // Return to the still-visible local source for project timber only after the tools exist.
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_COLLECT_TIMBER,
        timber,
        carried,
        Mass::from_milligrams(3_000_000),
        "fallen timber project load",
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
        components,
        4,
        Some(adze),
        "camp project boards",
    );
    let board_commodity = CommodityKey::new(MATERIAL_WOOD, deep_hearth::content::FORM_BOARD);
    let boards_before_box = state
        .inventory()
        .get_stockpile(components)
        .map(|stockpile| stockpile.get_mass(board_commodity))
        .unwrap_or_else(|| panic!("wilderness component stockpile disappeared before field box"));
    let field_box_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("wilderness field-box joinery disappeared"));
    assert!(boards_before_box > field_box_joinery.input_mass());
    craft_batches(
        &registries,
        &mut state,
        PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
        components,
        components,
        1,
        None,
        "field-box body",
    );
    let boards_after_box = state
        .inventory()
        .get_stockpile(components)
        .map(|stockpile| stockpile.get_mass(board_commodity))
        .unwrap_or_else(|| panic!("wilderness component stockpile disappeared after field box"));
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
    let provisions = validate_allocate_ground_stockpile(
        &state,
        STATIONARY_PLAYER_ORIGIN,
        Mass::from_milligrams(10_000_000),
    )
    .unwrap_or_else(|error| panic!("wilderness provisions stockpile allocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness provisions stockpile commit failed: {error}"));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_ROUGH_TIMBER_FIELD_BOX,
        provisions,
        components,
    )
    .unwrap_or_else(|error| panic!("wilderness field-box construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness field-box construction commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(provisions)
            .and_then(|record| record.enclosure())
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_ROUGH_TIMBER_FIELD_BOX)
    );

    let berries = local_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        berries,
        provisions,
        Mass::from_milligrams(500_000),
        "berry forage",
    );
    assert!(
        state.available_surface_resources().next().is_none(),
        "controlled opening must deplete its disclosed local stone, timber, and forage opportunities"
    );
    let berry_commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berry_lot = state
        .inventory()
        .lot_ids(provisions)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == berry_commodity)
        })
        .unwrap_or_else(|| panic!("foraged berries disappeared from the field box"));
    let before_meal = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared before meal"));
    let meal = validate_eat(
        &registries,
        &state,
        provisions,
        &[MaterialLotSelection::new(
            berry_lot,
            Mass::from_milligrams(250_000),
        )],
    )
    .unwrap_or_else(|error| panic!("wilderness berry meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness berry meal commit failed: {error}"));
    advance_to(
        &registries,
        &mut state,
        meal.completes_at(),
        TickEventAllowance::default(),
        "berry meal",
    );
    let after_meal = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared after meal"));
    assert!(
        after_meal.metabolic_energy() > before_meal.metabolic_energy(),
        "foraged opening food must replenish some of the energy spent establishing camp"
    );

    let water = local_drinkable_store(&registries, &state);
    let water_before = state
        .fluid()
        .get_store(water)
        .map(|store| store.stored_volume())
        .unwrap_or_else(|| panic!("locally observed wilderness water disappeared"));
    let before_drink = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared before drinking"));
    assert!(
        before_drink.hydration() < registries.survival().physiology().maximum_hydration(),
        "establishing camp must create some ordinary hydration deficit before the water action"
    );
    let drink = validate_drink_store_to_full(&registries, &state, water)
        .unwrap_or_else(|error| panic!("wilderness local water validation failed: {error}"))
        .unwrap_or_else(|| panic!("wilderness camp work should require a drink"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("wilderness local water commit failed: {error}"));
    advance_to(
        &registries,
        &mut state,
        drink.completes_at(),
        TickEventAllowance::default(),
        "water drink",
    );
    let after_drink = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared after drinking"));
    assert!(
        after_drink.hydration() > before_drink.hydration(),
        "locally discovered wilderness water must replenish hydration spent establishing camp"
    );
    assert_eq!(
        state
            .fluid()
            .get_store(water)
            .map(|store| store.stored_volume()),
        Some(
            water_before
                .checked_sub(drink.volume())
                .unwrap_or_else(|| unreachable!("validated drink cannot exceed source volume"))
        ),
        "direct drinking must deplete the exact finite local water source"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final fluid audit failed: {error}"))
            .total(),
        fluid_before,
        "drinking must transfer finite water into terminal survival custody without losing volume"
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
    let minute = 60_u128 * 1_000_000;
    assert!(
        (25 * minute..=50 * minute).contains(&elapsed_microseconds),
        "controlled gather/tool/storage/food/water opening should occupy a substantial pre-copper session before missing fire, shelter, travel, or ordinary world-source discovery are counted"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final matter audit failed: {error}"))
            .total(),
        matter_before,
        "wilderness opening must conserve represented matter across gathering, crafting, equipment, storage, eating, and drinking"
    );
    assert_exact_local_runtime_ready(
        &registries,
        &state,
        "completed controlled wilderness opening",
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("wilderness final state invalid: {error}"));
}
