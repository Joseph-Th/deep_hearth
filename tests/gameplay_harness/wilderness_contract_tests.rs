//! Fresh-world contracts for the executable portion of the wilderness opening.
//!
//! Built-in fresh-world generation supplies a four-step traversable opening trail with forage,
//! finite water, stone, and timber distributed along it before actor admission. Immediate
//! observation, acquisition, movement, fabrication, storage, consumption, survival, and time
//! consequences after admission use canonical runtime owners. General terrain geometry,
//! pathfinding, and hydrology remain absent.

use std::num::NonZeroU64;

use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER, SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::SimulationTick;
use deep_hearth::crafting::{project_manual_craft_equipment, project_manual_craft_hand_work};
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::labor::{PlayerWork, SurfaceGatheringMethodId};
use deep_hearth::logistics::{PlayerStepError, assess_player_carrying, validate_start_player_step};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::{
    AdjacentSurfaceResource, SurfaceResourceId, available_adjacent_surface_resources,
    validate_start_player_surface_gathering,
};
use deep_hearth::survival::{
    AdjacentDrinkSource, assess_survival, available_adjacent_drink_sources,
    available_local_drink_sources,
};
use deep_hearth::world_generation::{WorldGenerationSeed, validate_generate_initial_wilderness};

use super::exact_local_runtime::{
    STATIONARY_PLAYER_ORIGIN, assert_ordinary_admitted_runtime_ready,
};
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const WILDERNESS_WORLD_SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WildernessOpeningReview {
    pub(super) food_secured_ticks: u64,
    pub(super) camp_ticks: u64,
    pub(super) carried_food_mass_mg: u64,
    pub(super) peak_carried_mass_mg: u64,
    pub(super) carried_capacity_mg: u64,
    pub(super) water_observed_ul: u64,
    pub(super) water_consumed_ul: u64,
    pub(super) total_steps: u64,
    pub(super) water_steps: u64,
    pub(super) available_step_choices: u64,
    pub(super) max_step_choices: u64,
    pub(super) blocked_adjacent_probes: u64,
    pub(super) stone_steps: u64,
    pub(super) timber_steps: u64,
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

fn adjacent_surface_resource(state: &AppState, commodity: CommodityKey) -> AdjacentSurfaceResource {
    let matches = available_adjacent_surface_resources(state)
        .filter(|resource| resource.commodity() == commodity)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "generated wilderness opening requires exactly one accessible adjacent source for commodity {}",
        commodity.value()
    );
    matches[0]
}

fn adjacent_drinkable_source(registries: &Registries, state: &AppState) -> AdjacentDrinkSource {
    let matches = available_adjacent_drink_sources(registries, state).collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "generated wilderness opening requires exactly one face-adjacent drinkable source"
    );
    matches[0]
}

