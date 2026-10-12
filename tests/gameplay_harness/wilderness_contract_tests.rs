//! Fresh-world contracts for the executable portion of the wilderness opening.
//!
//! Built-in fresh-world generation supplies finite same-voxel stone, timber, and forage before
//! actor admission. Hydrology is still absent, so only water remains a controlled local fixture.
//! Every acquisition, fabrication, assembly, storage, consumption, locality, survival, and time
//! consequence after admission uses its canonical runtime owner.

use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::seed_fluid_store;
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
use deep_hearth::crafting::{project_manual_craft_equipment, project_manual_craft_hand_work};
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{MaterialLotSelection, StockpileId};
use deep_hearth::labor::{PlayerWork, SurfaceGatheringMethodId};
use deep_hearth::logistics::{assess_player_carrying, validate_place_fluid_store};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::surface::{
    SurfaceGatheringRequest, SurfaceResourceId, validate_start_surface_gathering,
};
use deep_hearth::survival::{assess_survival, available_local_drink_sources, validate_eat};
use deep_hearth::world_generation::{
    WorldGenerationSeed, validate_generate_initial_wilderness_surface,
};

use super::exact_local_runtime::{STATIONARY_PLAYER_ORIGIN, assert_exact_local_runtime_ready};
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const WILDERNESS_WORLD_SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WildernessOpeningReview {
    pub(super) first_meal_ticks: u64,
    pub(super) camp_ticks: u64,
    pub(super) meal_mass_mg: u64,
    pub(super) retained_food_mass_mg: u64,
    pub(super) peak_carried_mass_mg: u64,
    pub(super) carried_capacity_mg: u64,
    pub(super) water_observed_ul: u64,
    pub(super) water_consumed_ul: u64,
    pub(super) remaining_stone_mg: u64,
    pub(super) remaining_timber_mg: u64,
    pub(super) field_box_build_ticks: u64,
    pub(super) field_box_capacity_mg: u64,
    pub(super) ambient_food_shelf_ticks: u64,
    pub(super) field_box_food_shelf_ticks: u64,
    pub(super) field_box_board_batches: u64,
    pub(super) adze_setup_ticks: u64,
    pub(super) bare_board_ticks: u64,
    pub(super) adze_board_ticks: u64,
    pub(super) adze_crossover_batches: u64,
    pub(super) final_warning_safe: bool,
}

fn project_wilderness_adze_decision(
    registries: &Registries,
    board_batches: u64,
) -> (u64, u64, u64, u64) {
    let one = NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one batch is nonzero"));
    let board_batches = NonZeroU64::new(board_batches)
        .unwrap_or_else(|| panic!("wilderness field-box board count must be nonzero"));
    let knapping = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("wilderness stone-tool knapping disappeared"));
    let handle = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
        .unwrap_or_else(|| panic!("wilderness handle shaping disappeared"));
    let stone_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .copied()
        .unwrap_or_else(|| panic!("wilderness stone gathering disappeared"));
    let timber_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .copied()
        .unwrap_or_else(|| panic!("wilderness timber gathering disappeared"));
    let stone_gather_ticks = stone_gathering
        .duration_for_mass(knapping.input_mass())
        .unwrap_or_else(|| panic!("wilderness stone gathering cannot schedule adze input"))
        .value();
    let timber_gather_ticks = timber_gathering
        .duration_for_mass(handle.input_mass())
        .unwrap_or_else(|| panic!("wilderness timber gathering cannot schedule adze input"))
        .value();
    let knap_ticks = project_manual_craft_hand_work(registries, PROCESS_KNAP_STONE_TOOL, one)
        .unwrap_or_else(|error| panic!("wilderness stone-tool projection failed: {error}"))
        .duration()
        .value();
    let handle_ticks = project_manual_craft_hand_work(registries, PROCESS_SHAPE_WOOD_HANDLE, one)
        .unwrap_or_else(|error| panic!("wilderness handle projection failed: {error}"))
        .duration()
        .value();
    let setup_ticks = stone_gather_ticks
        .checked_add(timber_gather_ticks)
        .and_then(|ticks| ticks.checked_add(knap_ticks))
        .and_then(|ticks| ticks.checked_add(handle_ticks))
        .unwrap_or_else(|| panic!("wilderness adze acquisition projection overflowed"));
    let bare_ticks =
        project_manual_craft_hand_work(registries, PROCESS_SHAPE_WOOD_BOARDS, board_batches)
            .unwrap_or_else(|error| panic!("wilderness bare board projection failed: {error}"))
            .duration()
            .value();
    let adze_ticks = project_manual_craft_equipment(
        registries,
        PROCESS_SHAPE_WOOD_BOARDS,
        board_batches,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("wilderness adze board projection failed: {error}"))
    .duration()
    .value();
    let crossover = (1_u64..=64)
        .find(|batches| {
            let batches = NonZeroU64::new(*batches)
                .unwrap_or_else(|| unreachable!("wilderness crossover candidates are nonzero"));
            let bare =
                project_manual_craft_hand_work(registries, PROCESS_SHAPE_WOOD_BOARDS, batches)
                    .unwrap_or_else(|error| {
                        panic!("wilderness crossover hand projection failed: {error}")
                    })
                    .duration()
                    .value();
            let assisted = project_manual_craft_equipment(
                registries,
                PROCESS_SHAPE_WOOD_BOARDS,
                batches,
                EQUIPMENT_STONE_WOODWORKING_ADZE,
                Condition::PRISTINE,
            )
            .unwrap_or_else(|error| panic!("wilderness crossover adze projection failed: {error}"))
            .duration()
            .value();
            setup_ticks
                .checked_add(assisted)
                .is_some_and(|with_setup| with_setup < bare)
        })
        .unwrap_or_else(|| panic!("wilderness adze never repays within 64 board batches"));
    (setup_ticks, bare_ticks, adze_ticks, crossover)
}

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

