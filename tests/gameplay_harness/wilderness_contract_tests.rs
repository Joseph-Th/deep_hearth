//! Fresh-world contracts for the executable portion of the wilderness opening.
//!
//! Built-in fresh-world generation supplies a bounded traversable opening graph with forage,
//! optional finite water, and a later stone/timber choice distributed before actor admission. Immediate
//! observation, acquisition, movement, fabrication, storage, consumption, survival, and time
//! consequences after admission use canonical runtime owners. General terrain geometry,
//! pathfinding, and hydrology remain absent.

use std::{env, num::NonZeroU64};

use deep_hearth::content::gameplay_fixture::{
    admit_player_at_hunger_warning_boundary, admit_player_at_hydration_warning_boundary,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER, SURFACE_GATHERING_HAND_FORAGE_BERRIES, build_registries,
};
use deep_hearth::core::quantity::{Energy, Mass};
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
    available_local_drink_sources, project_survival_resource_budget,
    validate_drink_store_to_hydration_target, validate_eat_lot_to_metabolic_target,
};
use deep_hearth::world_generation::{WorldGenerationSeed, validate_generate_initial_wilderness};

use super::exact_local_runtime::{
    STATIONARY_PLAYER_ORIGIN, assert_ordinary_admitted_runtime_ready,
};
use super::seed;
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const WILDERNESS_WORLD_SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WildernessStartProfile {
    Rested,
    HungerPressure,
    HydrationPressure,
}