fn step_to(registries: &Registries, state: &mut AppState, destination: VoxelCoord, context: &str) {
    let movement = validate_start_player_step(registries, state, destination)
        .unwrap_or_else(|error| panic!("wilderness {context} step validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("wilderness {context} step commit failed: {error}"));
    advance_to(
        registries,
        state,
        movement.completes_at(),
        TickEventAllowance {
            player_step: true,
            ..TickEventAllowance::default()
        },
        context,
    );
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(destination),
        "completed wilderness {context} step must move authoritative locality"
    );
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
    mass: Mass,
    context: &str,
) {
    validate_start_player_surface_gathering(registries, state, method, resource, mass)
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

pub(super) fn evaluate_generated_wilderness_opening() -> WildernessOpeningReview {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_initial_wilderness(
        &registries,
        &state,
        WILDERNESS_WORLD_SEED,
        STATIONARY_PLAYER_ORIGIN,
    )
    .unwrap_or_else(|error| panic!("wilderness initial generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wilderness initial generation commit failed: {error}"));

    // Ordinary admission owns a fixed body-scale carrying limit. The opening must adapt to that
    // capability instead of resizing the player to whatever the current script intends to gather.
    let forage_mass = Mass::from_milligrams(500_000);
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
    let carried = validate_admit_player(&registries, &state, STATIONARY_PLAYER_ORIGIN)
        .unwrap_or_else(|error| panic!("wilderness player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("wilderness player admission commit failed: {error}"))
        .carried_stockpile();
    assert_eq!(
        assess_player_carrying(&state).map(|assessment| assessment.capacity()),
        Some(registries.survival().physiology().carried_mass_capacity()),
        "generated wilderness carrying must expose authored body capability"
    );
    assert_ordinary_admitted_runtime_ready(&registries, &state, "generated wilderness opening");
    assert!(
        state.available_local_ground_stockpiles().next().is_none(),
        "the opening should not begin with an empty ground staging pile masquerading as player storage"
    );
    assert!(
        state.available_surface_resources().next().is_none(),
        "fresh spawn should begin empty rather than placing opening resources under the player"
    );
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        0,
        "generated trail water must not masquerade as exact-local before the player reaches it"
    );
    assert!(
        available_adjacent_drink_sources(&registries, &state)
            .next()
            .is_none(),
        "two-step opening water must not leak through immediate-neighbor observation at spawn"
    );
    let berry_commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let adjacent_berries = adjacent_surface_resource(&state, berry_commodity);
    let available_steps = state.available_player_steps().collect::<Vec<_>>();
    assert_eq!(
        available_steps,
        vec![adjacent_berries.position()],
        "fresh-world access should expose exactly the first generated trail edge"
    );
    let initial_step_choices = u64::try_from(available_steps.len())
        .unwrap_or_else(|_| panic!("wilderness available-step count exceeds u64"));
    let mut max_step_choices = initial_step_choices;
    let mut total_steps = 0_u64;
    let blocked_destination = [
        VoxelCoord::new(1, 0, 0),
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(0, 0, -1),
    ]
    .into_iter()
    .find(|candidate| *candidate != adjacent_berries.position())
    .unwrap_or_else(|| unreachable!("opening has more than one horizontal neighbor"));
    let before_blocked_step = state.clone();
    assert_eq!(
        validate_start_player_step(&registries, &state, blocked_destination).err(),
        Some(PlayerStepError::DestinationInaccessible {
            origin: STATIONARY_PLAYER_ORIGIN,
            destination: blocked_destination,
        }),
        "ordinary movement must fail closed when the generated world did not authorize an adjacent edge"
    );
    assert_eq!(state, before_blocked_step);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial matter audit failed: {error}"))
        .total();
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("wilderness initial fluid audit failed: {error}"))
        .total();
    let started_at = state.tick();
    let minute = 60_u128 * 1_000_000;

    // Food is an immediate camp provision, not a reward for finishing storage first. Secure one
    // useful reserve early. Reaching it must use actor-visible adjacent observation plus the
    // canonical generated trail instead of generation knowledge or a same-voxel bootstrap.
    step_to(
        &registries,
        &mut state,
        adjacent_berries.position(),
        "forage approach",
    );
    total_steps += 1;
    let berries = local_surface_resource(&state, berry_commodity);
    assert_eq!(
        berries,
        adjacent_berries.resource(),
        "exact-local forage observation must recover the adjacent resource the actor chose"
    );
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        berries,
        forage_mass,
        "early berry forage",
    );
    let carrying_after_forage = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("wilderness carrying assessment disappeared after forage"));
    assert_eq!(carrying_after_forage.stored(), forage_mass);
    let provisioning_decision = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared after early forage"));
    let physiology = registries.survival().physiology();
    assert!(
        provisioning_decision.metabolic_energy() > physiology.hungry_below()
            && provisioning_decision.hydration() > physiology.thirsty_below(),
        "a rested wilderness opening should secure food before ordinary work forces hunger or thirst warnings"
    );
    assert!(
        provisioning_decision.metabolic_energy() < physiology.maximum_metabolic_energy()
            && provisioning_decision.hydration() < physiology.maximum_hydration(),
        "opening foraging should spend some reserve without turning a negligible deficit into a forced top-off action"
    );
    let food_secured_ticks = state
        .tick()
        .checked_duration_since(started_at)
        .unwrap_or_else(|| panic!("wilderness food-provisioning time reversed"))
        .value();
    let food_secured_microseconds = u128::from(food_secured_ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness food-provisioning duration overflowed"));
    assert!(
        (3 * minute..=8 * minute).contains(&food_secured_microseconds),
        "a useful food reserve should be secured early instead of waiting behind fabrication or storage construction"
    );

    // Storage and tools are investments, not opening checkboxes. The food reserve already fits
    // comfortably on the player and remains fresh for four ambient days. Building a field box now
    // would consume two kilograms of timber and much of the short opening merely to extend a small
    // carried reserve to seven days. Defer both box and adze until a real workload or
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
    let carrying_after_provisioning = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("wilderness carrying assessment disappeared after provisioning"));
    assert_eq!(carrying_after_provisioning.stored(), forage_mass);
    assert!(
        carrying_after_provisioning.available() > field_box_raw_timber,
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
    let provisioned_berries = state
        .inventory()
        .lot_ids(carried)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == berry_commodity)
        })
        .unwrap_or_else(|| panic!("provisioned opening berries disappeared from carried custody"));
    assert_eq!(
        state
            .inventory()
            .get_lot(provisioned_berries)
            .map(|record| record.mass()),
        Some(forage_mass),
        "rested opening should retain the secured food reserve instead of consuming a redundant top-off meal"
    );

    let forage_step_choices = u64::try_from(state.available_player_steps().count())
        .unwrap_or_else(|_| panic!("wilderness forage-step choice count exceeds u64"));
    max_step_choices = max_step_choices.max(forage_step_choices);
    let adjacent_water = adjacent_drinkable_source(&registries, &state);
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none(),
        "stone/timber must remain undiscovered until the player reaches the water waypoint"
    );
    step_to(
        &registries,
        &mut state,
        adjacent_water.position(),
        "water approach",
    );
    total_steps += 1;
    let water_steps = total_steps;
    let water = local_drinkable_store(&registries, &state);
    assert_eq!(
        water,
        adjacent_water.store(),
        "local water observation after movement must recover the adjacent source the actor chose"
    );
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
        "the rested opening should remain too hydrated to justify forcing a minimum cup solely to top off reserves"
    );
    assert!(
        water_decision.hydration() > physiology.thirsty_below(),
        "the generated wilderness opening must remain comfortably above the authored thirst warning after foraging and its water step"
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

    // Continue a short reconnaissance along the generated trail. Each material site is discovered
    // only from the immediately previous authorized voxel; exact finite mass is read only after
    // arrival. This proves distributed ordinary opportunities without manufacturing collection
    // merely to satisfy coverage.
    let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let mut remaining_stone_mg = None;
    let mut remaining_timber_mg = None;
    let mut stone_steps = None;
    let mut timber_steps = None;
    for material_index in 0..2 {
        let choices = u64::try_from(state.available_player_steps().count())
            .unwrap_or_else(|_| panic!("wilderness material-step choice count exceeds u64"));
        max_step_choices = max_step_choices.max(choices);
        let adjacent_materials = available_adjacent_surface_resources(&state)
            .filter(|resource| {
                resource.commodity() == stone_commodity || resource.commodity() == timber_commodity
            })
            .collect::<Vec<_>>();
        assert_eq!(
            adjacent_materials.len(),
            1,
            "trail waypoint {material_index} should expose exactly one new material opportunity"
        );
        let target = adjacent_materials[0];
        step_to(
            &registries,
            &mut state,
            target.position(),
            "material reconnaissance",
        );
        total_steps += 1;
        let local = state
            .available_surface_resources()
            .filter(|resource| resource.commodity() == target.commodity())
            .collect::<Vec<_>>();
        assert_eq!(
            local.len(),
            1,
            "arriving at a discovered material site must expose exactly that local source"
        );
        assert_eq!(
            local[0].id(),
            target.resource(),
            "local material identity must match the adjacent source chosen before movement"
        );
        if target.commodity() == stone_commodity {
            remaining_stone_mg = Some(local[0].remaining_mass().milligrams());
            stone_steps = Some(total_steps);
        } else {
            remaining_timber_mg = Some(local[0].remaining_mass().milligrams());
            timber_steps = Some(total_steps);
        }
    }
    let remaining_stone_mg = remaining_stone_mg.unwrap_or_else(|| {
        panic!("wilderness trail never exposed the generated stone opportunity")
    });
    let remaining_timber_mg = remaining_timber_mg.unwrap_or_else(|| {
        panic!("wilderness trail never exposed the generated timber opportunity")
    });
    let stone_steps = stone_steps
        .unwrap_or_else(|| panic!("wilderness trail never recorded stone discovery distance"));
    let timber_steps = timber_steps
        .unwrap_or_else(|| panic!("wilderness trail never recorded timber discovery distance"));
    assert_eq!(
        total_steps, 4,
        "opening trail should require four lived movement steps"
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
        "generated opening should end when immediate provisioning is stable instead of padding the first quarter hour with unneeded infrastructure"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final matter audit failed: {error}"))
            .total(),
        matter_before,
        "wilderness opening must conserve represented matter across gathering and movement"
    );
    assert_ordinary_admitted_runtime_ready(
        &registries,
        &state,
        "completed generated wilderness opening",
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
        food_secured_ticks,
        camp_ticks: elapsed_ticks,
        carried_food_mass_mg: forage_mass.milligrams(),
        peak_carried_mass_mg: carrying_after_forage.stored().milligrams(),
        carried_capacity_mg: carrying_after_forage.capacity().milligrams(),
        water_observed_ul: water_before.microliters(),
        water_consumed_ul: 0,
        total_steps,
        water_steps,
        available_step_choices: initial_step_choices,
        max_step_choices,
        blocked_adjacent_probes: 1,
        stone_steps,
        timber_steps,
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
        "WILDERNESS EXPERIENCE evidence=generated+exact-local generation=[surface:ordinary water:ordinary] movement=[trail:{} forage:1 water:{} stone:{} timber:{} choices:{}-{} blocked:{}/1] timing=[food-secured:{}t/{} stable:{}t/{}] provisioning=[carried:{}mg meal:defer-rested warning-safe:{}] carrying=[peak:{}mg capacity:{}mg utilization:{}ppm] water=[observed:{}uL consumed:{}uL decision:defer-hydrated] storage-choice=[selected:defer build:{}t capacity:{}mg food:{}mg ambient:{}d boxed:{}d reason:no-capacity-or-freshness-pressure] tool-choice=[selected:defer counterfactual-field-box=[board-batches:{} setup:{}t bare:{}t adze:{}t crossover:{}batches] reason:no-current-workload] remaining-opportunity=[stone:{}mg timber:{}mg]",
        review.total_steps,
        review.water_steps,
        review.stone_steps,
        review.timber_steps,
        review.available_step_choices,
        review.max_step_choices,
        review.blocked_adjacent_probes,
        review.food_secured_ticks,
        physical_minutes_tenths(&registries, review.food_secured_ticks),
        review.camp_ticks,
        physical_minutes_tenths(&registries, review.camp_ticks),
        review.carried_food_mass_mg,
        review.final_warning_safe,
        review.peak_carried_mass_mg,
        review.carried_capacity_mg,
        u128::from(review.peak_carried_mass_mg) * 1_000_000
            / u128::from(review.carried_capacity_mg),
        review.water_observed_ul,
        review.water_consumed_ul,
        review.field_box_build_ticks,
        review.field_box_capacity_mg,
        review.carried_food_mass_mg,
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
fn generated_wilderness_opening_stabilizes_without_premature_investment() {
    let review = evaluate_generated_wilderness_opening();
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some() {
        std::println!(
            "PROBE INPUT name=wilderness mode=explore samples=1 organic=0 world_root=0x{:016X} behavior_root=unused replay=generated-opening",
            WILDERNESS_WORLD_SEED.value(),
        );
        print_wilderness_experience(review);
    }
}