pub(super) fn evaluate_controlled_wilderness_opening() -> WildernessOpeningReview {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_initial_wilderness_surface(
        &registries,
        &state,
        WILDERNESS_WORLD_SEED,
        STATIONARY_PLAYER_ORIGIN,
    )
    .unwrap_or_else(|error| panic!("wilderness initial surface generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness initial surface generation commit failed: {error}"));
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

    // Ordinary admission owns a fixed body-scale carrying limit. The opening must adapt to that
    // capability instead of resizing the player to whatever the current script intends to gather.
    let forage_mass = Mass::from_milligrams(500_000);
    let meal_mass = Mass::from_milligrams(250_000);
    let board_craft = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("wilderness board shaping disappeared"));
    let board_commodity = CommodityKey::new(MATERIAL_WOOD, deep_hearth::content::FORM_BOARD);
    let board_output = board_craft
        .outputs()
        .iter()
        .find(|output| output.commodity() == board_commodity)
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("wilderness board shaping lost its board output"));
    let field_box_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("wilderness field-box joinery disappeared"));
    let board_batches = field_box_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let (adze_setup_ticks, bare_board_ticks, adze_board_ticks, adze_crossover_batches) =
        project_wilderness_adze_decision(&registries, board_batches);
    assert!(
        board_batches < adze_crossover_batches,
        "the first field-box board workload should remain below the full wilderness adze payback crossover"
    );
    assert!(
        adze_setup_ticks
            .checked_add(adze_board_ticks)
            .is_some_and(|with_setup| with_setup >= bare_board_ticks),
        "the first field-box board workload should rationally defer a newly acquired stone adze"
    );
    let field_box_raw_timber = Mass::from_milligrams(
        board_craft
            .input_mass()
            .milligrams()
            .checked_mul(board_batches)
            .unwrap_or_else(|| panic!("wilderness field-box timber requirement overflowed")),
    );
    let carried_food_after_meal = forage_mass
        .checked_sub(meal_mass)
        .unwrap_or_else(|| unreachable!("opening meal is smaller than gathered forage"));
    let carried = validate_admit_player(&registries, &state, STATIONARY_PLAYER_ORIGIN)
        .unwrap_or_else(|error| panic!("wilderness player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("wilderness player admission commit failed: {error}"))
        .carried_stockpile();
    assert_eq!(
        assess_player_carrying(&state).map(|assessment| assessment.capacity()),
        Some(registries.survival().physiology().carried_mass_capacity()),
        "controlled wilderness carrying must expose authored body capability"
    );
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

    // Food is an immediate camp problem, not a reward for finishing storage first. Forage and eat
    // one serving first; keep the second serving in carried custody until there is actually a field
    // box worth using.
    let berries = local_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        berries,
        carried,
        forage_mass,
        "early berry forage",
    );
    let carrying_after_forage = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("wilderness carrying assessment disappeared after forage"));
    assert_eq!(carrying_after_forage.stored(), forage_mass);
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
        "opening foraging should spend enough reserve for ordinary food to have a real physiological consequence"
    );
    let meal = validate_eat(
        &registries,
        &state,
        carried,
        &[MaterialLotSelection::new(meal_lot, meal_mass)],
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
        "foraged opening food must replenish some of the energy spent acquiring it"
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
        (3 * minute..=8 * minute).contains(&first_meal_microseconds),
        "a normal first serving should be acquired and eaten early instead of waiting behind fabrication or storage construction"
    );

    // Storage and tools are investments, not opening checkboxes. The remaining food already fits
    // comfortably on the player and remains fresh for four ambient days. Building a field box now
    // would consume two kilograms of timber and the rest of the quarter-hour opening merely to
    // extend one 250 g serving to seven days. Defer both box and adze until a real workload or
    // preservation horizon can repay them.
    let timber_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .copied()
        .unwrap_or_else(|| panic!("wilderness timber gathering disappeared"));
    let field_box_gather_ticks = timber_gathering
        .duration_for_mass(field_box_raw_timber)
        .unwrap_or_else(|| panic!("wilderness field-box timber gathering cannot schedule"))
        .value();
    let field_box_build_ticks = field_box_gather_ticks
        .checked_add(bare_board_ticks)
        .and_then(|ticks| ticks.checked_add(field_box_joinery.duration().value()))
        .unwrap_or_else(|| panic!("wilderness field-box build projection overflowed"));
    let field_box_storage = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("wilderness field-box storage definition disappeared"));
    let ambient_food_shelf_ticks = registries
        .survival()
        .get_food(berry_commodity)
        .unwrap_or_else(|| panic!("wilderness berry food definition disappeared"))
        .shelf_life()
        .value();
    let field_box_food_shelf_ticks = u64::try_from(
        u128::from(ambient_food_shelf_ticks)
            .checked_mul(u128::from(
                field_box_storage
                    .storage_profile()
                    .preservation_multiplier_ppm(),
            ))
            .unwrap_or_else(|| panic!("wilderness field-box shelf-life projection overflowed"))
            / 1_000_000,
    )
    .unwrap_or_else(|_| panic!("wilderness field-box shelf-life projection exceeds u64"));
    assert_eq!(
        ambient_food_shelf_ticks,
        registries.core().calendar().ticks_per_day() * 4,
        "opening berry storage policy assumes the authored four-day ambient shelf life"
    );
    assert_eq!(
        field_box_food_shelf_ticks,
        registries.core().calendar().ticks_per_day() * 7,
        "rough field box should extend opening berries from four to seven days"
    );
    let carrying_after_meal = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("wilderness carrying assessment disappeared after meal"));
    assert_eq!(carrying_after_meal.stored(), carried_food_after_meal);
    assert!(
        carrying_after_meal.available() > field_box_raw_timber,
        "field-box investment should be deferred only while the actor has ample body-custody headroom"
    );
    assert!(
        state.available_local_equipment().next().is_none(),
        "opening should not manufacture a tool before a workload can repay it"
    );
    assert!(
        state.available_local_ground_stockpiles().next().is_none(),
        "opening should not manufacture stationary storage before capacity or freshness requires it"
    );
    let remaining_stone_mg = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .map(|resource| resource.remaining_mass().milligrams())
        .unwrap_or_else(|| panic!("wilderness remaining stone opportunity disappeared"));
    let remaining_timber_mg = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .map(|resource| resource.remaining_mass().milligrams())
        .unwrap_or_else(|| panic!("wilderness remaining timber opportunity disappeared"));
    let surplus_berries = state
        .inventory()
        .lot_ids(carried)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == berry_commodity)
        })
        .unwrap_or_else(|| panic!("surplus opening berries disappeared after meal"));
    assert_eq!(
        state
            .inventory()
            .get_lot(surplus_berries)
            .map(|record| record.mass()),
        Some(carried_food_after_meal),
        "eating one serving should leave one carried serving without forcing premature storage"
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
        (3 * minute..=8 * minute).contains(&elapsed_microseconds),
        "controlled opening should end when immediate provisioning is stable instead of padding the first quarter hour with unneeded infrastructure"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final matter audit failed: {error}"))
            .total(),
        matter_before,
        "wilderness opening must conserve represented matter across gathering and eating"
    );
    assert_exact_local_runtime_ready(
        &registries,
        &state,
        "completed controlled wilderness opening",
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("wilderness final state invalid: {error}"));
    let final_survival = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared at final review"));
    let final_warning_safe = final_survival.metabolic_energy() > physiology.hungry_below()
        && final_survival.hydration() > physiology.thirsty_below();
    assert!(
        final_warning_safe,
        "the first camp should finish with retained provisions and the player still above hunger/thirst warnings"
    );

    WildernessOpeningReview {
        first_meal_ticks,
        camp_ticks: elapsed_ticks,
        meal_mass_mg: meal_mass.milligrams(),
        retained_food_mass_mg: carried_food_after_meal.milligrams(),
        peak_carried_mass_mg: carrying_after_forage.stored().milligrams(),
        carried_capacity_mg: carrying_after_forage.capacity().milligrams(),
        water_observed_ul: water_before.microliters(),
        water_consumed_ul: 0,
        remaining_stone_mg,
        remaining_timber_mg,
        field_box_build_ticks,
        field_box_capacity_mg: field_box_storage.maximum_stockpile_capacity().milligrams(),
        ambient_food_shelf_ticks,
        field_box_food_shelf_ticks,
        field_box_board_batches: board_batches,
        adze_setup_ticks,
        bare_board_ticks,
        adze_board_ticks,
        adze_crossover_batches,
        final_warning_safe,
    }
}