impl WildernessStartProfile {
    const fn label(self) -> &'static str {
        match self {
            Self::Rested => "rested",
            Self::HungerPressure => "hunger-pressure",
            Self::HydrationPressure => "hydration-pressure",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WildernessOpeningReview {
    pub(super) seed: u64,
    pub(super) start_profile: WildernessStartProfile,
    pub(super) food_secured_ticks: u64,
    pub(super) camp_ticks: u64,
    pub(super) forage_opportunity_mass_mg: u64,
    pub(super) forage_remaining_mass_mg: u64,
    pub(super) forage_rest_runway_ticks: u64,
    pub(super) gathered_food_mass_mg: u64,
    pub(super) carried_food_mass_mg: u64,
    pub(super) meal_consumed_mg: u64,
    pub(super) peak_carried_mass_mg: u64,
    pub(super) carried_capacity_mg: u64,
    pub(super) water_adjacent_known: bool,
    pub(super) water_visited: bool,
    pub(super) water_source_volume_ul: Option<u64>,
    pub(super) water_remaining_ul: Option<u64>,
    pub(super) water_rest_runway_ticks: Option<u64>,
    pub(super) water_consumed_ul: u64,
    pub(super) total_steps: u64,
    pub(super) available_step_choices: u64,
    pub(super) max_step_choices: u64,
    pub(super) blocked_adjacent_probes: u64,
    pub(super) material_choices: u64,
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

pub(super) fn evaluate_generated_wilderness_opening(
    seed: WorldGenerationSeed,
    start_profile: WildernessStartProfile,
) -> WildernessOpeningReview {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_initial_wilderness(&registries, &state, seed, STATIONARY_PLAYER_ORIGIN)
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
    let carried = match start_profile {
        WildernessStartProfile::Rested => {
            validate_admit_player(&registries, &state, STATIONARY_PLAYER_ORIGIN)
                .unwrap_or_else(|error| panic!("wilderness player admission failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("wilderness player admission commit failed: {error}")
                })
                .carried_stockpile()
        }
        WildernessStartProfile::HungerPressure => admit_player_at_hunger_warning_boundary(
            &registries,
            &mut state,
            STATIONARY_PLAYER_ORIGIN,
        )
        .carried_stockpile(),
        WildernessStartProfile::HydrationPressure => admit_player_at_hydration_warning_boundary(
            &registries,
            &mut state,
            STATIONARY_PLAYER_ORIGIN,
        )
        .carried_stockpile(),
    };
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
    let physiology = registries.survival().physiology();

    // Food is an immediate camp provision, not a reward for finishing storage first. Secure one
    // portable ration early rather than stripping the whole visible patch merely because it exists.
    // Reaching it must use actor-visible adjacent observation plus the canonical generated graph
    // instead of generation knowledge or a same-voxel bootstrap.
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
    let forage_opportunity_mass = state
        .available_surface_resources()
        .find(|resource| resource.id() == berries)
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("exact-local opening forage disappeared before gathering"));
    assert!(
        forage_opportunity_mass > forage_mass,
        "opening forage should be a finite opportunity larger than the actor's first portable ration"
    );
    let berry_food = registries
        .survival()
        .get_food(berry_commodity)
        .unwrap_or_else(|| panic!("wilderness berry food definition disappeared"));
    let forage_rest_runway_ticks_u128 = berry_food
        .dietary_energy_for_mass(forage_opportunity_mass)
        .nanojoules()
        / physiology.basal_energy_cost_per_tick().nanojoules();
    let forage_rest_runway_ticks = u64::try_from(forage_rest_runway_ticks_u128)
        .unwrap_or_else(|_| panic!("wilderness forage rest-runway exceeds u64 ticks"));
    assert!(
        forage_rest_runway_ticks > registries.core().calendar().ticks_per_day(),
        "the exact-local forage opportunity should buy more than one resting day before active-work costs"
    );
    gather(
        &registries,
        &mut state,
        SURFACE_GATHERING_HAND_FORAGE_BERRIES,
        berries,
        forage_mass,
        "early berry forage",
    );
    let forage_remaining_mass = state
        .available_surface_resources()
        .find(|resource| resource.id() == berries)
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("opening forage record disappeared after partial gathering"));
    assert_eq!(
        forage_opportunity_mass.checked_sub(forage_mass),
        Some(forage_remaining_mass),
        "quick opening forage should leave the unselected local opportunity in the world"
    );
    let carrying_after_forage = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("wilderness carrying assessment disappeared after forage"));
    assert_eq!(carrying_after_forage.stored(), forage_mass);
    let provisioning_decision = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared after early forage"));
    match start_profile {
        WildernessStartProfile::HungerPressure => assert!(
            provisioning_decision.metabolic_energy() <= physiology.hungry_below(),
            "the hunger-pressure witness should still need the gathered food after foraging"
        ),
        WildernessStartProfile::Rested | WildernessStartProfile::HydrationPressure => assert!(
            provisioning_decision.metabolic_energy() > physiology.hungry_below(),
            "non-hungry openings should preserve enough metabolic reserve to choose the next problem deliberately"
        ),
    }
    match start_profile {
        WildernessStartProfile::Rested | WildernessStartProfile::HungerPressure => assert!(
            provisioning_decision.hydration() > physiology.thirsty_below(),
            "a non-thirsty wilderness opening should remain above the thirst warning after early forage"
        ),
        WildernessStartProfile::HydrationPressure => assert!(
            provisioning_decision.hydration() <= physiology.thirsty_below(),
            "the hydration-pressure witness should still need the nearby water after foraging"
        ),
    }
    assert!(
        provisioning_decision.hydration() < physiology.maximum_hydration(),
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
    let mut meal_consumed_mg = 0_u64;
    if start_profile == WildernessStartProfile::HungerPressure {
        let carrying = assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("hunger-pressure opening lost carrying state before meal"));
        let locomotion = physiology.locomotion();
        let step_budget = project_survival_resource_budget(
            physiology,
            locomotion.exertion_for_load(carrying.stored(), carrying.capacity()),
            locomotion.step_duration(),
        )
        .unwrap_or_else(|error| panic!("hunger-pressure next-step projection failed: {error:?}"));
        let metabolic_target = physiology
            .hungry_below()
            .checked_add(step_budget.metabolic_energy())
            .and_then(|target| target.checked_add(Energy::from_nanojoules(1)))
            .unwrap_or_else(|| panic!("hunger-pressure opening metabolic target overflowed"));
        let meal = validate_eat_lot_to_metabolic_target(
            &registries,
            &state,
            provisioned_berries,
            metabolic_target,
        )
        .unwrap_or_else(|error| panic!("hunger-pressure opening meal projection failed: {error}"))
        .unwrap_or_else(|| panic!("hunger-pressure opening unexpectedly needed no meal"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("hunger-pressure opening meal commit failed: {error}"));
        meal_consumed_mg = meal.total_mass().milligrams();
        advance_to(
            &registries,
            &mut state,
            meal.completes_at(),
            TickEventAllowance::default(),
            "needed opening meal",
        );
        assert!(
            assess_survival(&registries, &state)
                .is_some_and(|survival| survival.metabolic_energy() >= metabolic_target),
            "needed opening meal should recover above the hunger warning"
        );
    }
    let retained_food_mass = Mass::from_milligrams(
        forage_mass
            .milligrams()
            .checked_sub(meal_consumed_mg)
            .unwrap_or_else(|| panic!("opening meal consumed more food than was gathered")),
    );

    // Storage and tools are investments, not opening checkboxes. The actor intentionally carries
    // only a small ration while the larger finite food opportunity remains in the world. That
    // selected ration fits comfortably on the player and remains fresh for four ambient days.
    // Building a field box now would consume two kilograms of timber merely to extend that small
    // carried reserve to seven days. Defer both box and adze until harvesting more food, a real
    // workload, capacity pressure, or a preservation horizon can repay them.
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
    let ambient_food_shelf_ticks = berry_food.shelf_life().value();
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
    assert_eq!(carrying_after_provisioning.stored(), retained_food_mass);
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
    assert_eq!(
        state
            .inventory()
            .get_lot(provisioned_berries)
            .map(|record| record.mass()),
        Some(retained_food_mass),
        "opening should consume food only when current hunger makes the meal useful"
    );

    let forage_position = adjacent_berries.position();
    let forage_steps = state.available_player_steps().collect::<Vec<_>>();
    let forage_step_choices = u64::try_from(forage_steps.len())
        .unwrap_or_else(|_| panic!("wilderness forage-step choice count exceeds u64"));
    max_step_choices = max_step_choices.max(forage_step_choices);
    let adjacent_water = adjacent_drinkable_source(&registries, &state);
    assert!(
        available_adjacent_surface_resources(&state)
            .next()
            .is_none(),
        "stone/timber should remain one decision deeper than the forage waypoint"
    );
    let junctions = forage_steps
        .iter()
        .copied()
        .filter(|position| {
            *position != STATIONARY_PLAYER_ORIGIN && *position != adjacent_water.position()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        junctions.len(),
        1,
        "forage should expose one work-path continuation beside the optional water branch"
    );
    let junction = junctions[0];
    let water_decision = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared before water decision"));
    let minimum_cup = physiology.direct_consumption().minimum_drink_volume();
    let proactive_drink_below = physiology
        .maximum_hydration()
        .checked_sub(minimum_cup)
        .unwrap_or_else(|| unreachable!("minimum drink fits inside maximum hydration"));
    let mut water_visited = false;
    let mut water_source_volume_ul = None;
    let mut water_remaining_ul = None;
    let mut water_rest_runway_ticks = None;
    let mut water_consumed_ul = 0_u64;
    match start_profile {
        WildernessStartProfile::Rested | WildernessStartProfile::HungerPressure => {
            assert!(
                water_decision.hydration() >= proactive_drink_below,
                "the rested opening should be able to skip optional water instead of walking there for a scripted checkpoint"
            );
        }
        WildernessStartProfile::HydrationPressure => {
            assert!(
                water_decision.hydration() <= physiology.thirsty_below(),
                "hydration-pressure opening should choose the visible water branch"
            );
            step_to(
                &registries,
                &mut state,
                adjacent_water.position(),
                "needed-water approach",
            );
            total_steps += 1;
            water_visited = true;
            let water = local_drinkable_store(&registries, &state);
            assert_eq!(water, adjacent_water.store());
            let water_before = state
                .fluid()
                .get_store(water)
                .map(|store| store.stored_volume())
                .unwrap_or_else(|| panic!("locally observed wilderness water disappeared"));
            water_source_volume_ul = Some(water_before.microliters());
            let rest_runway =
                water_before.microliters() / physiology.hydration_loss_per_tick().microliters();
            assert!(
                rest_runway > registries.core().calendar().ticks_per_day(),
                "the exact-local water opportunity should buy more than one resting day before active-work costs"
            );
            water_rest_runway_ticks = Some(rest_runway);
            let hydration_target = physiology
                .thirsty_below()
                .checked_add(minimum_cup)
                .unwrap_or_else(|| panic!("wilderness hydration target overflowed"));
            let drink = validate_drink_store_to_hydration_target(
                &registries,
                &state,
                water,
                hydration_target,
            )
            .unwrap_or_else(|error| panic!("wilderness needed drink projection failed: {error}"))
            .unwrap_or_else(|| panic!("hydration-pressure player unexpectedly needed no drink"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wilderness needed drink commit failed: {error}"));
            water_consumed_ul = drink.volume().microliters();
            advance_to(
                &registries,
                &mut state,
                drink.completes_at(),
                TickEventAllowance::default(),
                "needed-water drink",
            );
            let water_after = state
                .fluid()
                .get_store(water)
                .map(|store| store.stored_volume())
                .unwrap_or_else(|| panic!("wilderness water disappeared after drinking"));
            water_remaining_ul = Some(water_after.microliters());
            assert_eq!(
                water_before.checked_sub(drink.volume()),
                Some(water_after),
                "needed opening drink must withdraw exactly its admitted finite volume"
            );
            assert!(
                assess_survival(&registries, &state)
                    .is_some_and(|survival| survival.hydration() >= hydration_target),
                "needed opening drink should establish the chosen hydration buffer"
            );
            step_to(
                &registries,
                &mut state,
                forage_position,
                "return from optional water",
            );
            total_steps += 1;
        }
    }

    // The material junction is the first genuine world-choice moment in the default opening. The
    // actor can see both raw opportunities at once and need not visit either merely to satisfy a
    // harness checklist when there is no current workload worth paying for.
    step_to(&registries, &mut state, junction, "material junction");
    total_steps += 1;
    let junction_step_choices = u64::try_from(state.available_player_steps().count())
        .unwrap_or_else(|_| panic!("wilderness junction-step choice count exceeds u64"));
    max_step_choices = max_step_choices.max(junction_step_choices);
    let stone_commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let adjacent_materials = available_adjacent_surface_resources(&state)
        .filter(|resource| {
            resource.commodity() == stone_commodity || resource.commodity() == timber_commodity
        })
        .collect::<Vec<_>>();
    assert_eq!(
        adjacent_materials.len(),
        2,
        "material junction should expose stone and timber as simultaneous choices"
    );
    assert!(
        adjacent_materials
            .iter()
            .any(|resource| resource.commodity() == stone_commodity)
            && adjacent_materials
                .iter()
                .any(|resource| resource.commodity() == timber_commodity),
        "material junction must expose both primitive raw-material families"
    );
    let material_choices = u64::try_from(adjacent_materials.len())
        .unwrap_or_else(|_| panic!("wilderness material choice count exceeds u64"));
    let expected_steps = match start_profile {
        WildernessStartProfile::Rested | WildernessStartProfile::HungerPressure => 2,
        WildernessStartProfile::HydrationPressure => 4,
    };
    assert_eq!(
        total_steps, expected_steps,
        "opening movement should reflect whether water is a current need instead of a mandatory waypoint"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("wilderness final fluid audit failed: {error}"))
            .total(),
        fluid_before,
        "opening water decisions must conserve represented fluid across source and consumed custody"
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
        seed: seed.value(),
        start_profile,
        food_secured_ticks,
        camp_ticks: elapsed_ticks,
        forage_opportunity_mass_mg: forage_opportunity_mass.milligrams(),
        forage_remaining_mass_mg: forage_remaining_mass.milligrams(),
        forage_rest_runway_ticks,
        gathered_food_mass_mg: forage_mass.milligrams(),
        carried_food_mass_mg: retained_food_mass.milligrams(),
        meal_consumed_mg,
        peak_carried_mass_mg: carrying_after_forage.stored().milligrams(),
        carried_capacity_mg: carrying_after_forage.capacity().milligrams(),
        water_adjacent_known: true,
        water_visited,
        water_source_volume_ul,
        water_remaining_ul,
        water_rest_runway_ticks,
        water_consumed_ul,
        total_steps,
        available_step_choices: initial_step_choices,
        max_step_choices,
        blocked_adjacent_probes: 1,
        material_choices,
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

fn physical_hours_tenths(registries: &Registries, ticks: u64) -> String {
    let microseconds = u128::from(ticks)
        .checked_mul(u128::from(
            registries.core().physical_tick_duration().microseconds(),
        ))
        .unwrap_or_else(|| panic!("wilderness report physical duration overflowed"));
    let tenths = microseconds
        .checked_add(180_000_000)
        .unwrap_or_else(|| panic!("wilderness report duration rounding overflowed"))
        / 360_000_000;
    format!("{}.{:01}h", tenths / 10, tenths % 10)
}

pub(super) fn print_wilderness_experience(review: WildernessOpeningReview, sample: &str) {
    let registries = build_registries();
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let water_decision = if review.water_visited {
        "drink-needed"
    } else {
        "defer-hydrated"
    };
    let meal_decision = if review.meal_consumed_mg > 0 {
        "eat-needed"
    } else {
        "defer-fed"
    };
    let water_supply = match (
        review.water_source_volume_ul,
        review.water_remaining_ul,
        review.water_rest_runway_ticks,
    ) {
        (Some(source), Some(remaining), Some(runway)) => format!(
            "source:{source}uL remaining:{remaining}uL rest-runway:{}",
            physical_hours_tenths(&registries, runway),
        ),
        (None, None, None) => {
            "source:unvisited remaining:unvisited rest-runway:unvisited".to_owned()
        }
        _ => panic!("wilderness water telemetry must be wholly exact-local or wholly unvisited"),
    };
    std::println!(
        "WILDERNESS EXPERIENCE sample={} seed=0x{:016X} start={} evidence=generated+exact-local generation=[surface:ordinary water:ordinary] movement=[graph:5-edges steps:{} choices:{}-{} blocked:{}/1 water-branch:{} material-junction:reached material-choices:{}] timing=[food-secured:{}t/{} stable:{}t/{}] forage=[opportunity:{}mg secured:{}mg left:{}mg rest-runway:{}] provisioning=[retained:{}mg meal:{} consumed:{}mg warning-safe:{}] carrying=[peak:{}mg capacity:{}mg utilization:{}ppm] water=[observed:{} visited:{} {} consumed:{}uL decision:{}] storage-choice=[selected:defer build:{}t capacity:{}mg carried-food:{}mg ambient:{}d boxed:{}d local-surplus-left:{}mg reason:no-current-capacity-or-preservation-pressure] tool-choice=[selected:defer counterfactual-field-box=[board-batches:{} setup:{}t bare:{}t adze:{}t crossover:{}batches] reason:no-current-workload] material-choice=[stone+timber:simultaneous selected:defer reason:no-current-workload]",
        sample,
        review.seed,
        review.start_profile.label(),
        review.total_steps,
        review.available_step_choices,
        review.max_step_choices,
        review.blocked_adjacent_probes,
        if review.water_visited {
            "visited"
        } else {
            "skipped"
        },
        review.material_choices,
        review.food_secured_ticks,
        physical_minutes_tenths(&registries, review.food_secured_ticks),
        review.camp_ticks,
        physical_minutes_tenths(&registries, review.camp_ticks),
        review.forage_opportunity_mass_mg,
        review.gathered_food_mass_mg,
        review.forage_remaining_mass_mg,
        physical_hours_tenths(&registries, review.forage_rest_runway_ticks),
        review.carried_food_mass_mg,
        meal_decision,
        review.meal_consumed_mg,
        review.final_warning_safe,
        review.peak_carried_mass_mg,
        review.carried_capacity_mg,
        u128::from(review.peak_carried_mass_mg) * 1_000_000
            / u128::from(review.carried_capacity_mg),
        if review.water_adjacent_known {
            "adjacent"
        } else {
            "unknown"
        },
        review.water_visited,
        water_supply,
        review.water_consumed_ul,
        water_decision,
        review.field_box_build_ticks,
        review.field_box_capacity_mg,
        review.carried_food_mass_mg,
        review.ambient_food_shelf_ticks / ticks_per_day,
        review.field_box_food_shelf_ticks / ticks_per_day,
        review.forage_remaining_mass_mg,
        review.field_box_board_batches,
        review.adze_setup_ticks,
        review.bare_board_ticks,
        review.adze_board_ticks,
        review.adze_crossover_batches,
    );
}

fn configured_root(name: &str, fallback: u64) -> u64 {
    env::var(name).ok().map_or(fallback, |raw| {
        let trimmed = raw.trim();
        let parsed = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .map_or_else(
                || trimmed.parse().ok(),
                |hex| u64::from_str_radix(hex, 16).ok(),
            );
        parsed.unwrap_or_else(|| panic!("wilderness gameplay seed {name} is invalid: {raw:?}"))
    })
}

fn organic_start_profile(behavior_root: u64, index: usize) -> WildernessStartProfile {
    match (behavior_root as usize).wrapping_add(index) % 3 {
        0 => WildernessStartProfile::Rested,
        1 => WildernessStartProfile::HungerPressure,
        _ => WildernessStartProfile::HydrationPressure,
    }
}

pub(super) fn run_wilderness_report() {
    const ORGANIC_SAMPLES: usize = 4;
    let world_root = configured_root(
        "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED",
        seed::mix64(WILDERNESS_WORLD_SEED.value() ^ 0x5749_4C44_5F4F_5247),
    );
    let behavior_root = configured_root(
        "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED",
        seed::mix64(WILDERNESS_WORLD_SEED.value() ^ 0x5749_4C44_5F42_4856),
    );
    std::println!(
        "PROBE INPUT name=wilderness mode=explore samples={} maintained=1 coverage=1 organic={} world_root=0x{world_root:016X} behavior_root=0x{behavior_root:016X} replay=generated-opening-graph",
        ORGANIC_SAMPLES + 2,
        ORGANIC_SAMPLES,
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
        ),
        "maintained",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HydrationPressure,
        ),
        "coverage",
    );
    for index in 0..ORGANIC_SAMPLES {
        let ordinal = u64::try_from(index + 1)
            .unwrap_or_else(|_| unreachable!("bounded wilderness organic index fits u64"));
        let world_seed = seed::mix64(world_root ^ ordinal.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let profile = organic_start_profile(behavior_root, index);
        print_wilderness_experience(
            evaluate_generated_wilderness_opening(WorldGenerationSeed::new(world_seed), profile),
            "organic",
        );
    }
}

#[test]
fn generated_wilderness_opening_stabilizes_without_premature_investment() {
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some() {
        run_wilderness_report();
    } else {
        let _ = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
        );
        let hunger = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HungerPressure,
        );
        assert!(hunger.meal_consumed_mg > 0);
        assert!(hunger.carried_food_mass_mg < hunger.gathered_food_mass_mg);
        assert_eq!(hunger.total_steps, 2);
        assert!(!hunger.water_visited);
        assert!(hunger.final_warning_safe);
    }
}