fn physical_minutes_tenths(registries: &Registries, ticks: u64) -> String {
    let microseconds = u128::from(ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness report physical duration overflowed"));
    let tenths = microseconds
        .checked_add(3_000_000)
        .unwrap_or_else(|| panic!("wilderness report duration rounding overflowed"))
        / 6_000_000;
    format!("{}.{:01}m", tenths / 10, tenths % 10)
}

pub(super) fn print_wilderness_experience(review: WildernessOpeningReview) {
    let registries = build_registries();
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    std::println!(
        "WILDERNESS EXPERIENCE evidence=ordinary-spawn-surface+controlled-water+exact-local-after-admission generation=[surface:ordinary water:controlled] timing=[first-meal:{}t/{} stable:{}t/{}] provisioning=[meal:{}mg retained:{}mg warning-safe:{}] carrying=[peak:{}mg capacity:{}mg utilization:{}ppm] water=[observed:{}uL consumed:{}uL decision:defer-after-hydrating-food] storage-choice=[selected:defer build:{}t capacity:{}mg food:{}mg ambient:{}d boxed:{}d reason:no-current-capacity-or-freshness-pressure] tool-choice=[selected:defer counterfactual-field-box=[board-batches:{} setup:{}t bare:{}t adze:{}t crossover:{}batches] reason:no-current-workload] remaining-opportunity=[stone:{}mg timber:{}mg]",
        review.first_meal_ticks,
        physical_minutes_tenths(&registries, review.first_meal_ticks),
        review.camp_ticks,
        physical_minutes_tenths(&registries, review.camp_ticks),
        review.meal_mass_mg,
        review.retained_food_mass_mg,
        review.final_warning_safe,
        review.peak_carried_mass_mg,
        review.carried_capacity_mg,
        u128::from(review.peak_carried_mass_mg) * 1_000_000
            / u128::from(review.carried_capacity_mg),
        review.water_observed_ul,
        review.water_consumed_ul,
        review.field_box_build_ticks,
        review.field_box_capacity_mg,
        review.retained_food_mass_mg,
        review.ambient_food_shelf_ticks / ticks_per_day,
        review.field_box_food_shelf_ticks / ticks_per_day,
        review.field_box_board_batches,
        review.adze_setup_ticks,
        review.bare_board_ticks,
        review.adze_board_ticks,
        review.adze_crossover_batches,
        review.remaining_stone_mg,
        review.remaining_timber_mg,
    );
}

#[test]
fn controlled_wilderness_opening_stabilizes_without_premature_investment() {
    let review = evaluate_controlled_wilderness_opening();
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some() {
        std::println!(
            "PROBE INPUT name=wilderness mode=explore samples=1 organic=0 world_root=0x{:016X} behavior_root=unused replay=generated-surface+controlled-water",
            WILDERNESS_WORLD_SEED.value(),
        );
        print_wilderness_experience(review);
    }
}
