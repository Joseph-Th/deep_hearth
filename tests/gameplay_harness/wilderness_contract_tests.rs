//! Fresh-world contracts for the executable portion of the wilderness opening.
//!
//! Built-in fresh-world generation supplies a bounded traversable opening graph with forage,
//! optional finite water, and a later stone/timber choice distributed before actor admission. Immediate
//! observation, acquisition, movement, fabrication, storage, consumption, survival, and time
//! consequences after admission use canonical runtime owners. The direct-use witness additionally
//! composes deterministic generated terrain with 20 Hz continuous-body locomotion; general
//! pathfinding, route generation, and hydrology remain absent.

use std::{env, num::NonZeroU64};

use deep_hearth::content::gameplay_fixture::{
    admit_player_at_hunger_warning_boundary, admit_player_at_hydration_warning_boundary,
};
use deep_hearth::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, CAMPFIRE_CHIP_FUEL_MASS,
    CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY, CAMPFIRE_FUEL_MASS, CAMPFIRE_FUEL_THERMAL_ENERGY,
    ENERGY_STONE_RING_CAMPFIRE, EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_BOARD, FORM_CHIP, FORM_FOOD,
    FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_TOOL, MATERIAL_BERRIES, MATERIAL_STONE,
    MATERIAL_WILD_NUTS, MATERIAL_WOOD, PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_STONE_PROVISIONS_CROCK, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE, STORAGE_CARVED_STONE_PROVISIONS_CROCK,
    STORAGE_ROUGH_TIMBER_FIELD_BOX, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, SURFACE_GATHERING_HAND_COLLECT_STONE,
    SURFACE_GATHERING_HAND_COLLECT_TIMBER, build_registries,
};
use deep_hearth::core::quantity::{Area, Energy, Force, Length, Mass};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{SimulationTick, TickSpan};
use deep_hearth::crafting::{
    ManualCraftInputMode, player_manual_craft_options, project_manual_craft_equipment,
    project_manual_craft_hand_work, validate_start_player_manual_craft_option,
};
use deep_hearth::energy::{
    EnergyStoreId, assess_local_illumination, passive_dissipation_ticks_until_empty,
};
use deep_hearth::equipment::{
    EquipmentDefinitionId, player_equipment_assembly_options,
    validate_assemble_player_equipment_option,
};
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{
    MaterialStackProjection, StockpileId, StorageDefinitionId, project_material_stacks,
    validate_mount_stockpile, validate_place_player_storage,
};
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::{
    HorizontalControlInput, PlayerActiveSelection, PlayerControlInput,
    PlayerOverheadCoverAssessment, assess_player_carrying, assess_player_overhead_cover,
    project_player_walk_path, project_player_walk_path_for_load, validate_drop_stack_to_ground,
    validate_pickup_equipment, validate_pickup_stack_from_ground, validate_set_active_selection,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::{ProcessEquipmentRole, Registries};
use deep_hearth::simulation::{
    PlayerDirectControlInput, SecondaryActionOutcome, SecondaryControlIntent, advance_control_tick,
    advance_control_tick_with_direct_input, advance_control_tick_with_input, advance_tick,
};
use deep_hearth::spatial::{
    VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelBounds, VoxelCoord,
};
use deep_hearth::structural::{
    GroundedStructuralElementPlan, PrismaticMemberLoadRequest, StructuralElementGeometry,
    StructuralLifecycle, StructuralLoadKind, SupportedStructuralElementPlan,
    calculate_prismatic_material_mass_ceiling, project_prismatic_member_load,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_supported_structural_construction_from_stack,
};
use deep_hearth::surface::{
    AdjacentSurfaceResource, PlayerSurfaceGatheringOption, SurfaceResourceId,
    available_adjacent_surface_resources, player_surface_gathering_options,
    validate_start_player_surface_gathering_option,
};
use deep_hearth::survival::{
    AdjacentDrinkSource, FoodCategory, FoodStackPortion, SurvivalExertion, SurvivalResourceBudget,
    ThermalComfortState, assess_survival, available_adjacent_drink_sources,
    available_local_drink_sources, project_survival_resource_budget,
    validate_drink_store_to_hydration_target, validate_eat_stack_portions,
    validate_eat_stack_to_metabolic_target,
};
use deep_hearth::world::{
    BlockStateId, WorldGenerationSeed, validate_generate_player_neighborhood,
};
use deep_hearth::world_generation::validate_generate_initial_wilderness;

use super::exact_local_runtime::{
    assert_ordinary_admitted_runtime_ready, generated_wilderness_surface_spawn,
    walk_continuous_body_adjacent_and_settle,
};
use super::seed;
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const WILDERNESS_WORLD_SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5749_4C44_4552_4E53);
const SETTLED_ADJACENT_LEG_SLOW_TICKS: u64 = 1;

#[path = "wilderness_clay_progression.rs"]
mod clay_progression;
#[path = "wilderness_kobold_encounter.rs"]
mod kobold_encounter;
#[path = "wilderness_mobile_continuation.rs"]
mod mobile_continuation;

fn project_continuous_body_path_budget_for_load(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
    carried: Mass,
    context: &str,
) -> SurvivalResourceBudget {
    let capacity = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("wilderness {context} lost carrying assessment"))
        .capacity();
    project_player_walk_path_for_load(registries, state, destinations, carried, capacity)
        .unwrap_or_else(|error| panic!("wilderness {context} walk projection failed: {error}"))
        .resource_budget()
}

fn walk_body_to_adjacent(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
    context: &str,
) -> u64 {
    let origin = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("wilderness {context} lost admitted player"));
    assert!(
        origin.is_face_adjacent_to(destination),
        "wilderness {context} direct body route must be one face-adjacent voxel"
    );
    assert_eq!(origin.y(), destination.y());
    let x = i32::try_from(destination.x() - origin.x())
        .unwrap_or_else(|_| unreachable!("adjacent wilderness x delta fits i32"))
        * VIEW_DIRECTION_PARTS_PER_MILLION;
    let z = i32::try_from(destination.z() - origin.z())
        .unwrap_or_else(|_| unreachable!("adjacent wilderness z delta fits i32"))
        * VIEW_DIRECTION_PARTS_PER_MILLION;
    let horizontal = HorizontalControlInput::new(x, z)
        .unwrap_or_else(|error| panic!("wilderness {context} movement input failed: {error}"));
    let movement = PlayerControlInput::new(horizontal);
    let mut control_ticks = 0_u64;
    while state
        .logistics()
        .player()
        .is_some_and(|player| player.position() != destination)
    {
        assert!(
            control_ticks < 80,
            "wilderness {context} body movement did not reach destination"
        );
        let _ = advance_control_tick_with_input(registries, state, movement)
            .unwrap_or_else(|error| panic!("wilderness {context} movement failed: {error}"));
        control_ticks += 1;
    }
    while state.logistics().player().is_some_and(|player| {
        let velocity = player.body().horizontal_velocity();
        velocity.x_nanometers_per_second() != 0 || velocity.z_nanometers_per_second() != 0
    }) {
        assert!(
            control_ticks < 100,
            "wilderness {context} body movement did not settle"
        );
        let _ = advance_control_tick(registries, state)
            .unwrap_or_else(|error| panic!("wilderness {context} braking failed: {error}"));
        control_ticks += 1;
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(destination),
            "wilderness {context} braking overshot the intended local interaction voxel"
        );
    }
    control_ticks
}

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

fn assert_generated_wilderness_resources_are_directly_usable_with_empty_hands(emit_report: bool) {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, WILDERNESS_WORLD_SEED);
    validate_generate_initial_wilderness(&registries, &state, WILDERNESS_WORLD_SEED, spawn)
        .unwrap_or_else(|error| panic!("direct-water wilderness generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("direct-water wilderness generation commit failed: {error}")
        });
    let _ = admit_player_at_hydration_warning_boundary(&registries, &mut state, spawn);
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("direct-use wilderness terrain demand failed: {error}"))
    {
        demand.commit(&mut state).unwrap_or_else(|error| {
            panic!("direct-use wilderness terrain demand commit failed: {error}")
        });
    }
    assert_eq!(state.logistics().active_selection(), None);

    let forage = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    let forage_movement_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        forage.position(),
        "direct-use forage approach",
    );
    let forage_before = state
        .available_surface_resources()
        .find(|resource| resource.id() == forage.resource())
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("direct-use forage disappeared before gathering"));
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("direct-use view direction failed: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let forage_queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct-use forage queue failed: {error}"));
    assert_eq!(
        forage_queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary),
        "one empty-hand forage press should be retained until its legal work boundary"
    );
    let (forage_started_at, gathering) = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-use forage continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) => {
                break (control.control_tick(), *work);
            }
            None => {}
            other => panic!("direct-use forage returned unexpected outcome: {other:?}"),
        }
    };
    let forage_queue_ticks = forage_started_at
        .checked_steps_since(forage_queued.control_tick())
        .unwrap_or_else(|| panic!("direct-use forage start preceded queued input"));
    assert_eq!(
        forage_queued.queued_action_wait_control_ticks(),
        Some(forage_queue_ticks),
        "the input-tick wait projection must predict actual forage admission"
    );
    assert_eq!(gathering.resource(), forage.resource());
    assert!(!gathering.gathered_mass().is_zero());
    advance_to(
        &registries,
        &mut state,
        gathering.completes_at(),
        TickEventAllowance {
            surface_resources: &[forage.resource()],
            ..TickEventAllowance::default()
        },
        "direct-use forage gathering",
    );
    let forage_after = state
        .available_surface_resources()
        .find(|resource| resource.id() == forage.resource())
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("direct-use forage disappeared after gathering"));
    assert_eq!(
        forage_before.checked_sub(gathering.gathered_mass()),
        Some(forage_after),
        "direct-use forage gathering must consume exactly its admitted finite batch"
    );

    let water = adjacent_drinkable_source(&registries, &state);
    let forage_choices = state
        .available_adjacent_traversal_destinations()
        .collect::<Vec<_>>();
    let junctions = forage_choices
        .iter()
        .copied()
        .filter(|position| *position != spawn && *position != water.position())
        .collect::<Vec<_>>();
    assert_eq!(
        junctions.len(),
        1,
        "direct-use forage waypoint should expose exactly one work-path junction beside spawn and water"
    );
    let junction = junctions[0];
    let water_movement_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        water.position(),
        "direct-water source approach",
    );
    let local = available_local_drink_sources(&registries, &state).collect::<Vec<_>>();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].store(), water.store());
    let water_before = state
        .fluid()
        .get_store(water.store())
        .map(|store| store.stored_volume())
        .unwrap_or_else(|| panic!("direct-water source disappeared before use"));
    let hydration_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("direct-water actor disappeared before use"))
        .hydration();

    let water_queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct-water queue failed: {error}"));
    assert_eq!(
        water_queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary),
        "one empty-hand drink press should be retained until its legal work boundary"
    );
    let (water_started_at, started) = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-water continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::DrinkingStarted(outcome)) => {
                break (control.control_tick(), *outcome);
            }
            None => {}
            other => panic!("direct-water returned unexpected outcome: {other:?}"),
        }
    };
    let water_queue_ticks = water_started_at
        .checked_steps_since(water_queued.control_tick())
        .unwrap_or_else(|| panic!("direct-water start preceded queued input"));
    assert_eq!(
        water_queued.queued_action_wait_control_ticks(),
        Some(water_queue_ticks),
        "the input-tick wait projection must predict actual drinking admission"
    );
    assert_eq!(started.store(), water.store());
    assert!(!started.volume().is_zero());
    assert_eq!(
        state
            .fluid()
            .get_store(water.store())
            .map(|store| store.stored_volume()),
        water_before.checked_sub(started.volume()),
        "empty-hand direct use must withdraw exactly the admitted finite water volume"
    );
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Drinking { work }) if work.volume() == started.volume()
    ));
    advance_to(
        &registries,
        &mut state,
        started.completes_at(),
        TickEventAllowance::default(),
        "direct-water drinking",
    );
    let hydration_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("direct-water actor disappeared after use"))
        .hydration();
    assert!(
        hydration_after > hydration_before,
        "timed direct drinking should improve the actor's hydration"
    );
    let return_projection =
        project_player_walk_path(&registries, &state, &[forage.position(), junction])
            .unwrap_or_else(|error| {
                panic!("direct-use known return path projection failed: {error}")
            });
    let return_to_forage_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        forage.position(),
        "direct-use return from water",
    );
    let junction_movement_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        junction,
        "direct-use material junction",
    );
    assert_eq!(
        return_to_forage_ticks
            .checked_add(junction_movement_ticks)
            .unwrap_or_else(|| panic!("direct-use return path movement telemetry overflowed")),
        return_projection.control_ticks(),
        "continuous execution should match the pre-action projection for the already-observed two-edge return path"
    );
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
        "continuous-body opening should present stone and timber together at the material junction"
    );
    let timber = adjacent_materials
        .iter()
        .copied()
        .find(|resource| resource.commodity() == timber_commodity)
        .unwrap_or_else(|| panic!("direct-use material junction lost timber"));
    let stone = adjacent_materials
        .iter()
        .copied()
        .find(|resource| resource.commodity() == stone_commodity)
        .unwrap_or_else(|| panic!("direct-use material junction lost stone"));
    let timber_movement_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        timber.position(),
        "direct-use timber leaf",
    );
    assert_eq!(
        local_surface_resource(&state, timber_commodity),
        timber.resource(),
        "continuous-body timber visit must recover the actor-visible adjacent timber source"
    );
    let timber_haul_mass = Mass::from_milligrams(5_000_000);
    let timber_before = state
        .available_surface_resources()
        .find(|resource| resource.id() == timber.resource())
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("direct-use timber disappeared before haul gathering"));
    let timber_queued = advance_control_tick_with_direct_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("direct-use timber queue failed: {error}"));
    assert_eq!(
        timber_queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary),
        "one empty-hand timber press should survive until its legal gathering boundary"
    );
    let (timber_started_at, timber_direct) = loop {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-use timber continuation failed: {error}"));
        match control.secondary_action() {
            Some(SecondaryActionOutcome::SurfaceGatheringStarted(work)) => {
                break (control.control_tick(), *work);
            }
            None => {}
            other => panic!("direct-use timber returned unexpected outcome: {other:?}"),
        }
    };
    let timber_queue_ticks = timber_started_at
        .checked_steps_since(timber_queued.control_tick())
        .unwrap_or_else(|| panic!("direct-use timber start preceded queued input"));
    assert_eq!(
        timber_queued.queued_action_wait_control_ticks(),
        Some(timber_queue_ticks),
        "the input-tick wait projection must predict actual timber admission"
    );
    assert_eq!(timber_direct.resource(), timber.resource());
    advance_to(
        &registries,
        &mut state,
        timber_direct.completes_at(),
        TickEventAllowance {
            surface_resources: &[timber.resource()],
            ..TickEventAllowance::default()
        },
        "direct-use first timber batch",
    );
    let bulk_remainder = timber_haul_mass
        .checked_sub(timber_direct.gathered_mass())
        .unwrap_or_else(|| panic!("direct-use timber batch exceeded the selected haul"));
    if !bulk_remainder.is_zero() {
        gather(
            &registries,
            &mut state,
            timber.resource(),
            bulk_remainder,
            "direct-use timber haul remainder",
        );
    }
    let timber_after = state
        .available_surface_resources()
        .find(|resource| resource.id() == timber.resource())
        .map(|resource| resource.remaining_mass())
        .unwrap_or_else(|| panic!("direct-use timber disappeared after haul gathering"));
    assert_eq!(
        timber_before.checked_sub(timber_haul_mass),
        Some(timber_after),
        "continuous-body haul witness must remove exactly its selected timber parcel"
    );
    let hauling = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("direct-use timber haul lost carrying assessment"));
    assert_eq!(
        hauling.stored(),
        gathering
            .gathered_mass()
            .checked_add(timber_haul_mass)
            .unwrap_or_else(|| panic!("direct-use carried haul mass overflowed")),
        "timber haul must remain in finite carried custody alongside the selected forage ration"
    );
    let return_to_junction_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        junction,
        "direct-use loaded timber return",
    );
    assert_eq!(
        assess_player_carrying(&state).map(|assessment| assessment.stored()),
        Some(hauling.stored()),
        "continuous-body loaded return must keep the gathered timber in carried custody"
    );
    let stone_movement_ticks = walk_body_to_adjacent(
        &registries,
        &mut state,
        stone.position(),
        "direct-use stone leaf",
    );
    assert_eq!(
        local_surface_resource(&state, stone_commodity),
        stone.resource(),
        "continuous-body stone visit must recover the actor-visible adjacent stone source"
    );
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-use pre-jump settlement failed: {error}"));
    }
    let jump_survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("direct-use actor disappeared before jump"));
    let jump_input = PlayerControlInput::new(HorizontalControlInput::NEUTRAL).with_jump(true);
    let jump_control = advance_control_tick_with_input(&registries, &mut state, jump_input)
        .unwrap_or_else(|error| panic!("direct-use loaded jump failed: {error}"));
    let jump_motion = jump_control
        .body_motion()
        .unwrap_or_else(|| panic!("direct-use loaded jump produced no body motion"));
    let jump_vertical_speed = jump_motion
        .after()
        .vertical_velocity()
        .nanometers_per_second();
    assert!(
        jump_vertical_speed > 0,
        "direct-use jump must produce upward body velocity"
    );
    let mut jump_settlement_controls = 1_u64;
    let mut jump_settled = jump_control.simulation().is_some();
    while !jump_settled {
        let control = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("direct-use jump settlement failed: {error}"));
        jump_settlement_controls = jump_settlement_controls
            .checked_add(1)
            .unwrap_or_else(|| panic!("direct-use jump settlement duration overflowed"));
        jump_settled = control.simulation().is_some();
    }
    let jump_survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("direct-use actor disappeared after jump settlement"));
    let jump_energy_delta = jump_survival_before
        .metabolic_energy()
        .checked_sub(jump_survival_after.metabolic_energy())
        .unwrap_or_else(|| panic!("direct-use jump increased metabolic reserve"));
    let jump_hydration_delta = jump_survival_before
        .hydration()
        .checked_sub(jump_survival_after.hydration())
        .unwrap_or_else(|| panic!("direct-use jump increased hydration reserve"));
    let physiology = registries.survival().physiology();
    let expected_jump_exertion = physiology
        .locomotion()
        .jump_exertion_for_load(hauling.stored(), hauling.capacity());
    assert_eq!(
        jump_energy_delta,
        physiology
            .basal_energy_cost_per_tick()
            .checked_add(expected_jump_exertion.energy_cost_per_tick())
            .unwrap_or_else(|| panic!("direct-use jump energy expectation overflowed")),
        "loaded stationary jump must settle the exact authored launch cost above idle metabolism"
    );
    assert_eq!(
        jump_hydration_delta,
        physiology
            .hydration_loss_per_tick()
            .checked_add(expected_jump_exertion.hydration_loss_per_tick())
            .unwrap_or_else(|| panic!("direct-use jump hydration expectation overflowed")),
        "loaded stationary jump must settle the exact authored launch hydration cost above idle loss"
    );
    let route_control_ticks = forage_movement_ticks
        .checked_add(water_movement_ticks)
        .and_then(|ticks| ticks.checked_add(return_to_forage_ticks))
        .and_then(|ticks| ticks.checked_add(junction_movement_ticks))
        .and_then(|ticks| ticks.checked_add(timber_movement_ticks))
        .and_then(|ticks| ticks.checked_add(return_to_junction_ticks))
        .and_then(|ticks| ticks.checked_add(stone_movement_ticks))
        .unwrap_or_else(|| panic!("direct-use movement telemetry overflowed"));
    if emit_report {
        let water_after = state
            .fluid()
            .get_store(water.store())
            .map(|store| store.stored_volume())
            .unwrap_or_else(|| panic!("direct-water source disappeared after use"));
        std::println!(
            "WILDERNESS DIRECT USE seed=0x{:016X} movement=continuous-body route=[voxels:7 unique-edges:5/6 control-ticks:{}] known-path-projection=[edges:2 control-ticks:{} matched:true] material-choice=[stone+timber:2 visited:2] haul=[timber:{}mg carried:{}mg/{}mg return:{}ct] body-effort=[jump-load:{}mg vertical:{}nm/s energy:{}nJ>{}nJ-idle hydration:{}uL>{}uL-idle settle:{}ct] input=one-shot-empty-hand-secondary queue=[forage:{}ct drink:{}ct timber:{}ct] gather=[before:{}mg batch:{}mg remaining:{}mg] water=[before:{}uL serving:{}uL remaining:{}uL] hydration=[before:{}uL after:{}uL] timed=true",
            WILDERNESS_WORLD_SEED.value(),
            route_control_ticks,
            return_projection.control_ticks(),
            timber_haul_mass.milligrams(),
            hauling.stored().milligrams(),
            hauling.capacity().milligrams(),
            return_to_junction_ticks,
            hauling.stored().milligrams(),
            jump_vertical_speed,
            jump_energy_delta.nanojoules(),
            physiology.basal_energy_cost_per_tick().nanojoules(),
            jump_hydration_delta.microliters(),
            physiology.hydration_loss_per_tick().microliters(),
            jump_settlement_controls,
            forage_queue_ticks,
            water_queue_ticks,
            timber_queue_ticks,
            forage_before.milligrams(),
            gathering.gathered_mass().milligrams(),
            forage_after.milligrams(),
            water_before.microliters(),
            started.volume().microliters(),
            water_after.microliters(),
            hydration_before.microliters(),
            hydration_after.microliters(),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WildernessCampGoal {
    StayMobile,
    WorksiteCache,
    LongStayShelter,
    PreserveProvisions,
}

impl WildernessCampGoal {
    const fn label(self) -> &'static str {
        match self {
            Self::StayMobile => "stay-mobile",
            Self::WorksiteCache => "worksite-cache",
            Self::LongStayShelter => "long-stay-shelter",
            Self::PreserveProvisions => "preserve-provisions",
        }
    }

    const fn forage_mass(self) -> Mass {
        match self {
            Self::StayMobile => Mass::from_milligrams(500_000),
            Self::WorksiteCache | Self::LongStayShelter | Self::PreserveProvisions => {
                Mass::from_milligrams(1_000_000)
            }
        }
    }

    const fn uses_worksite_cache(self) -> bool {
        matches!(self, Self::WorksiteCache | Self::LongStayShelter)
    }

    const fn builds_shelter(self) -> bool {
        matches!(self, Self::LongStayShelter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WildernessShelterReview {
    pub(super) tool: EquipmentDefinitionId,
    pub(super) logistics_legs: u64,
    pub(super) stone_gathered_mg: u64,
    pub(super) staged_timber_used_mg: u64,
    pub(super) fresh_timber_gathered_mg: u64,
    pub(super) timber_source_remaining_mg: u64,
    pub(super) peak_carried_mass_mg: u64,
    pub(super) support_count: u64,
    pub(super) support_mass_mg: u64,
    pub(super) panel_mass_mg: u64,
    pub(super) board_batches: u64,
    pub(super) setup_ticks: u64,
    pub(super) bare_ticks: u64,
    pub(super) assisted_ticks: u64,
    pub(super) with_setup_ticks: u64,
    pub(super) saved_ticks: u64,
    pub(super) open_loss_ppm_per_tick: u32,
    pub(super) covered_loss_ppm_per_tick: u32,
    pub(super) observed_loss_ppm_per_tick: u32,
    pub(super) board_output_mg: u64,
    pub(super) tool_worn: bool,
    pub(super) postfire_deferred_refuel: bool,
    pub(super) postfire_covered_comfort_ppm: u32,
    pub(super) postfire_open_comfort_ppm: u32,
    pub(super) postfire_fuel_only_comfort_ppm: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WildernessOpeningReview {
    pub(super) seed: u64,
    pub(super) start_profile: WildernessStartProfile,
    pub(super) camp_goal: WildernessCampGoal,
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
    pub(super) total_legs: u64,
    pub(super) available_route_choices: u64,
    pub(super) max_route_choices: u64,
    pub(super) unadvertised_adjacent_probes: u64,
    pub(super) material_choices: u64,
    pub(super) selected_storage: Option<StorageDefinitionId>,
    pub(super) selected_material: Option<CommodityKey>,
    pub(super) storage_project_ticks: u64,
    pub(super) cached_food_mass_mg: u64,
    pub(super) cached_total_mass_mg: u64,
    pub(super) staged_worksite_timber_mg: u64,
    pub(super) worksite_construction_residue_mg: u64,
    pub(super) worksite_support_built: bool,
    pub(super) worksite_support_mass_mg: u64,
    pub(super) worksite_support_ticks: u64,
    pub(super) campfire_built: bool,
    pub(super) campfire_fuel_mg: u64,
    pub(super) campfire_logistics_legs: u64,
    pub(super) campfire_recovery_ticks: u64,
    pub(super) campfire_single_charge_runway_ticks: u64,
    pub(super) thermal_before_fire_ppm: u32,
    pub(super) thermal_after_fire_ppm: u32,
    pub(super) campfire_heat_remaining_nj: u128,
    /// Current fire energy at the end of the complete episode, not just initial recovery.
    pub(super) campfire_heat_at_end_nj: u128,
    pub(super) campfire_runway_at_end_ticks: u64,
    pub(super) campfire_runway_before_refuel_ticks: u64,
    pub(super) campfire_refuel_retrieval_ticks: u64,
    pub(super) campfire_refueled: bool,
    pub(super) campfire_recycled_chips_mg: u64,
    pub(super) campfire_one_hour_comfort_with_refuel_ppm: u32,
    pub(super) campfire_one_hour_comfort_without_refuel_ppm: u32,
    pub(super) campfire_light_level: u8,
    pub(super) fire_light_before_refuel: u8,
    pub(super) fire_light_after_refuel: u8,
    pub(super) final_thermal_comfort_ppm: u32,
    pub(super) selected_source_opportunity_mg: u64,
    pub(super) selected_source_remaining_mg: u64,
    pub(super) camp_logistics_legs: u64,
    pub(super) selected_storage_capacity_mg: u64,
    pub(super) selected_storage_preservation_ppm: u32,
    pub(super) project_metabolic_budget_nj: u128,
    pub(super) project_hydration_budget_ul: u64,
    pub(super) project_meal_consumed_mg: u64,
    pub(super) field_box_build_ticks: u64,
    pub(super) field_box_capacity_mg: u64,
    pub(super) ambient_food_shelf_ticks: u64,
    pub(super) field_box_food_shelf_ticks: u64,
    pub(super) stone_crock_build_ticks: u64,
    pub(super) stone_crock_capacity_mg: u64,
    pub(super) stone_crock_food_shelf_ticks: u64,
    pub(super) field_box_board_batches: u64,
    pub(super) adze_setup_ticks: u64,
    pub(super) bare_board_ticks: u64,
    pub(super) adze_board_ticks: u64,
    pub(super) adze_crossover_batches: u64,
    pub(super) shelter: Option<WildernessShelterReview>,
    pub(super) mobile_adaptation: Option<mobile_continuation::MobileColdResponse>,
    pub(super) final_warning_safe: bool,
}

fn worksite_support_projection(registries: &Registries) -> (Mass, TickSpan) {
    let projection = project_prismatic_member_load(
        registries,
        PrismaticMemberLoadRequest::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            Length::from_micrometers(500_000),
            Area::from_square_millimeters(5_000),
            Force::ZERO,
        ),
    )
    .unwrap_or_else(|error| panic!("wilderness worksite-support projection failed: {error}"));
    let definition = registries
        .labor()
        .get_structural_construction(STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER)
        .copied()
        .unwrap_or_else(|| panic!("wilderness worksite structural labor disappeared"));
    let duration = definition
        .duration_for_mass(projection.material_mass())
        .unwrap_or_else(|| panic!("wilderness worksite support exceeds hand-setting envelope"));
    (projection.material_mass(), duration)
}

fn worksite_support_geometry(junction: VoxelCoord) -> StructuralElementGeometry {
    let maximum = VoxelCoord::new(
        junction
            .x()
            .checked_add(1)
            .unwrap_or_else(|| panic!("wilderness support x-bound overflowed")),
        junction
            .y()
            .checked_add(1)
            .unwrap_or_else(|| panic!("wilderness support y-bound overflowed")),
        junction
            .z()
            .checked_add(1)
            .unwrap_or_else(|| panic!("wilderness support z-bound overflowed")),
    );
    let bounds = VoxelBounds::new(junction, maximum)
        .unwrap_or_else(|error| panic!("wilderness support bounds failed: {error}"));
    StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(500_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("wilderness worksite-support geometry failed: {error}"))
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

fn carried_material_stack(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
    context: &str,
) -> MaterialStackProjection {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("wilderness {context} lost carried custody"));
    let matches = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| panic!("wilderness {context} stack projection failed: {error}"))
        .into_iter()
        .filter(|stack| stack.commodity() == commodity)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "wilderness {context} requires one visible carried stack for commodity {}",
        commodity.value()
    );
    matches.into_iter().next().unwrap_or_else(|| unreachable!())
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
    resource: SurfaceResourceId,
    mass: Mass,
    context: &str,
) {
    let option = local_gathering_option(registries, state, resource, context);
    validate_start_player_surface_gathering_option(registries, state, option, mass)
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

fn local_gathering_option(
    registries: &Registries,
    state: &AppState,
    resource: SurfaceResourceId,
    context: &str,
) -> PlayerSurfaceGatheringOption {
    let matches = player_surface_gathering_options(registries, state)
        .unwrap_or_else(|error| panic!("wilderness {context} gathering catalog failed: {error}"))
        .into_iter()
        .filter(|option| option.resource() == resource)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "wilderness {context} requires one actor-visible gathering technique for resource {}",
        resource.value()
    );
    matches[0]
}

fn gather_in_batches(
    registries: &Registries,
    state: &mut AppState,
    resource: SurfaceResourceId,
    requested: Mass,
    context: &str,
) {
    let mut remaining = requested;
    while !remaining.is_zero() {
        let option = local_gathering_option(registries, state, resource, context);
        let batch = remaining.min(option.maximum_batch_mass());
        gather(registries, state, resource, batch, context);
        remaining = remaining.checked_sub(batch).unwrap_or_else(|| {
            unreachable!("bounded wilderness gathering batch cannot exceed remainder")
        });
    }
}

fn craft_in_place(
    registries: &Registries,
    state: &mut AppState,
    desired_input: CommodityKey,
    desired_output: CommodityKey,
    batches: u64,
    context: &str,
) {
    let batches = NonZeroU64::new(batches)
        .unwrap_or_else(|| panic!("wilderness {context} requires a nonzero craft batch count"));
    let options = player_manual_craft_options(registries, state)
        .unwrap_or_else(|error| panic!("wilderness {context} recipe catalog failed: {error}"));
    let matches = options
        .into_iter()
        .filter(|option| {
            let definition = registries
                .crafting()
                .get_manual(option.process())
                .unwrap_or_else(|| panic!("wilderness recipe catalog exposed an unknown process"));
            let produces_output = definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == desired_output);
            let (input, enough_input) = match option.input_mode() {
                ManualCraftInputMode::Automatic(availability) => (
                    availability.input(),
                    availability.maximum_batches() >= batches.get(),
                ),
                ManualCraftInputMode::ExplicitStackChoice { input, .. } => (input, false),
            };
            input == desired_input
                && produces_output
                && enough_input
                && option.equipment_role() != ProcessEquipmentRole::Required
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "wilderness {context} requires one actor-visible craft option from {}:{} to {}:{}",
        desired_input.material().value(),
        desired_input.form().value(),
        desired_output.material().value(),
        desired_output.form().value(),
    );
    let option = matches[0];
    let job = validate_start_player_manual_craft_option(registries, state, option, batches, None)
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
    assert_eq!(state.player_work().active(), None);
}

fn extend_worksite_with_shelter(
    registries: &Registries,
    state: &mut AppState,
    carried: StockpileId,
    storage: StockpileId,
    junction: VoxelCoord,
    campfire: EnergyStoreId,
) -> WildernessShelterReview {
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(junction),
        "long-stay shelter must extend the already established worksite state"
    );
    let stone_tool = CommodityKey::new(MATERIAL_STONE, FORM_TOOL);
    let wood_handle = CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let knapping = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("long-stay shelter stone-tool recipe disappeared"));
    let handle = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
        .unwrap_or_else(|| panic!("long-stay shelter handle recipe disappeared"));
    let board = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("long-stay shelter board recipe disappeared"));
    let panel_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        BASIC_TIMBER_COVER_PANEL_AREA,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
    )
    .unwrap_or_else(|error| panic!("long-stay shelter panel mass failed: {error}"));
    let board_output = board
        .outputs()
        .iter()
        .find(|output| output.commodity() == boards)
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("long-stay shelter board output disappeared"));
    let workload_batches = panel_mass.milligrams().div_ceil(board_output.milligrams());
    let batches = NonZeroU64::new(workload_batches)
        .unwrap_or_else(|| unreachable!("long-stay shelter requires nonzero board work"));
    let (setup_ticks, bare_ticks, assisted_ticks, crossover_batches) =
        project_wilderness_adze_decision(registries, workload_batches);
    let with_setup_ticks = setup_ticks
        .checked_add(assisted_ticks)
        .unwrap_or_else(|| panic!("long-stay shelter attention comparison overflowed"));
    assert!(
        workload_batches >= crossover_batches && with_setup_ticks < bare_ticks,
        "the real roof workload must repay primitive adze acquisition"
    );

    let support_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        BASIC_TIMBER_POST_CROSS_SECTION,
        BASIC_TIMBER_POST_LENGTH,
    )
    .unwrap_or_else(|error| panic!("long-stay shelter support mass failed: {error}"));
    let support_timber = Mass::from_milligrams(
        support_mass
            .milligrams()
            .checked_mul(3)
            .unwrap_or_else(|| panic!("long-stay shelter support timber overflowed")),
    );
    let timber_required = Mass::from_milligrams(
        handle
            .input_mass()
            .milligrams()
            .checked_add(
                board
                    .input_mass()
                    .milligrams()
                    .checked_mul(workload_batches)
                    .unwrap_or_else(|| panic!("long-stay shelter board timber overflowed")),
            )
            .and_then(|mass| mass.checked_add(support_timber.milligrams()))
            .unwrap_or_else(|| panic!("long-stay shelter timber requirement overflowed")),
    );
    let stone_required = knapping.input_mass();

    let stone_source = adjacent_surface_resource(state, stone);
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        stone_source.position(),
        "long-stay shelter stone pickup",
    );
    let stone_resource = local_surface_resource(state, stone);
    gather_in_batches(
        registries,
        state,
        stone_resource,
        stone_required,
        "long-stay shelter adze stone",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        junction,
        "long-stay shelter stone return",
    );

    let staged_stack = project_material_stacks(registries, state, storage)
        .unwrap_or_else(|error| {
            panic!("long-stay shelter staged timber projection failed: {error}")
        })
        .into_iter()
        .find(|stack| stack.commodity() == timber)
        .unwrap_or_else(|| panic!("long-stay shelter lost its staged timber reserve"));
    let staged_timber = staged_stack.mass().min(timber_required);
    assert!(
        staged_timber > Mass::ZERO,
        "long-stay shelter must consume the worksite reserve before gathering fresh timber"
    );
    validate_pickup_stack_from_ground(registries, state, storage, &staged_stack, staged_timber)
        .unwrap_or_else(|error| panic!("long-stay shelter staged timber pickup failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("long-stay shelter staged timber pickup commit failed: {error}")
        });

    let fresh_timber = timber_required
        .checked_sub(staged_timber)
        .unwrap_or_else(|| unreachable!("staged timber is capped by shelter timber demand"));
    let timber_source = adjacent_surface_resource(state, timber);
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        timber_source.position(),
        "long-stay shelter timber pickup",
    );
    let timber_resource = local_surface_resource(state, timber);
    gather_in_batches(
        registries,
        state,
        timber_resource,
        fresh_timber,
        "long-stay shelter fresh timber",
    );
    let timber_source_remaining_mg = state
        .available_surface_resources()
        .find(|record| record.id() == timber_resource)
        .map(|record| record.remaining_mass().milligrams())
        .unwrap_or_else(|| panic!("long-stay shelter timber source disappeared after gathering"));
    let peak_carried_mass_mg = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("long-stay shelter lost carrying assessment"))
        .stored()
        .milligrams();
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        junction,
        "long-stay shelter timber return",
    );

    craft_in_place(
        registries,
        state,
        stone,
        stone_tool,
        1,
        "long-stay shelter stone head",
    );
    craft_in_place(
        registries,
        state,
        timber,
        wood_handle,
        1,
        "long-stay shelter handle",
    );
    let board_options = player_manual_craft_options(registries, state)
        .unwrap_or_else(|error| panic!("long-stay shelter board catalog failed: {error}"))
        .into_iter()
        .filter(|option| {
            let definition = registries
                .crafting()
                .get_manual(option.process())
                .unwrap_or_else(|| panic!("long-stay shelter catalog exposed unknown process"));
            matches!(
                option.input_mode(),
                ManualCraftInputMode::Automatic(availability)
                    if availability.input() == timber
                        && availability.maximum_batches() >= batches.get()
            ) && definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == boards)
                && option.equipment_role() != ProcessEquipmentRole::Required
        })
        .collect::<Vec<_>>();
    assert_eq!(
        board_options.len(),
        1,
        "long-stay shelter requires one actor-visible optional-tool board recipe"
    );
    let board_option = board_options[0];
    let candidates = player_equipment_assembly_options(registries, state, carried)
        .unwrap_or_else(|error| panic!("long-stay shelter equipment catalog failed: {error}"))
        .into_iter()
        .filter(|option| option.is_ready())
        .filter_map(|option| {
            project_manual_craft_equipment(
                registries,
                board_option.process(),
                batches,
                option.definition(),
                Condition::PRISTINE,
            )
            .ok()
            .map(|projection| (option, projection))
        })
        .collect::<Vec<_>>();
    let best_ticks = candidates
        .iter()
        .map(|(_, projection)| projection.duration().value())
        .min()
        .unwrap_or_else(|| panic!("long-stay shelter catalog exposed no ready board tool"));
    let mut best = candidates
        .into_iter()
        .filter(|(_, projection)| projection.duration().value() == best_ticks)
        .collect::<Vec<_>>();
    assert_eq!(
        best.len(),
        1,
        "long-stay shelter workload must have one best ready tool"
    );
    let (assembly_option, projection) = best
        .pop()
        .unwrap_or_else(|| unreachable!("unique long-stay shelter tool exists"));
    assert_eq!(
        assembly_option.definition(),
        EQUIPMENT_STONE_WOODWORKING_ADZE
    );
    assert_eq!(projection.duration().value(), assisted_ticks);
    let tool = validate_assemble_player_equipment_option(registries, state, &assembly_option)
        .unwrap_or_else(|error| panic!("long-stay shelter adze assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("long-stay shelter adze assembly commit failed: {error}"));
    validate_pickup_equipment(registries, state, tool)
        .unwrap_or_else(|error| panic!("long-stay shelter adze pickup failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("long-stay shelter adze pickup commit failed: {error}"));
    validate_set_active_selection(
        registries,
        state,
        Some(PlayerActiveSelection::equipment(tool)),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter adze selection failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("long-stay shelter adze selection commit failed: {error}"));

    let started_at = state.tick();
    let job = validate_start_player_manual_craft_option(
        registries,
        state,
        board_option,
        batches,
        Some(tool),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter board start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("long-stay shelter board commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at())
        .unwrap_or_else(|| panic!("long-stay shelter board job disappeared"));
    assert_eq!(
        completes_at.value() - started_at.value(),
        assisted_ticks,
        "long-stay shelter board work must match the actor-visible adze projection"
    );
    advance_to(
        registries,
        state,
        completes_at,
        TickEventAllowance {
            production_jobs: &[job],
            ..TickEventAllowance::default()
        },
        "long-stay shelter boards",
    );

    let expected_board_mass = Mass::from_milligrams(
        board_output
            .milligrams()
            .checked_mul(workload_batches)
            .unwrap_or_else(|| panic!("long-stay shelter board output overflowed")),
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(boards)),
        Some(expected_board_mass)
    );
    let tool_worn = state
        .equipment()
        .get_equipment(tool)
        .is_some_and(|record| record.condition() < Condition::PRISTINE);
    assert!(
        tool_worn,
        "long-stay shelter board work should wear the earned adze"
    );

    let shelter_base = VoxelCoord::new(
        junction
            .x()
            .checked_add(1)
            .unwrap_or_else(|| panic!("long-stay shelter support x overflowed")),
        junction.y(),
        junction.z(),
    );
    let shelter_middle = VoxelCoord::new(
        shelter_base.x(),
        shelter_base
            .y()
            .checked_add(1)
            .unwrap_or_else(|| panic!("long-stay shelter middle y overflowed")),
        shelter_base.z(),
    );
    let shelter_upper = VoxelCoord::new(
        shelter_base.x(),
        shelter_base
            .y()
            .checked_add(2)
            .unwrap_or_else(|| panic!("long-stay shelter upper y overflowed")),
        shelter_base.z(),
    );
    let panel_position = VoxelCoord::new(
        junction.x(),
        junction
            .y()
            .checked_add(2)
            .unwrap_or_else(|| panic!("long-stay shelter panel y overflowed")),
        junction.z(),
    );
    let support_ground = VoxelCoord::new(
        shelter_base.x(),
        shelter_base
            .y()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("long-stay shelter ground y underflowed")),
        shelter_base.z(),
    );
    assert!(
        state
            .world()
            .block_at(support_ground)
            .is_some_and(|block| block != BlockStateId::AIR),
        "long-stay shelter support must begin on generated solid terrain"
    );
    for position in [shelter_base, shelter_middle, shelter_upper, panel_position] {
        assert_eq!(
            state.world().block_at(position),
            Some(BlockStateId::AIR),
            "long-stay shelter must occupy generated air"
        );
    }
    let support_geometry = |position: VoxelCoord| {
        let maximum = VoxelCoord::new(
            position
                .x()
                .checked_add(1)
                .unwrap_or_else(|| panic!("long-stay shelter support x bound overflowed")),
            position
                .y()
                .checked_add(1)
                .unwrap_or_else(|| panic!("long-stay shelter support y bound overflowed")),
            position
                .z()
                .checked_add(1)
                .unwrap_or_else(|| panic!("long-stay shelter support z bound overflowed")),
        );
        StructuralElementGeometry::new(
            VoxelBounds::new(position, maximum)
                .unwrap_or_else(|error| panic!("long-stay shelter support bounds failed: {error}")),
            BASIC_TIMBER_POST_LENGTH,
            BASIC_TIMBER_POST_CROSS_SECTION,
        )
        .unwrap_or_else(|error| panic!("long-stay shelter support geometry failed: {error}"))
    };
    let timber_stack = |state: &AppState, context: &str| {
        project_material_stacks(registries, state, carried)
            .unwrap_or_else(|error| panic!("{context} stack projection failed: {error}"))
            .into_iter()
            .find(|stack| stack.commodity() == timber)
            .unwrap_or_else(|| panic!("{context} timber disappeared"))
    };
    let lower_start = validate_start_player_grounded_structural_construction_from_stack(
        registries,
        state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(shelter_base),
        ),
        &timber_stack(state, "long-stay shelter lower support"),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter lower support failed: {error}"));
    let lower = lower_start.element();
    let lower_work = lower_start
        .commit(state)
        .unwrap_or_else(|error| panic!("long-stay shelter lower support commit failed: {error}"));
    advance_to(
        registries,
        state,
        lower_work.completes_at(),
        TickEventAllowance {
            structural_construction: Some(lower),
            ..TickEventAllowance::default()
        },
        "long-stay shelter lower support",
    );
    let middle_start = validate_start_player_supported_structural_construction_from_stack(
        registries,
        state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(shelter_middle),
            lower,
        ),
        &timber_stack(state, "long-stay shelter middle support"),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter middle support failed: {error}"));
    let middle = middle_start.element();
    let middle_work = middle_start
        .commit(state)
        .unwrap_or_else(|error| panic!("long-stay shelter middle support commit failed: {error}"));
    advance_to(
        registries,
        state,
        middle_work.completes_at(),
        TickEventAllowance {
            structural_construction: Some(middle),
            ..TickEventAllowance::default()
        },
        "long-stay shelter middle support",
    );
    let upper_start = validate_start_player_supported_structural_construction_from_stack(
        registries,
        state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            support_geometry(shelter_upper),
            middle,
        ),
        &timber_stack(state, "long-stay shelter upper support"),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter upper support failed: {error}"));
    let upper = upper_start.element();
    let upper_work = upper_start
        .commit(state)
        .unwrap_or_else(|error| panic!("long-stay shelter upper support commit failed: {error}"));
    advance_to(
        registries,
        state,
        upper_work.completes_at(),
        TickEventAllowance {
            structural_construction: Some(upper),
            ..TickEventAllowance::default()
        },
        "long-stay shelter upper support",
    );

    assert_eq!(
        assess_player_overhead_cover(registries, state),
        Some(PlayerOverheadCoverAssessment::Open)
    );
    validate_set_active_selection(
        registries,
        state,
        Some(PlayerActiveSelection::material(boards)),
    )
    .unwrap_or_else(|error| panic!("long-stay shelter board selection failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("long-stay shelter board selection commit failed: {error}"));
    // The same camera aim is not guaranteed to hit the upper support from different
    // sub-voxel stopping positions. Treat a reported body-intersection or absent target
    // as useful feedback and adjust the view, just as the player would.
    let mut roof_work = None;
    let mut last_rejection = None;
    for (horizontal, upward) in [(707_107, 707_107), (600_000, 800_000), (800_000, 600_000)] {
        let roof_view = ViewDirection::new(horizontal, upward, 0)
            .unwrap_or_else(|error| panic!("long-stay shelter roof view failed: {error}"));
        let direct = PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(roof_view));
        let first = advance_control_tick_with_direct_input(registries, state, direct)
            .unwrap_or_else(|error| panic!("long-stay shelter roof action failed: {error}"));
        let mut attempted = first.secondary_action().cloned();
        let mut queued_ticks = 0_u64;
        while attempted.is_none()
            || matches!(
                attempted,
                Some(SecondaryActionOutcome::QueuedUntilSimulationBoundary)
            )
        {
            assert!(
                queued_ticks < 72,
                "long-stay roof queued input did not resolve"
            );
            let control = advance_control_tick(registries, state)
                .unwrap_or_else(|error| panic!("long-stay roof continuation failed: {error}"));
            attempted = control.secondary_action().cloned();
            queued_ticks += 1;
        }
        match attempted {
            Some(SecondaryActionOutcome::StructuralConstructionStarted(work)) => {
                roof_work = Some(work);
                break;
            }
            Some(
                rejected @ (SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { .. }
                | SecondaryActionOutcome::NoStructuralPlacementTarget),
            ) => last_rejection = Some(rejected),
            other => panic!("long-stay shelter roof rejected after aiming: {other:?}"),
        }
    }
    assert!(
        roof_work.is_some(),
        "long-stay roof remained untargetable after view adjustment: {last_rejection:?}"
    );
    let roof_work = roof_work.unwrap_or_else(|| unreachable!("long-stay shelter roof resolved"));
    let roof = roof_work.element();
    let roof_record = state
        .structures()
        .get_element(roof)
        .unwrap_or_else(|| panic!("long-stay shelter roof disappeared after admission"));
    assert!(roof_record.bounds().has_voxel(panel_position));
    assert_eq!(roof_record.embodied_mass(), panel_mass);
    assert_eq!(
        state
            .structures()
            .supports(roof)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![upper])
    );
    assert_eq!(
        assess_player_overhead_cover(registries, state),
        Some(PlayerOverheadCoverAssessment::Open),
        "planned long-stay roof must not reduce exposure"
    );
    advance_to(
        registries,
        state,
        roof_work.completes_at(),
        TickEventAllowance {
            structural_construction: Some(roof),
            ..TickEventAllowance::default()
        },
        "long-stay shelter roof hand-setting",
    );
    assert_eq!(
        assess_player_overhead_cover(registries, state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );

    // Isolate the roof's authored exposure consequence in an exact continuation of the completed
    // camp. The lived opening remains at roof completion with its finite fire still burning; the
    // cloned continuation only removes that additive warmth before measuring covered cooling.
    let mut exposure_probe = state.clone();
    while exposure_probe
        .energy()
        .get_store(campfire)
        .is_some_and(|store| !store.stored().is_zero())
    {
        let outcome = advance_tick(registries, &mut exposure_probe)
            .unwrap_or_else(|error| panic!("long-stay shelter fire rundown failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance::default(),
            "long-stay shelter fire rundown",
        );
    }
    let thermal = registries.survival().physiology().thermal_comfort();
    let comfort_before = assess_survival(registries, &exposure_probe)
        .unwrap_or_else(|| panic!("long-stay shelter survival assessment disappeared"))
        .thermal_comfort_ppm();
    let covered_tick = advance_tick(registries, &mut exposure_probe)
        .unwrap_or_else(|error| panic!("long-stay shelter covered tick failed: {error}"));
    assert_tick_events_within(
        &covered_tick,
        TickEventAllowance::default(),
        "long-stay shelter covered survival",
    );
    let comfort_after = assess_survival(registries, &exposure_probe)
        .unwrap_or_else(|| panic!("long-stay shelter covered result disappeared"))
        .thermal_comfort_ppm();
    let observed_cover_loss = comfort_before.saturating_sub(comfort_after);
    assert_eq!(
        observed_cover_loss,
        thermal.overhead_cover_loss_ppm_per_tick(),
        "long-stay shelter must settle the authored covered exposure after finite fire heat is gone"
    );
    assert!(thermal.overhead_cover_loss_ppm_per_tick() < thermal.unsheltered_loss_ppm_per_tick());

    WildernessShelterReview {
        tool: assembly_option.definition(),
        logistics_legs: 4,
        stone_gathered_mg: stone_required.milligrams(),
        staged_timber_used_mg: staged_timber.milligrams(),
        fresh_timber_gathered_mg: fresh_timber.milligrams(),
        timber_source_remaining_mg,
        peak_carried_mass_mg,
        support_count: 3,
        support_mass_mg: support_mass.milligrams(),
        panel_mass_mg: panel_mass.milligrams(),
        board_batches: workload_batches,
        setup_ticks,
        bare_ticks,
        assisted_ticks,
        with_setup_ticks,
        saved_ticks: bare_ticks - with_setup_ticks,
        open_loss_ppm_per_tick: thermal.unsheltered_loss_ppm_per_tick(),
        covered_loss_ppm_per_tick: thermal.overhead_cover_loss_ppm_per_tick(),
        observed_loss_ppm_per_tick: observed_cover_loss,
        board_output_mg: expected_board_mass.milligrams(),
        tool_worn,
        postfire_deferred_refuel: false,
        postfire_covered_comfort_ppm: 0,
        postfire_open_comfort_ppm: 0,
        postfire_fuel_only_comfort_ppm: 0,
    }
}

pub(super) fn evaluate_generated_wilderness_opening(
    seed: WorldGenerationSeed,
    start_profile: WildernessStartProfile,
    camp_goal: WildernessCampGoal,
) -> WildernessOpeningReview {
    evaluate_generated_wilderness_opening_internal(seed, start_profile, camp_goal)
}

fn evaluate_generated_wilderness_opening_internal(
    seed: WorldGenerationSeed,
    start_profile: WildernessStartProfile,
    camp_goal: WildernessCampGoal,
) -> WildernessOpeningReview {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("wilderness initial generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("wilderness initial generation commit failed: {error}"));

    // Ordinary admission owns a fixed body-scale carrying limit. The opening must adapt to that
    // capability instead of resizing the player to whatever the current script intends to gather.
    let forage_mass = camp_goal.forage_mass();
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
        WildernessStartProfile::Rested => validate_admit_player(&registries, &state, spawn)
            .unwrap_or_else(|error| panic!("wilderness player admission failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wilderness player admission commit failed: {error}"))
            .carried_stockpile(),
        WildernessStartProfile::HungerPressure => {
            admit_player_at_hunger_warning_boundary(&registries, &mut state, spawn)
                .carried_stockpile()
        }
        WildernessStartProfile::HydrationPressure => {
            admit_player_at_hydration_warning_boundary(&registries, &mut state, spawn)
                .carried_stockpile()
        }
    };
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("wilderness terrain neighborhood demand failed: {error}"))
    {
        demand.commit(&mut state).unwrap_or_else(|error| {
            panic!("wilderness terrain neighborhood commit failed: {error}")
        });
    }
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
    let available_steps = state
        .available_adjacent_traversal_destinations()
        .collect::<Vec<_>>();
    assert_eq!(
        available_steps,
        vec![adjacent_berries.position()],
        "fresh-world access should expose exactly the first generated trail edge"
    );
    let initial_step_choices = u64::try_from(available_steps.len())
        .unwrap_or_else(|_| panic!("wilderness available-step count exceeds u64"));
    let mut max_step_choices = initial_step_choices;
    let mut total_steps = 0_u64;
    let blocked_destination = [(1_i64, 0_i64), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|(dx, dz)| VoxelCoord::new(spawn.x() + dx, spawn.y(), spawn.z() + dz))
        .find(|candidate| *candidate != adjacent_berries.position())
        .unwrap_or_else(|| unreachable!("opening has more than one horizontal neighbor"));
    assert!(
        !available_steps.contains(&blocked_destination),
        "bounded opening route observation must not advertise an unauthored adjacent branch"
    );
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
    walk_continuous_body_adjacent_and_settle(
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
    let mut peak_carried_mass_mg = carrying_after_forage.stored().milligrams();
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
        food_secured_microseconds > 0 && food_secured_microseconds <= 3 * minute,
        "first-session food acquisition should take meaningful physical time but finish within three minutes"
    );

    let mut meal_consumed_mg = 0_u64;
    if start_profile == WildernessStartProfile::HungerPressure {
        let carrying = assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("hunger-pressure opening lost carrying state before meal"));
        let locomotion = physiology.locomotion();
        // The next branch has not been selected yet, so no exact continuous path exists to project.
        // Price one full slow settlement interval at the current load as a conservative upper
        // bound. Every leg actually chosen later is exact-projected and checked against control-rate
        // execution by `walk_body_to_adjacent_and_settle`.
        let conservative_next_leg_budget = project_survival_resource_budget(
            physiology,
            locomotion.exertion_for_load(carrying.stored(), carrying.capacity()),
            TickSpan::new(SETTLED_ADJACENT_LEG_SLOW_TICKS),
        )
        .unwrap_or_else(|error| {
            panic!("hunger-pressure conservative next-leg projection failed: {error:?}")
        });
        let metabolic_target = physiology
            .hungry_below()
            .checked_add(conservative_next_leg_budget.metabolic_energy())
            .and_then(|target| target.checked_add(Energy::from_nanojoules(1)))
            .unwrap_or_else(|| panic!("hunger-pressure opening metabolic target overflowed"));
        let berry_stack = carried_material_stack(
            &registries,
            &state,
            berry_commodity,
            "hunger-pressure opening meal",
        );
        let meal = validate_eat_stack_to_metabolic_target(
            &registries,
            &state,
            &berry_stack,
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
    let mut retained_food_mass = Mass::from_milligrams(
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
    let stone_crock_craft = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("wilderness stone-crock shaping disappeared"));
    let stone_crock_raw_stone = stone_crock_craft.input_mass();
    let stone_gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .copied()
        .unwrap_or_else(|| panic!("wilderness stone gathering disappeared"));
    let stone_crock_gather_ticks = stone_gathering
        .duration_for_mass(stone_crock_raw_stone)
        .map(|duration| duration.value())
        .unwrap_or_else(|| panic!("wilderness stone-crock gathering cannot schedule"));
    let stone_crock_build_ticks = stone_crock_gather_ticks
        .checked_add(stone_crock_craft.duration().value())
        .unwrap_or_else(|| panic!("wilderness stone-crock build projection overflowed"));
    let stone_crock_storage = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("wilderness stone-crock storage definition disappeared"));
    let stone_crock_food_shelf_ticks = u64::try_from(
        u128::from(ambient_food_shelf_ticks)
            .checked_mul(u128::from(
                stone_crock_storage
                    .storage_profile()
                    .preservation_multiplier_ppm(),
            ))
            .unwrap_or_else(|| panic!("wilderness stone-crock shelf-life projection overflowed"))
            / 1_000_000,
    )
    .unwrap_or_else(|_| panic!("wilderness stone-crock shelf-life projection exceeds u64"));
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
    assert_eq!(
        stone_crock_food_shelf_ticks,
        registries.core().calendar().ticks_per_day() * 10,
        "carved stone crock should extend opening berries from four to ten days"
    );
    assert!(
        field_box_storage.maximum_stockpile_capacity()
            > stone_crock_storage.maximum_stockpile_capacity(),
        "timber field box should remain the primitive bulk-storage choice"
    );
    assert!(
        stone_crock_storage
            .storage_profile()
            .preservation_multiplier_ppm()
            > field_box_storage
                .storage_profile()
                .preservation_multiplier_ppm(),
        "stone crock should remain the primitive preservation choice"
    );
    let projected_budget = |exertion, duration: TickSpan, context: &str| {
        project_survival_resource_budget(physiology, exertion, duration).unwrap_or_else(|error| {
            panic!("wilderness {context} survival projection failed: {error:?}")
        })
    };
    // Future project logistics below include legs whose exact source voxel or post-work carried load
    // has not been reached yet. Use one full slow settlement interval as a conservative survival
    // upper bound for each such leg. Lived legs are never executed from this estimate: they are
    // exact-projected from the reached body state immediately before control-rate movement.
    let conservative_future_leg_budget = |load: Mass, context: &str| {
        projected_budget(
            physiology
                .locomotion()
                .exertion_for_load(load, physiology.carried_mass_capacity()),
            TickSpan::new(SETTLED_ADJACENT_LEG_SLOW_TICKS),
            context,
        )
    };
    let worksite_staged_timber = Mass::from_milligrams(6_000_000);
    let campfire_stone_mass = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .and_then(|definition| definition.assembly_profile())
        .and_then(|assembly| assembly.inputs().first())
        .map(|input| input.mass())
        .unwrap_or_else(|| panic!("wilderness campfire stone input disappeared"));
    let campfire_stone_gather_ticks = stone_gathering
        .duration_for_mass(campfire_stone_mass)
        .unwrap_or_else(|| panic!("wilderness campfire stone gathering cannot schedule"))
        .value();
    const CAMPFIRE_RECOVERY_BUDGET_TICKS: u64 = 20;
    let (worksite_support_mass, worksite_support_duration) =
        worksite_support_projection(&registries);
    let worksite_second_trip_timber = worksite_staged_timber
        .checked_add(worksite_support_mass)
        .unwrap_or_else(|| panic!("wilderness worksite second-trip timber overflowed"));
    let mut worksite_second_trip_remaining = worksite_second_trip_timber;
    let mut worksite_second_trip_gather_ticks = 0_u64;
    while !worksite_second_trip_remaining.is_zero() {
        let batch = worksite_second_trip_remaining.min(timber_gathering.maximum_batch_mass());
        let ticks = timber_gathering
            .duration_for_mass(batch)
            .unwrap_or_else(|| {
                panic!("wilderness worksite second-trip timber batch cannot schedule")
            })
            .value();
        worksite_second_trip_gather_ticks = worksite_second_trip_gather_ticks
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("wilderness worksite second-trip gathering overflowed"));
        worksite_second_trip_remaining = worksite_second_trip_remaining
            .checked_sub(batch)
            .unwrap_or_else(|| {
                unreachable!("bounded worksite second-trip batch cannot exceed remainder")
            });
    }
    let worksite_budget = || {
        let construction_return_load = retained_food_mass
            .checked_add(field_box_raw_timber)
            .unwrap_or_else(|| panic!("wilderness field-box return load overflowed"));
        projected_budget(
            timber_gathering.exertion(),
            TickSpan::new(
                field_box_gather_ticks
                    .checked_add(worksite_second_trip_gather_ticks)
                    .unwrap_or_else(|| panic!("wilderness worksite gathering duration overflowed")),
            ),
            "field-box, support, and staging gathering",
        )
        .checked_add(projected_budget(
            board_craft.exertion(),
            TickSpan::new(bare_board_ticks),
            "field-box board shaping",
        ))
        .and_then(|budget| {
            budget.checked_add(projected_budget(
                field_box_joinery.exertion(),
                field_box_joinery.duration(),
                "field-box joinery",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(projected_budget(
                registries
                    .labor()
                    .get_structural_construction(STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER)
                    .copied()
                    .unwrap_or_else(|| panic!("wilderness worksite structural labor disappeared"))
                    .exertion(),
                worksite_support_duration,
                "worksite grounded-support construction",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(conservative_future_leg_budget(
                construction_return_load,
                "field-box construction return",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(conservative_future_leg_budget(
                Mass::ZERO,
                "worksite-cache empty outbound trip",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(conservative_future_leg_budget(
                worksite_staged_timber,
                "worksite-cache loaded return",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(projected_budget(
                stone_gathering.exertion(),
                TickSpan::new(campfire_stone_gather_ticks),
                "campfire stone gathering",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(conservative_future_leg_budget(
                Mass::ZERO,
                "campfire stone outbound trip",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(conservative_future_leg_budget(
                campfire_stone_mass,
                "campfire stone loaded return",
            ))
        })
        .and_then(|budget| {
            budget.checked_add(projected_budget(
                SurvivalExertion::REST,
                TickSpan::new(CAMPFIRE_RECOVERY_BUDGET_TICKS),
                "campfire warmth recovery",
            ))
        })
        .unwrap_or_else(|| panic!("wilderness field-box survival budget overflowed"))
    };
    let camp_project_budget = match camp_goal {
        WildernessCampGoal::StayMobile => SurvivalResourceBudget::ZERO,
        WildernessCampGoal::WorksiteCache => worksite_budget(),
        WildernessCampGoal::LongStayShelter => {
            let knapping = registries
                .crafting()
                .get_manual(PROCESS_KNAP_STONE_TOOL)
                .unwrap_or_else(|| panic!("wilderness long-stay stone-tool recipe disappeared"));
            let handle = registries
                .crafting()
                .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
                .unwrap_or_else(|| panic!("wilderness long-stay handle recipe disappeared"));
            let panel_mass = calculate_prismatic_material_mass_ceiling(
                registries.materials(),
                MATERIAL_WOOD,
                BASIC_TIMBER_COVER_PANEL_AREA,
                BASIC_TIMBER_COVER_PANEL_THICKNESS,
            )
            .unwrap_or_else(|error| panic!("wilderness long-stay panel mass failed: {error}"));
            let board_output = board_craft
                .outputs()
                .iter()
                .find(|output| output.commodity() == board_commodity)
                .map(|output| output.mass())
                .unwrap_or_else(|| panic!("wilderness long-stay board output disappeared"));
            let shelter_board_batches = panel_mass.milligrams().div_ceil(board_output.milligrams());
            let (_, _, shelter_assisted_ticks, _) =
                project_wilderness_adze_decision(&registries, shelter_board_batches);
            let shelter_support_mass = calculate_prismatic_material_mass_ceiling(
                registries.materials(),
                MATERIAL_WOOD,
                BASIC_TIMBER_POST_CROSS_SECTION,
                BASIC_TIMBER_POST_LENGTH,
            )
            .unwrap_or_else(|error| panic!("wilderness long-stay support mass failed: {error}"));
            let shelter_support_timber = Mass::from_milligrams(
                shelter_support_mass
                    .milligrams()
                    .checked_mul(3)
                    .unwrap_or_else(|| panic!("wilderness long-stay support timber overflowed")),
            );
            let shelter_timber = Mass::from_milligrams(
                handle
                    .input_mass()
                    .milligrams()
                    .checked_add(
                        board_craft
                            .input_mass()
                            .milligrams()
                            .checked_mul(shelter_board_batches)
                            .unwrap_or_else(|| {
                                panic!("wilderness long-stay board timber overflowed")
                            }),
                    )
                    .and_then(|mass| mass.checked_add(shelter_support_timber.milligrams()))
                    .unwrap_or_else(|| panic!("wilderness long-stay timber overflowed")),
            );
            let staged_after_fire = worksite_staged_timber
                .checked_sub(CAMPFIRE_FUEL_MASS)
                .unwrap_or_else(|| panic!("campfire fuel exceeds worksite staged timber"));
            let fresh_shelter_timber = shelter_timber
                .checked_sub(staged_after_fire.min(shelter_timber))
                .unwrap_or_else(|| unreachable!("staged shelter timber is demand-capped"));
            let mut remaining = fresh_shelter_timber;
            let mut fresh_timber_gather_ticks = 0_u64;
            while !remaining.is_zero() {
                let batch = remaining.min(timber_gathering.maximum_batch_mass());
                fresh_timber_gather_ticks = fresh_timber_gather_ticks
                    .checked_add(
                        timber_gathering
                            .duration_for_mass(batch)
                            .unwrap_or_else(|| {
                                panic!("wilderness long-stay timber batch cannot schedule")
                            })
                            .value(),
                    )
                    .unwrap_or_else(|| panic!("wilderness long-stay gather time overflowed"));
                remaining = remaining
                    .checked_sub(batch)
                    .unwrap_or_else(|| unreachable!("long-stay gather batch is bounded"));
            }
            let structural = registries
                .labor()
                .get_structural_construction(STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER)
                .copied()
                .unwrap_or_else(|| panic!("wilderness long-stay structural labor disappeared"));
            let support_ticks = structural
                .duration_for_mass(shelter_support_mass)
                .unwrap_or_else(|| panic!("wilderness long-stay support cannot schedule"))
                .value();
            let panel_ticks = structural
                .duration_for_mass(panel_mass)
                .unwrap_or_else(|| panic!("wilderness long-stay roof cannot schedule"))
                .value();
            let total_structural_ticks = support_ticks
                .checked_mul(3)
                .and_then(|ticks| ticks.checked_add(panel_ticks))
                .unwrap_or_else(|| panic!("wilderness long-stay structural time overflowed"));
            let stone_gather_ticks = stone_gathering
                .duration_for_mass(knapping.input_mass())
                .unwrap_or_else(|| panic!("wilderness long-stay stone gathering cannot schedule"));
            let stone_and_staged_load = knapping
                .input_mass()
                .checked_add(staged_after_fire.min(shelter_timber))
                .unwrap_or_else(|| panic!("wilderness long-stay outbound load overflowed"));
            let return_load = knapping
                .input_mass()
                .checked_add(shelter_timber)
                .unwrap_or_else(|| panic!("wilderness long-stay return load overflowed"));
            worksite_budget()
                .checked_add(projected_budget(
                    stone_gathering.exertion(),
                    stone_gather_ticks,
                    "long-stay adze stone gathering",
                ))
                .and_then(|budget| {
                    budget.checked_add(projected_budget(
                        timber_gathering.exertion(),
                        TickSpan::new(fresh_timber_gather_ticks),
                        "long-stay fresh timber gathering",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(projected_budget(
                        knapping.exertion(),
                        knapping.duration(),
                        "long-stay stone-tool knapping",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(projected_budget(
                        handle.exertion(),
                        handle.duration(),
                        "long-stay handle shaping",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(projected_budget(
                        board_craft.exertion(),
                        TickSpan::new(shelter_assisted_ticks),
                        "long-stay adze board shaping",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(projected_budget(
                        structural.exertion(),
                        TickSpan::new(total_structural_ticks),
                        "long-stay shelter hand-setting",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(conservative_future_leg_budget(
                        Mass::ZERO,
                        "long-stay stone outbound trip",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(conservative_future_leg_budget(
                        knapping.input_mass(),
                        "long-stay stone loaded return",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(conservative_future_leg_budget(
                        stone_and_staged_load,
                        "long-stay timber outbound trip",
                    ))
                })
                .and_then(|budget| {
                    budget.checked_add(conservative_future_leg_budget(
                        return_load,
                        "long-stay timber loaded return",
                    ))
                })
                .unwrap_or_else(|| panic!("wilderness long-stay survival budget overflowed"))
        }
        WildernessCampGoal::PreserveProvisions => {
            let return_load = retained_food_mass
                .checked_add(stone_crock_raw_stone)
                .unwrap_or_else(|| panic!("wilderness stone-crock return load overflowed"));
            projected_budget(
                stone_gathering.exertion(),
                TickSpan::new(stone_crock_gather_ticks),
                "stone-crock gathering",
            )
            .checked_add(projected_budget(
                stone_crock_craft.exertion(),
                stone_crock_craft.duration(),
                "stone-crock shaping",
            ))
            .and_then(|budget| {
                budget.checked_add(conservative_future_leg_budget(
                    return_load,
                    "stone-crock loaded return",
                ))
            })
            .unwrap_or_else(|| panic!("wilderness stone-crock survival budget overflowed"))
        }
    };
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
        carried_material_stack(
            &registries,
            &state,
            berry_commodity,
            "retained opening provisions",
        )
        .mass(),
        retained_food_mass,
        "opening should consume food only when current hunger makes the meal useful"
    );

    let forage_position = adjacent_berries.position();
    let forage_steps = state
        .available_adjacent_traversal_destinations()
        .collect::<Vec<_>>();
    let forage_step_choices = u64::try_from(forage_steps.len())
        .unwrap_or_else(|_| panic!("wilderness forage-step choice count exceeds u64"));
    max_step_choices = max_step_choices.max(forage_step_choices);
    let adjacent_water = adjacent_drinkable_source(&registries, &state);
    let next_resources = available_adjacent_surface_resources(&state).collect::<Vec<_>>();
    assert_eq!(
        next_resources.len(),
        1,
        "forage should reveal only the work-junction protein, while water-side clay is another leg away"
    );
    assert_eq!(
        next_resources[0].commodity(),
        CommodityKey::new(deep_hearth::content::MATERIAL_WILD_NUTS, FORM_FOOD)
    );
    let junctions = forage_steps
        .iter()
        .copied()
        .filter(|position| *position != spawn && *position != adjacent_water.position())
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
    let known_junction_budget = project_continuous_body_path_budget_for_load(
        &registries,
        &state,
        &[junction],
        retained_food_mass,
        "known forage-to-junction project approach",
    );
    let hidden_material_leg_budget = match camp_goal {
        WildernessCampGoal::StayMobile => SurvivalResourceBudget::ZERO,
        WildernessCampGoal::WorksiteCache
        | WildernessCampGoal::LongStayShelter
        | WildernessCampGoal::PreserveProvisions => conservative_future_leg_budget(
            retained_food_mass,
            "still-hidden material-leaf approach upper bound",
        ),
    };
    let project_from_forage_budget = camp_project_budget
        .checked_add(known_junction_budget)
        .and_then(|budget| budget.checked_add(hidden_material_leg_budget))
        .unwrap_or_else(|| panic!("wilderness pre-project route budget overflowed"));
    let hydration_working_target = |budget: SurvivalResourceBudget| {
        physiology
            .thirsty_below()
            .checked_add(budget.hydration())
            .and_then(|target| target.checked_add(minimum_cup))
            .unwrap_or(physiology.maximum_hydration())
            .min(physiology.maximum_hydration())
    };
    let no_water_target = hydration_working_target(project_from_forage_budget);
    let water_needed = water_decision.hydration() < no_water_target;
    assert_eq!(
        water_needed,
        matches!(start_profile, WildernessStartProfile::HydrationPressure),
        "wilderness setup profile must create the intended actor-visible hydration pressure without controlling the route directly"
    );
    let mut water_visited = false;
    let mut water_source_volume_ul = None;
    let mut water_remaining_ul = None;
    let mut water_rest_runway_ticks = None;
    let mut water_consumed_ul = 0_u64;
    if water_needed {
        walk_continuous_body_adjacent_and_settle(
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
        let return_to_forage_budget = project_continuous_body_path_budget_for_load(
            &registries,
            &state,
            &[forage_position],
            retained_food_mass,
            "known water-to-forage return",
        );
        let project_after_drink_budget = project_from_forage_budget
            .checked_add(return_to_forage_budget)
            .unwrap_or_else(|| panic!("wilderness water-return project budget overflowed"));
        let hydration_target = hydration_working_target(project_after_drink_budget);
        let drink =
            validate_drink_store_to_hydration_target(&registries, &state, water, hydration_target)
                .unwrap_or_else(|error| {
                    panic!("wilderness needed drink projection failed: {error}")
                })
                .unwrap_or_else(|| {
                    panic!("actor-visible hydration target unexpectedly needed no drink")
                })
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
            "needed opening drink should establish the project-aware hydration buffer"
        );
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            forage_position,
            "return from optional water",
        );
        total_steps += 1;
    } else {
        assert!(
            water_decision.hydration() >= no_water_target,
            "water may be skipped only when observed hydration already covers the disclosed project working reserve"
        );
    }

    // The material junction is the first genuine world-choice moment in the default opening. The
    // actor can see both raw opportunities at once and need not visit either merely to satisfy a
    // harness checklist when there is no current workload worth paying for.
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "material junction",
    );
    total_steps += 1;
    let junction_step_choices =
        u64::try_from(state.available_adjacent_traversal_destinations().count())
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
    let mut selected_storage = None;
    let mut selected_material = None;
    let mut storage_project_ticks = 0_u64;
    let mut cached_food_mass_mg = 0_u64;
    let mut cached_total_mass_mg = 0_u64;
    let mut staged_worksite_timber_mg = 0_u64;
    let mut worksite_construction_residue_mg = 0_u64;
    let mut worksite_support_built = false;
    let mut worksite_support_mass_mg = 0_u64;
    let mut worksite_support_ticks = 0_u64;
    let mut campfire_built = false;
    let mut campfire_fuel_mg = 0_u64;
    let mut campfire_logistics_steps = 0_u64;
    let mut campfire_recovery_ticks = 0_u64;
    let mut campfire_single_charge_runway_ticks = 0_u64;
    let mut thermal_before_fire_ppm = 0_u32;
    let mut thermal_after_fire_ppm = 0_u32;
    let mut campfire_heat_remaining_nj = 0_u128;
    let mut campfire_id = None;
    let mut campfire_runway_before_refuel_ticks = 0_u64;
    let mut campfire_refuel_retrieval_ticks = 0_u64;
    let mut campfire_refueled = false;
    let mut campfire_recycled_chips_mg = 0_u64;
    let mut campfire_one_hour_comfort_with_refuel_ppm = 0_u32;
    let mut campfire_one_hour_comfort_without_refuel_ppm = 0_u32;
    let mut campfire_light_level = 0_u8;
    let mut fire_light_before_refuel = 0_u8;
    let mut fire_light_after_refuel = 0_u8;
    let mut selected_source_opportunity_mg = 0_u64;
    let mut selected_source_remaining_mg = 0_u64;
    let mut camp_logistics_steps = 0_u64;
    let mut selected_storage_capacity_mg = 0_u64;
    let mut selected_storage_preservation_ppm = 0_u32;
    let mut project_meal_consumed_mg = 0_u64;
    let mut shelter = None;
    if camp_goal != WildernessCampGoal::StayMobile {
        const WORKSITE_STAGED_TIMBER_MG: u64 = 6_000_000;
        const PRESERVATION_TARGET_DAYS: u64 = 8;
        match camp_goal {
            WildernessCampGoal::StayMobile => unreachable!(),
            WildernessCampGoal::WorksiteCache | WildernessCampGoal::LongStayShelter => {
                assert!(
                    retained_food_mass > Mass::ZERO,
                    "worksite-cache goal requires some provisions to stage at the camp node"
                );
                let staged_total = WORKSITE_STAGED_TIMBER_MG
                    .checked_add(retained_food_mass.milligrams())
                    .unwrap_or_else(|| panic!("wilderness worksite-cache target overflowed"));
                assert!(
                    stone_crock_storage
                        .maximum_stockpile_capacity()
                        .milligrams()
                        < staged_total
                        && field_box_storage.maximum_stockpile_capacity().milligrams()
                            >= staged_total,
                    "six kilograms of staged timber plus provisions must require the timber box rather than fit in the stone crock"
                );
            }
            WildernessCampGoal::PreserveProvisions => {
                let ticks_per_day = registries.core().calendar().ticks_per_day();
                assert!(
                    field_box_food_shelf_ticks / ticks_per_day < PRESERVATION_TARGET_DAYS
                        && stone_crock_food_shelf_ticks / ticks_per_day >= PRESERVATION_TARGET_DAYS,
                    "preservation goal must distinguish the stone crock from the shorter-lived timber box"
                );
            }
        }
        let (storage_definition, material, gathering_mass) = match camp_goal {
            WildernessCampGoal::StayMobile => {
                unreachable!("mobile goal does not build storage")
            }
            WildernessCampGoal::WorksiteCache | WildernessCampGoal::LongStayShelter => (
                STORAGE_ROUGH_TIMBER_FIELD_BOX,
                timber_commodity,
                field_box_raw_timber,
            ),
            WildernessCampGoal::PreserveProvisions => (
                STORAGE_CARVED_STONE_PROVISIONS_CROCK,
                stone_commodity,
                stone_crock_raw_stone,
            ),
        };
        let selected = adjacent_materials
            .iter()
            .copied()
            .find(|resource| resource.commodity() == material)
            .unwrap_or_else(|| {
                panic!("wilderness selected material is not adjacent at the junction")
            });
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            selected.position(),
            "selected camp-material approach",
        );
        total_steps += 1;
        camp_logistics_steps += 1;
        let project_survival = assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("wilderness player disappeared before camp project"));
        let metabolic_target = physiology
            .hungry_below()
            .checked_add(camp_project_budget.metabolic_energy())
            .and_then(|target| target.checked_add(Energy::from_nanojoules(1)))
            .unwrap_or_else(|| panic!("wilderness camp-project metabolic target overflowed"));
        if project_survival.metabolic_energy() < metabolic_target {
            let berry_stack =
                carried_material_stack(&registries, &state, berry_commodity, "camp-project meal");
            let meal = validate_eat_stack_to_metabolic_target(
                &registries,
                &state,
                &berry_stack,
                metabolic_target,
            )
            .unwrap_or_else(|error| {
                panic!("wilderness camp-project meal projection failed: {error}")
            })
            .unwrap_or_else(|| {
                panic!("wilderness camp project required provisioning but produced no meal")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wilderness camp-project meal commit failed: {error}"));
            project_meal_consumed_mg = meal.total_mass().milligrams();
            meal_consumed_mg = meal_consumed_mg
                .checked_add(project_meal_consumed_mg)
                .unwrap_or_else(|| panic!("wilderness meal accounting overflowed"));
            retained_food_mass = retained_food_mass
                .checked_sub(meal.total_mass())
                .unwrap_or_else(|| panic!("wilderness camp-project meal exceeded retained food"));
            advance_to(
                &registries,
                &mut state,
                meal.completes_at(),
                TickEventAllowance::default(),
                "camp-project meal",
            );
            assert!(
                assess_survival(&registries, &state)
                    .is_some_and(|survival| survival.metabolic_energy() >= metabolic_target),
                "camp-project meal must establish the disclosed working reserve"
            );
        }
        let project_started_at = state.tick();
        let resource = local_surface_resource(&state, material);
        assert_eq!(resource, selected.resource());
        selected_source_opportunity_mg = state
            .available_surface_resources()
            .find(|record| record.id() == resource)
            .map(|record| record.remaining_mass().milligrams())
            .unwrap_or_else(|| panic!("selected wilderness source disappeared before gathering"));
        gather(
            &registries,
            &mut state,
            resource,
            gathering_mass,
            "selected camp material",
        );
        let carrying_after_material = assess_player_carrying(&state).unwrap_or_else(|| {
            panic!("wilderness carrying assessment disappeared during camp project")
        });
        peak_carried_mass_mg =
            peak_carried_mass_mg.max(carrying_after_material.stored().milligrams());
        selected_source_remaining_mg = state
            .available_surface_resources()
            .find(|record| record.id() == resource)
            .map(|record| record.remaining_mass().milligrams())
            .unwrap_or_else(|| panic!("selected wilderness source disappeared after gathering"));

        // Construction material comes back to the material junction. Storage is camp
        // infrastructure, not a container spawned on top of the resource it is meant to organize.
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            junction,
            "return with camp construction material",
        );
        total_steps += 1;
        camp_logistics_steps += 1;
        let storage_body_commodity = registries
            .storage()
            .get(storage_definition)
            .and_then(|definition| definition.assembly_profile().inputs().first())
            .map(|input| input.commodity())
            .unwrap_or_else(|| panic!("wilderness selected storage body input disappeared"));
        match camp_goal {
            WildernessCampGoal::StayMobile => {
                unreachable!("mobile goal does not craft storage")
            }
            WildernessCampGoal::WorksiteCache | WildernessCampGoal::LongStayShelter => {
                craft_in_place(
                    &registries,
                    &mut state,
                    timber_commodity,
                    board_commodity,
                    board_batches,
                    "field-box boards",
                );
                craft_in_place(
                    &registries,
                    &mut state,
                    board_commodity,
                    storage_body_commodity,
                    1,
                    "field-box body",
                );
            }
            WildernessCampGoal::PreserveProvisions => craft_in_place(
                &registries,
                &mut state,
                stone_commodity,
                storage_body_commodity,
                1,
                "stone-crock body",
            ),
        }
        let storage = validate_place_player_storage(&registries, &state, storage_definition)
            .unwrap_or_else(|error| panic!("wilderness camp storage placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness camp storage placement commit failed: {error}")
            });
        if camp_goal.uses_worksite_cache() {
            let mut residue_mass = Mass::ZERO;
            loop {
                let stack = project_material_stacks(&registries, &state, carried)
                    .unwrap_or_else(|error| {
                        panic!("wilderness construction-residue stack projection failed: {error}")
                    })
                    .into_iter()
                    .find(|stack| stack.commodity().material() == MATERIAL_WOOD);
                let Some(stack) = stack else {
                    break;
                };
                let mass = stack.mass();
                residue_mass = residue_mass
                    .checked_add(mass)
                    .unwrap_or_else(|| panic!("wilderness construction residue overflowed"));
                validate_drop_stack_to_ground(&registries, &state, storage, &stack, mass)
                    .unwrap_or_else(|error| {
                        panic!("wilderness construction-residue cache drop failed: {error}")
                    })
                    .commit(&mut state)
                    .unwrap_or_else(|error| {
                        panic!("wilderness construction-residue cache commit failed: {error}")
                    });
            }
            assert!(
                !residue_mass.is_zero(),
                "field-box fabrication should leave material-backed wood residue to retain at camp"
            );
            worksite_construction_residue_mg = residue_mass.milligrams();
        }
        if !retained_food_mass.is_zero() {
            let berry_stack =
                carried_material_stack(&registries, &state, berry_commodity, "food-cache transfer");
            validate_drop_stack_to_ground(
                &registries,
                &state,
                storage,
                &berry_stack,
                retained_food_mass,
            )
            .unwrap_or_else(|error| panic!("wilderness food-cache drop failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("wilderness food-cache drop commit failed: {error}"));
        }
        if camp_goal.uses_worksite_cache() {
            let staged_timber = Mass::from_milligrams(WORKSITE_STAGED_TIMBER_MG);
            let support_and_staging = staged_timber
                .checked_add(worksite_support_mass)
                .unwrap_or_else(|| panic!("wilderness worksite second-trip timber overflowed"));
            walk_continuous_body_adjacent_and_settle(
                &registries,
                &mut state,
                selected.position(),
                "worksite-cache timber pickup",
            );
            total_steps += 1;
            camp_logistics_steps += 1;
            assert_eq!(
                local_surface_resource(&state, timber_commodity),
                resource,
                "worksite cache must return to the same actor-discovered timber source"
            );
            let remaining_world_timber = state
                .available_surface_resources()
                .find(|record| record.id() == resource)
                .map(|record| record.remaining_mass())
                .unwrap_or_else(|| {
                    panic!("wilderness worksite-cache timber resource disappeared before staging")
                });
            assert!(
                remaining_world_timber >= support_and_staging,
                "generated timber opportunity must support the field-box body, grounded support, and staged reserve"
            );
            gather_in_batches(
                &registries,
                &mut state,
                resource,
                support_and_staging,
                "worksite-cache support and timber staging",
            );
            let carrying_after_staging = assess_player_carrying(&state).unwrap_or_else(|| {
                panic!("wilderness carrying assessment disappeared after bulk staging")
            });
            peak_carried_mass_mg =
                peak_carried_mass_mg.max(carrying_after_staging.stored().milligrams());
            selected_source_remaining_mg = state
                .available_surface_resources()
                .find(|record| record.id() == resource)
                .map(|record| record.remaining_mass().milligrams())
                .unwrap_or_else(|| {
                    panic!("wilderness worksite-cache source disappeared after staging")
                });
            let support_stack = carried_material_stack(
                &registries,
                &state,
                timber_commodity,
                "wilderness worksite grounded support",
            );
            let support_start = validate_start_player_grounded_structural_construction_from_stack(
                &registries,
                &state,
                STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
                GroundedStructuralElementPlan::new(
                    STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                    MATERIAL_WOOD,
                    worksite_support_geometry(junction),
                ),
                &support_stack,
            )
            .unwrap_or_else(|error| panic!("wilderness worksite support start failed: {error}"));
            let support = support_start.element();
            let support_work = support_start.commit(&mut state).unwrap_or_else(|error| {
                panic!("wilderness worksite support commit failed: {error}")
            });
            advance_to(
                &registries,
                &mut state,
                support_work.completes_at(),
                TickEventAllowance {
                    structural_construction: Some(support),
                    ..TickEventAllowance::default()
                },
                "worksite grounded-support construction",
            );
            let support_record = state.structures().get_element(support).unwrap_or_else(|| {
                panic!("wilderness worksite support disappeared after completion")
            });
            assert_eq!(support_record.lifecycle(), StructuralLifecycle::Active);
            assert_eq!(support_record.embodied_mass(), worksite_support_mass);
            worksite_support_built = true;
            worksite_support_mass_mg = worksite_support_mass.milligrams();
            worksite_support_ticks = worksite_support_duration.value();

            let carrying_after_support = assess_player_carrying(&state).unwrap_or_else(|| {
                panic!("wilderness carrying assessment disappeared after support construction")
            });
            assert_eq!(
                carrying_after_support.stored(),
                staged_timber,
                "worksite support should consume exactly its timber before the return haul"
            );
            walk_continuous_body_adjacent_and_settle(
                &registries,
                &mut state,
                junction,
                "return staged timber to worksite cache",
            );
            total_steps += 1;
            camp_logistics_steps += 1;
            let _ = validate_mount_stockpile(&registries, &state, storage, support)
                .unwrap_or_else(|error| panic!("wilderness worksite cache mount failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("wilderness worksite cache mount commit failed: {error}")
                });
            assert_eq!(
                state
                    .inventory()
                    .get_stockpile(storage)
                    .and_then(|record| record.supported_by()),
                Some(support),
                "worksite field box should be structurally supported before accepting staged material"
            );

            let staged_stack = carried_material_stack(
                &registries,
                &state,
                timber_commodity,
                "worksite staged timber",
            );
            validate_drop_stack_to_ground(
                &registries,
                &state,
                storage,
                &staged_stack,
                staged_timber,
            )
            .unwrap_or_else(|error| panic!("wilderness worksite-cache drop failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness worksite-cache drop commit failed: {error}")
            });
            assert!(
                state
                    .structures()
                    .get_element(support)
                    .is_some_and(|record| {
                        record.lifecycle() == StructuralLifecycle::Active
                            && record.load(StructuralLoadKind::StoredMatter) > Force::ZERO
                    }),
                "supported worksite cache should contribute a real stored-matter structural load"
            );
            staged_worksite_timber_mg = staged_timber.milligrams();
        }
        let definition = registries
            .storage()
            .get(storage_definition)
            .unwrap_or_else(|| panic!("wilderness selected storage definition disappeared"));
        let cached = state
            .inventory()
            .get_stockpile(storage)
            .map(|record| record.get_mass(berry_commodity))
            .unwrap_or_else(|| panic!("wilderness selected camp storage disappeared"));
        assert_eq!(cached, retained_food_mass);
        let cached_total = state
            .inventory()
            .get_stockpile(storage)
            .map(|record| record.stored_mass())
            .unwrap_or_else(|| panic!("wilderness selected camp storage disappeared"));
        if camp_goal.uses_worksite_cache() {
            assert_eq!(
                cached_total.milligrams(),
                cached
                    .milligrams()
                    .checked_add(WORKSITE_STAGED_TIMBER_MG)
                    .and_then(|mass| mass.checked_add(worksite_construction_residue_mg))
                    .unwrap_or_else(|| panic!("wilderness cached-total accounting overflowed")),
                "worksite cache must physically retain provisions, woodworking residue, and the staged timber reserve"
            );
            assert!(
                cached_total > stone_crock_storage.maximum_stockpile_capacity(),
                "the lived worksite cache must exceed the alternative stone crock capacity"
            );
        }
        selected_storage = Some(storage_definition);
        selected_material = Some(material);
        cached_food_mass_mg = cached.milligrams();
        cached_total_mass_mg = cached_total.milligrams();
        selected_storage_capacity_mg = definition.maximum_stockpile_capacity().milligrams();
        selected_storage_preservation_ppm =
            definition.storage_profile().preservation_multiplier_ppm();
        storage_project_ticks = state
            .tick()
            .checked_duration_since(project_started_at)
            .unwrap_or_else(|| panic!("wilderness storage project time reversed"))
            .value();
        let projected_ticks = match camp_goal {
            WildernessCampGoal::StayMobile => unreachable!(),
            WildernessCampGoal::WorksiteCache | WildernessCampGoal::LongStayShelter => {
                field_box_build_ticks
                    .checked_add(worksite_second_trip_gather_ticks)
                    .and_then(|ticks| ticks.checked_add(worksite_support_duration.value()))
                    .and_then(|ticks| {
                        ticks.checked_add(SETTLED_ADJACENT_LEG_SLOW_TICKS.checked_mul(3)?)
                    })
                    .unwrap_or_else(|| {
                        panic!("wilderness worksite-cache project duration overflowed")
                    })
            }
            WildernessCampGoal::PreserveProvisions => stone_crock_build_ticks
                .checked_add(SETTLED_ADJACENT_LEG_SLOW_TICKS)
                .unwrap_or_else(|| panic!("wilderness preservation project duration overflowed")),
        };
        assert_eq!(
            storage_project_ticks, projected_ticks,
            "executed primitive storage project should match its disclosed attention projection"
        );

        if camp_goal.uses_worksite_cache() {
            let before_fire = assess_survival(&registries, &state).unwrap_or_else(|| {
                panic!("wilderness player disappeared before campfire response")
            });
            assert_eq!(
                before_fire.thermal_comfort_state(),
                ThermalComfortState::Chilly,
                "the long worksite project should create a natural warmth problem before fire is considered"
            );
            thermal_before_fire_ppm = before_fire.thermal_comfort_ppm();

            let campfire_definition = registries
                .energy()
                .get_store(ENERGY_STONE_RING_CAMPFIRE)
                .unwrap_or_else(|| panic!("wilderness campfire definition disappeared"));
            let campfire_assembly = campfire_definition
                .assembly_profile()
                .unwrap_or_else(|| panic!("wilderness campfire lost its stone assembly profile"));
            assert_eq!(
                campfire_assembly.inputs().len(),
                1,
                "primitive campfire should require one exact local stone input"
            );
            let campfire_stone = &campfire_assembly.inputs()[0];
            assert_eq!(campfire_stone.commodity(), stone_commodity);

            let stone = adjacent_surface_resource(&state, stone_commodity);
            walk_continuous_body_adjacent_and_settle(
                &registries,
                &mut state,
                stone.position(),
                "campfire stone pickup",
            );
            total_steps += 1;
            campfire_logistics_steps += 1;
            let stone_resource = local_surface_resource(&state, stone_commodity);
            assert_eq!(stone_resource, stone.resource());
            gather(
                &registries,
                &mut state,
                stone_resource,
                campfire_stone.mass(),
                "campfire stone",
            );
            peak_carried_mass_mg = peak_carried_mass_mg.max(
                assess_player_carrying(&state)
                    .unwrap_or_else(|| {
                        panic!("wilderness carrying disappeared during campfire build")
                    })
                    .stored()
                    .milligrams(),
            );
            walk_continuous_body_adjacent_and_settle(
                &registries,
                &mut state,
                junction,
                "return campfire stone to worksite",
            );
            total_steps += 1;
            campfire_logistics_steps += 1;

            validate_set_active_selection(
                &registries,
                &state,
                Some(PlayerActiveSelection::material(stone_commodity)),
            )
            .unwrap_or_else(|error| panic!("wilderness campfire stone selection failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness campfire stone selection commit failed: {error}")
            });
            let direct_use_direction = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
                .unwrap_or_else(|error| {
                    panic!("wilderness campfire direct-use direction failed: {error}")
                });
            let assembly = advance_control_tick_with_direct_input(
                &registries,
                &mut state,
                PlayerDirectControlInput::NEUTRAL
                    .with_secondary(SecondaryControlIntent::use_active(direct_use_direction)),
            )
            .unwrap_or_else(|error| panic!("wilderness direct campfire assembly failed: {error}"));
            let campfire = match assembly.secondary_action() {
                Some(SecondaryActionOutcome::EnergyStoreAssembled { definition, store }) => {
                    assert_eq!(*definition, ENERGY_STONE_RING_CAMPFIRE);
                    *store
                }
                other => panic!("wilderness stone use did not assemble campfire: {other:?}"),
            };
            campfire_id = Some(campfire);

            let timber_stacks = project_material_stacks(&registries, &state, storage)
                .unwrap_or_else(|error| {
                    panic!("wilderness campfire fuel projection failed: {error}")
                })
                .into_iter()
                .filter(|stack| stack.commodity() == timber_commodity)
                .collect::<Vec<_>>();
            assert_eq!(
                timber_stacks.len(),
                1,
                "wilderness staged timber should remain one visible physical stack before fueling"
            );
            validate_pickup_stack_from_ground(
                &registries,
                &state,
                storage,
                &timber_stacks[0],
                CAMPFIRE_FUEL_MASS,
            )
            .unwrap_or_else(|error| panic!("wilderness campfire fuel pickup failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness campfire fuel pickup commit failed: {error}")
            });
            validate_set_active_selection(
                &registries,
                &state,
                Some(PlayerActiveSelection::material(timber_commodity)),
            )
            .unwrap_or_else(|error| panic!("wilderness campfire timber selection failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("wilderness campfire timber selection commit failed: {error}")
            });
            let fueling = advance_control_tick_with_direct_input(
                &registries,
                &mut state,
                PlayerDirectControlInput::NEUTRAL
                    .with_secondary(SecondaryControlIntent::use_active(direct_use_direction)),
            )
            .unwrap_or_else(|error| panic!("wilderness direct campfire fueling failed: {error}"));
            match fueling.secondary_action() {
                Some(SecondaryActionOutcome::CombustionFuelAdded { store, released }) => {
                    assert_eq!(*store, campfire);
                    assert_eq!(*released, CAMPFIRE_FUEL_THERMAL_ENERGY);
                }
                other => panic!("wilderness timber use did not fuel campfire: {other:?}"),
            }
            let illumination = assess_local_illumination(&registries, &state, junction);
            assert_eq!(
                illumination.strongest_source(),
                Some(campfire),
                "the fueled worksite campfire should become the actor's local light source"
            );
            campfire_light_level = illumination.level();
            assert!(
                campfire_light_level > 0,
                "a fueled authored campfire must provide nonzero local illumination"
            );
            campfire_built = true;
            campfire_fuel_mg = CAMPFIRE_FUEL_MASS.milligrams();
            campfire_single_charge_runway_ticks = passive_dissipation_ticks_until_empty(
                &registries,
                ENERGY_STONE_RING_CAMPFIRE,
                CAMPFIRE_FUEL_THERMAL_ENERGY,
            )
            .unwrap_or_else(|error| panic!("wilderness campfire runway projection failed: {error}"))
            .unwrap_or_else(|| panic!("wilderness campfire must have finite passive dissipation"))
            .value();
            assert!(
                campfire_single_charge_runway_ticks
                    >= registries.core().calendar().ticks_per_day() / 24,
                "one kilogram campfire charge should provide at least one authored physical hour of usable fire"
            );

            loop {
                let warmth = assess_survival(&registries, &state)
                    .unwrap_or_else(|| panic!("wilderness player disappeared beside campfire"));
                if warmth.thermal_comfort_ppm() >= physiology.thermal_comfort().warning_below_ppm()
                {
                    thermal_after_fire_ppm = warmth.thermal_comfort_ppm();
                    break;
                }
                let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
                    panic!("wilderness campfire recovery tick failed: {error}")
                });
                assert_tick_events_within(
                    &outcome,
                    TickEventAllowance::default(),
                    "campfire warmth recovery",
                );
                campfire_recovery_ticks = campfire_recovery_ticks
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("wilderness campfire recovery duration overflowed"));
                assert!(
                    campfire_recovery_ticks <= CAMPFIRE_RECOVERY_BUDGET_TICKS,
                    "primitive campfire recovery must stay within the pre-work survival budget"
                );
            }
            assert!(
                thermal_after_fire_ppm > thermal_before_fire_ppm,
                "staying beside a fueled local campfire must reverse the unsheltered warmth trend"
            );
            campfire_heat_remaining_nj = state
                .energy()
                .get_store(campfire)
                .map(|store| store.stored().nanojoules())
                .unwrap_or_else(|| panic!("wilderness campfire disappeared after recovery"));
            assert!(campfire_heat_remaining_nj > 0);

            cached_total_mass_mg = state
                .inventory()
                .get_stockpile(storage)
                .map(|record| record.stored_mass().milligrams())
                .unwrap_or_else(|| panic!("wilderness worksite cache disappeared after fueling"));
            assert_eq!(
                cached_total_mass_mg,
                cached_food_mass_mg
                    .checked_add(WORKSITE_STAGED_TIMBER_MG)
                    .and_then(|mass| mass.checked_add(worksite_construction_residue_mg))
                    .and_then(|mass| mass.checked_sub(campfire_fuel_mg))
                    .unwrap_or_else(|| panic!("wilderness post-fire cache accounting overflowed")),
                "campfire should consume exactly one authored fuel charge from the staged worksite reserve"
            );
            if camp_goal.builds_shelter() {
                let mut uncovered_at_roof_start = state.clone();
                let mut shelter_review = extend_worksite_with_shelter(
                    &registries,
                    &mut state,
                    carried,
                    storage,
                    junction,
                    campfire,
                );
                total_steps = total_steps
                    .checked_add(shelter_review.logistics_legs)
                    .unwrap_or_else(|| panic!("wilderness shelter route count overflowed"));
                camp_logistics_steps = camp_logistics_steps
                    .checked_add(shelter_review.logistics_legs)
                    .unwrap_or_else(|| panic!("wilderness shelter logistics count overflowed"));
                peak_carried_mass_mg =
                    peak_carried_mass_mg.max(shelter_review.peak_carried_mass_mg);
                selected_source_remaining_mg = shelter_review.timber_source_remaining_mg;
                cached_total_mass_mg = state
                    .inventory()
                    .get_stockpile(storage)
                    .map(|record| record.stored_mass().milligrams())
                    .unwrap_or_else(|| {
                        panic!("wilderness long-stay cache disappeared after shelter")
                    });
                // The roof changes exposure, but it does not manufacture fuel. A player intending
                // to stay another hour can choose to live on the roof's thermal reserve instead
                // of reflexively hauling a new fuel charge whenever the fire will expire.
                // Estimate a cautious usable fire window, then add the owner-projected fuel-free
                // comfortable runway at the current observed comfort. This is an actor policy,
                // not a guarantee that an intervening change cannot upset the forecast.
                let fire_energy = state
                    .energy()
                    .get_store(campfire)
                    .unwrap_or_else(|| panic!("long-stay fire disappeared at roof completion"))
                    .stored();
                campfire_runway_before_refuel_ticks = passive_dissipation_ticks_until_empty(
                    &registries,
                    ENERGY_STONE_RING_CAMPFIRE,
                    fire_energy,
                )
                .unwrap_or_else(|error| panic!("long-stay fire runway failed: {error}"))
                .unwrap_or_else(|| panic!("long-stay fire must have finite loss"))
                .value();
                let next_stay_ticks = registries.core().calendar().ticks_per_day() / 24;
                let current_comfort = assess_survival(&registries, &state)
                    .unwrap_or_else(|| panic!("long-stay roof survival state disappeared"))
                    .thermal_comfort_ppm();
                let passive_cover_runway = physiology
                    .thermal_comfort()
                    .ticks_until_unheated_discomfort(
                        current_comfort,
                        deep_hearth::survival::ThermalExposure::OverheadCover,
                    )
                    .value();
                // Do not credit the last fifth of a dwindling charge as fully protective heat.
                let reliable_fire_ticks = campfire_runway_before_refuel_ticks / 5 * 4;
                let covered_stay_is_plausibly_safe =
                    reliable_fire_ticks.saturating_add(passive_cover_runway) > next_stay_ticks;
                if covered_stay_is_plausibly_safe {
                    let stay_end = state
                        .tick()
                        .checked_add_span(TickSpan::new(next_stay_ticks))
                        .unwrap_or_else(|| panic!("roof-protected stay time overflowed"));
                    // A plausible third choice is to use one more kilogram already staged
                    // inside the worksite box rather than build any roof at all. This is a
                    // real material-backed action, not an evaluator-granted extra fire.
                    let mut fuel_only = uncovered_at_roof_start.clone();
                    let timber_stacks = project_material_stacks(&registries, &fuel_only, storage)
                        .unwrap_or_else(|error| panic!("fuel-only cache catalog failed: {error}"));
                    let timber_stack = timber_stacks
                        .iter()
                        .find(|stack| stack.commodity() == timber_commodity)
                        .unwrap_or_else(|| panic!("fuel-only actor has no staged firewood"));
                    validate_pickup_stack_from_ground(
                        &registries,
                        &fuel_only,
                        storage,
                        timber_stack,
                        CAMPFIRE_FUEL_MASS,
                    )
                    .unwrap_or_else(|error| panic!("fuel-only firewood pickup rejected: {error}"))
                    .commit(&mut fuel_only)
                    .unwrap_or_else(|error| panic!("fuel-only firewood pickup failed: {error}"));
                    validate_set_active_selection(
                        &registries,
                        &fuel_only,
                        Some(PlayerActiveSelection::material(timber_commodity)),
                    )
                    .unwrap_or_else(|error| panic!("fuel-only timber selection rejected: {error}"))
                    .commit(&mut fuel_only)
                    .unwrap_or_else(|error| panic!("fuel-only timber selection failed: {error}"));
                    let upward = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
                        .unwrap_or_else(|error| panic!("fuel-only fire direction failed: {error}"));
                    let fueled = advance_control_tick_with_direct_input(
                        &registries,
                        &mut fuel_only,
                        PlayerDirectControlInput::NEUTRAL
                            .with_secondary(SecondaryControlIntent::use_active(upward)),
                    )
                    .unwrap_or_else(|error| {
                        panic!("fuel-only campfire interaction failed: {error}")
                    });
                    assert_eq!(
                        fueled.secondary_action(),
                        Some(&SecondaryActionOutcome::CombustionFuelAdded {
                            store: campfire,
                            released: CAMPFIRE_FUEL_THERMAL_ENERGY,
                        }),
                        "fuel-only alternative must spend real timber on the same fire",
                    );
                    // A matched uncovered counterfactual begins before construction and waits
                    // to exactly the same time. It never changes the actor's selected action.
                    advance_to(
                        &registries,
                        &mut uncovered_at_roof_start,
                        stay_end,
                        TickEventAllowance::default(),
                        "matched exposed stay without roof",
                    );
                    advance_to(
                        &registries,
                        &mut fuel_only,
                        stay_end,
                        TickEventAllowance::default(),
                        "matched exposed stay with another fuel charge",
                    );
                    advance_to(
                        &registries,
                        &mut state,
                        stay_end,
                        TickEventAllowance::default(),
                        "roof-protected hour without fresh firewood",
                    );
                    let protected = assess_survival(&registries, &state)
                        .unwrap_or_else(|| panic!("protected hour lost survival state"));
                    let exposed = assess_survival(&registries, &uncovered_at_roof_start)
                        .unwrap_or_else(|| panic!("exposed hour lost survival state"));
                    let fueled_comfort = assess_survival(&registries, &fuel_only)
                        .unwrap_or_else(|| panic!("fuel-only alternative lost survival state"))
                        .thermal_comfort_ppm();
                    shelter_review.postfire_deferred_refuel = true;
                    shelter_review.postfire_covered_comfort_ppm = protected.thermal_comfort_ppm();
                    shelter_review.postfire_open_comfort_ppm = exposed.thermal_comfort_ppm();
                    shelter_review.postfire_fuel_only_comfort_ppm = fueled_comfort;
                    assert_eq!(
                        protected.thermal_comfort_state(),
                        ThermalComfortState::Comfortable,
                        "roof-protected deferral must preserve comfort for the selected stay"
                    );
                    assert!(
                        exposed.thermal_comfort_ppm() < protected.thermal_comfort_ppm(),
                        "the authored roof must produce better lived warmth than no construction"
                    );
                    assert!(
                        fueled_comfort >= protected.thermal_comfort_ppm(),
                        "an additional real fire charge should remain a strong short-horizon alternative to building a roof"
                    );
                    assert_eq!(
                        calculate_matter_accounting(&fuel_only)
                            .unwrap_or_else(|error| panic!(
                                "fuel-only matter audit failed: {error}"
                            ))
                            .total(),
                        matter_before,
                        "fuel-only branch may spend timber but cannot create or lose matter"
                    );
                    campfire_runway_before_refuel_ticks = passive_dissipation_ticks_until_empty(
                        &registries,
                        ENERGY_STONE_RING_CAMPFIRE,
                        state
                            .energy()
                            .get_store(campfire)
                            .unwrap_or_else(|| panic!("long-stay fire disappeared after deferral"))
                            .stored(),
                    )
                    .unwrap_or_else(|error| panic!("deferred fire runway failed: {error}"))
                    .unwrap_or_else(|| panic!("long-stay fire must have finite dissipation"))
                    .value();
                }
                shelter = Some(shelter_review);
                if campfire_runway_before_refuel_ticks < next_stay_ticks {
                    // Keep the roof-complete, unfueled alternative only as a matched evaluator
                    // branch. It never influences the player's choice or material custody.
                    let mut decline_branch = state.clone();
                    let refuel_started_at = state.tick();
                    // Roof-building already generated conserved chips. Before taking another
                    // tree from the finite known source, inspect actual carried custody and
                    // spend an authored half-kilogram charge when it is available.
                    let chip_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_CHIP);
                    let chip_mass = state
                        .inventory()
                        .get_stockpile(carried)
                        .unwrap_or_else(|| panic!("long-stay fuel inventory disappeared"))
                        .get_mass(chip_commodity);
                    let burn_chips = chip_mass >= CAMPFIRE_CHIP_FUEL_MASS;
                    let (chosen_fuel, fuel_charge, released_energy) = if burn_chips {
                        (
                            chip_commodity,
                            CAMPFIRE_CHIP_FUEL_MASS,
                            CAMPFIRE_CHIP_FUEL_THERMAL_ENERGY,
                        )
                    } else {
                        let timber_source = adjacent_surface_resource(&state, timber_commodity);
                        walk_continuous_body_adjacent_and_settle(
                            &registries,
                            &mut state,
                            timber_source.position(),
                            "long-stay fuel collection outward",
                        );
                        let timber_resource = local_surface_resource(&state, timber_commodity);
                        gather(
                            &registries,
                            &mut state,
                            timber_resource,
                            CAMPFIRE_FUEL_MASS,
                            "long-stay replacement fuel gathering",
                        );
                        selected_source_remaining_mg = state
                            .available_surface_resources()
                            .find(|record| record.id() == timber_resource)
                            .unwrap_or_else(|| panic!("long-stay fuel source disappeared"))
                            .remaining_mass()
                            .milligrams();
                        peak_carried_mass_mg = peak_carried_mass_mg.max(
                            assess_player_carrying(&state)
                                .unwrap_or_else(|| panic!("long-stay fuel load disappeared"))
                                .stored()
                                .milligrams(),
                        );
                        walk_continuous_body_adjacent_and_settle(
                            &registries,
                            &mut state,
                            junction,
                            "long-stay replacement fuel return",
                        );
                        total_steps += 2;
                        camp_logistics_steps += 2;
                        campfire_refuel_retrieval_ticks = state
                            .tick()
                            .checked_duration_since(refuel_started_at)
                            .unwrap_or_else(|| panic!("long-stay fuel retrieval reversed"))
                            .value();
                        (
                            timber_commodity,
                            CAMPFIRE_FUEL_MASS,
                            CAMPFIRE_FUEL_THERMAL_ENERGY,
                        )
                    };
                    validate_set_active_selection(
                        &registries,
                        &state,
                        Some(PlayerActiveSelection::material(chosen_fuel)),
                    )
                    .unwrap_or_else(|error| panic!("long-stay refuel selection failed: {error}"))
                    .commit(&mut state)
                    .unwrap_or_else(|error| {
                        panic!("long-stay refuel selection commit failed: {error}")
                    });
                    let upward = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
                        .unwrap_or_else(|error| panic!("long-stay refuel view failed: {error}"));
                    fire_light_before_refuel =
                        assess_local_illumination(&registries, &state, junction).level();
                    let heat_before_charge = state
                        .energy()
                        .get_store(campfire)
                        .unwrap_or_else(|| panic!("refueled campfire disappeared"))
                        .stored();
                    let refuel = advance_control_tick_with_direct_input(
                        &registries,
                        &mut state,
                        PlayerDirectControlInput::NEUTRAL
                            .with_secondary(SecondaryControlIntent::use_active(upward)),
                    )
                    .unwrap_or_else(|error| panic!("long-stay refuel control failed: {error}"));
                    assert_eq!(
                        refuel.secondary_action(),
                        Some(&SecondaryActionOutcome::CombustionFuelAdded {
                            store: campfire,
                            released: released_energy,
                        }),
                        "selected physical fuel must improve the actual campfire"
                    );
                    assert!(
                        state
                            .energy()
                            .get_store(campfire)
                            .unwrap_or_else(|| panic!("fueled campfire disappeared"))
                            .stored()
                            > heat_before_charge,
                        "the real selected charge must increase heat at the moment of fueling"
                    );
                    assert_eq!(
                        state
                            .inventory()
                            .get_stockpile(carried)
                            .map(|record| record.get_mass(chosen_fuel)),
                        Some(if burn_chips {
                            chip_mass
                                .checked_sub(fuel_charge)
                                .unwrap_or_else(|| panic!("validated chip fuel cannot overdraw"))
                        } else {
                            Mass::ZERO
                        }),
                        "long-stay refueling must consume only the selected physical charge"
                    );
                    fire_light_after_refuel =
                        assess_local_illumination(&registries, &state, junction).level();
                    assert!(
                        fire_light_after_refuel > fire_light_before_refuel,
                        "the chosen physical fuel should restore visible light, not only hidden heat"
                    );
                    assert!(
                        fire_light_after_refuel <= campfire_light_level,
                        "additional fuel must not exceed the authored source's maximum brightness"
                    );
                    campfire_fuel_mg += fuel_charge.milligrams();
                    if burn_chips {
                        campfire_recycled_chips_mg += fuel_charge.milligrams();
                    }
                    campfire_refueled = true;

                    let next_stay_end = state
                        .tick()
                        .checked_add_span(TickSpan::new(next_stay_ticks))
                        .unwrap_or_else(|| panic!("long-stay matched-hour endpoint overflowed"));
                    let mut refueled_branch = state.clone();
                    advance_to(
                        &registries,
                        &mut decline_branch,
                        next_stay_end,
                        TickEventAllowance::default(),
                        "long-stay one hour without new fuel",
                    );
                    advance_to(
                        &registries,
                        &mut refueled_branch,
                        next_stay_end,
                        TickEventAllowance::default(),
                        "long-stay one hour with gathered fuel",
                    );
                    campfire_one_hour_comfort_without_refuel_ppm =
                        assess_survival(&registries, &decline_branch)
                            .unwrap_or_else(|| panic!("unfueled continuation player disappeared"))
                            .thermal_comfort_ppm();
                    campfire_one_hour_comfort_with_refuel_ppm =
                        assess_survival(&registries, &refueled_branch)
                            .unwrap_or_else(|| panic!("fueled continuation player disappeared"))
                            .thermal_comfort_ppm();
                    assert!(
                        campfire_one_hour_comfort_with_refuel_ppm
                            > campfire_one_hour_comfort_without_refuel_ppm,
                        "an actual finite fuel trip must improve the later player's warmth"
                    );
                    assert_eq!(
                        decline_branch.tick(),
                        refueled_branch.tick(),
                        "one-hour refuel benefit must be measured at the same wall-clock time"
                    );
                }
            }
        }
    }

    let expected_steps = 2
        + u64::from(water_visited) * 2
        + match camp_goal {
            WildernessCampGoal::StayMobile => 0,
            WildernessCampGoal::PreserveProvisions => 2,
            WildernessCampGoal::WorksiteCache => 6,
            WildernessCampGoal::LongStayShelter => {
                10 + u64::from(campfire_refueled && campfire_recycled_chips_mg == 0) * 2
            }
        };
    assert_eq!(
        total_steps, expected_steps,
        "opening movement must match chosen worksite and fuel travel: chip-reuse={campfire_recycled_chips_mg}mg camp-legs={camp_logistics_steps}"
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
    match camp_goal {
        WildernessCampGoal::StayMobile => assert!(
            elapsed_microseconds <= 5 * minute,
            "mobile opening should finish promptly once essential provisioning is stable"
        ),
        WildernessCampGoal::WorksiteCache | WildernessCampGoal::PreserveProvisions => {
            let maximum = 30 * minute;
            assert!(
                elapsed_microseconds <= maximum,
                "an elected primitive camp project should be substantial but remain an opening-session investment: elapsed={elapsed_microseconds}us maximum={maximum}us"
            )
        }
        WildernessCampGoal::LongStayShelter => {
            let maximum = 180 * minute;
            assert!(
                elapsed_microseconds <= maximum,
                "roof building and an observed additional fuel-free hour should remain bounded: elapsed={elapsed_microseconds}us maximum={maximum}us"
            )
        }
    }
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
    let mobile_adaptation = if seed == WILDERNESS_WORLD_SEED
        && start_profile == WildernessStartProfile::Rested
        && camp_goal == WildernessCampGoal::StayMobile
    {
        // A short, sensible opening must not be mistaken for a complete survival experience.
        // Continue this same world after the goal-free mobile decision, then respond to the
        // first *observed* change in comfort without an evaluator-supplied construction goal.
        Some(mobile_continuation::evaluate_mobile_cold_response(
            &registries,
            &state,
            junction,
            matter_before,
        ))
    } else {
        None
    };
    let final_survival = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("wilderness player disappeared at final review"));
    let campfire_heat_at_end_nj = campfire_id.map_or(0, |campfire| {
        state
            .energy()
            .get_store(campfire)
            .unwrap_or_else(|| panic!("wilderness campfire disappeared before final review"))
            .stored()
            .nanojoules()
    });
    let campfire_runway_at_end_ticks = if campfire_id.is_some() {
        passive_dissipation_ticks_until_empty(
            &registries,
            ENERGY_STONE_RING_CAMPFIRE,
            Energy::from_nanojoules(campfire_heat_at_end_nj),
        )
        .unwrap_or_else(|error| panic!("final wilderness fire runway projection failed: {error}"))
        .unwrap_or_else(|| panic!("authored fire must have finite dissipation"))
        .value()
    } else {
        0
    };
    if campfire_refueled {
        assert!(
            campfire_heat_at_end_nj > 0,
            "an added material-backed charge must leave measurable fire heat after its use"
        );
    } else {
        assert!(
            campfire_heat_at_end_nj <= campfire_heat_remaining_nj,
            "finite fire must not acquire uncommitted energy without refueling"
        );
    }
    let final_warning_safe = final_survival.metabolic_energy() > physiology.hungry_below()
        && final_survival.hydration() > physiology.thirsty_below();
    assert!(
        final_warning_safe,
        "the first camp should finish with retained provisions and the player still above hunger/thirst warnings"
    );

    WildernessOpeningReview {
        seed: seed.value(),
        start_profile,
        camp_goal,
        food_secured_ticks,
        camp_ticks: elapsed_ticks,
        forage_opportunity_mass_mg: forage_opportunity_mass.milligrams(),
        forage_remaining_mass_mg: forage_remaining_mass.milligrams(),
        forage_rest_runway_ticks,
        gathered_food_mass_mg: forage_mass.milligrams(),
        carried_food_mass_mg: if selected_storage.is_some() {
            0
        } else {
            retained_food_mass.milligrams()
        },
        meal_consumed_mg,
        peak_carried_mass_mg,
        carried_capacity_mg: carrying_after_forage.capacity().milligrams(),
        water_adjacent_known: true,
        water_visited,
        water_source_volume_ul,
        water_remaining_ul,
        water_rest_runway_ticks,
        water_consumed_ul,
        total_legs: total_steps,
        available_route_choices: initial_step_choices,
        max_route_choices: max_step_choices,
        unadvertised_adjacent_probes: 1,
        material_choices,
        selected_storage,
        selected_material,
        storage_project_ticks,
        cached_food_mass_mg,
        cached_total_mass_mg,
        staged_worksite_timber_mg,
        worksite_construction_residue_mg,
        worksite_support_built,
        worksite_support_mass_mg,
        worksite_support_ticks,
        campfire_built,
        campfire_fuel_mg,
        campfire_logistics_legs: campfire_logistics_steps,
        campfire_recovery_ticks,
        campfire_single_charge_runway_ticks,
        thermal_before_fire_ppm,
        thermal_after_fire_ppm,
        campfire_heat_remaining_nj,
        campfire_heat_at_end_nj,
        campfire_runway_at_end_ticks,
        campfire_runway_before_refuel_ticks,
        campfire_refuel_retrieval_ticks,
        campfire_refueled,
        campfire_recycled_chips_mg,
        campfire_one_hour_comfort_with_refuel_ppm,
        campfire_one_hour_comfort_without_refuel_ppm,
        campfire_light_level,
        fire_light_before_refuel,
        fire_light_after_refuel,
        final_thermal_comfort_ppm: final_survival.thermal_comfort_ppm(),
        selected_source_opportunity_mg,
        selected_source_remaining_mg,
        camp_logistics_legs: camp_logistics_steps,
        selected_storage_capacity_mg,
        selected_storage_preservation_ppm,
        project_metabolic_budget_nj: camp_project_budget.metabolic_energy().nanojoules(),
        project_hydration_budget_ul: camp_project_budget.hydration().microliters(),
        project_meal_consumed_mg,
        field_box_build_ticks,
        field_box_capacity_mg: field_box_storage.maximum_stockpile_capacity().milligrams(),
        ambient_food_shelf_ticks,
        field_box_food_shelf_ticks,
        stone_crock_build_ticks,
        stone_crock_capacity_mg: stone_crock_storage
            .maximum_stockpile_capacity()
            .milligrams(),
        stone_crock_food_shelf_ticks,
        field_box_board_batches: board_batches,
        adze_setup_ticks,
        bare_board_ticks,
        adze_board_ticks,
        adze_crossover_batches,
        shelter,
        mobile_adaptation,
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

/// Short causal reading of the exact-local episode; unlike the owner ledger this follows
/// the problems the actor saw, the decisions made, and their experienced consequences.
fn format_megajoules_tenths(nanojoules: u128) -> String {
    let tenths = (nanojoules + 50_000_000_000_000) / 100_000_000_000_000;
    format!("{}.{:01}", tenths / 10, tenths % 10)
}

fn print_player_journey(registries: &Registries, review: WildernessOpeningReview, sample: &str) {
    let seed = review.seed;
    let goal = review.camp_goal.label();
    let episode = format!("{sample}/{}/{goal}", review.start_profile.label());
    std::println!(
        "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} initial={} first-provisions=[{}g berries in {} patch-left:{}g] water=[adjacent:known quantity-before-visit:unknown choice:{} consumed:{}mL]",
        review.start_profile.label(),
        review.gathered_food_mass_mg / 1_000,
        physical_minutes_tenths(registries, review.food_secured_ticks),
        review.forage_remaining_mass_mg / 1_000,
        if review.water_visited {
            "visit"
        } else {
            "skip"
        },
        review.water_consumed_ul / 1_000,
    );
    match review.camp_goal {
        WildernessCampGoal::StayMobile => std::println!(
            "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} junction=[stone,timber] action=leave-unneeded-resources outcome=travel-light-with-food",
        ),
        WildernessCampGoal::WorksiteCache | WildernessCampGoal::LongStayShelter => std::println!(
            "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} junction=[stone,timber] action=haul-timber-build-field-box-and-support outcome=[cached:{}g staged-timber:{}g fabrication-residue:{}g fire:{} charge-runway:{}]",
            review.cached_total_mass_mg / 1_000,
            review.staged_worksite_timber_mg / 1_000,
            review.worksite_construction_residue_mg / 1_000,
            review.campfire_built,
            physical_minutes_tenths(registries, review.campfire_single_charge_runway_ticks),
        ),
        WildernessCampGoal::PreserveProvisions => std::println!(
            "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} junction=[stone,timber] action=haul-stone-build-crock outcome=[cached-food:{}g shelf:{}d vs-field-box:{}d]",
            review.cached_food_mass_mg / 1_000,
            review.stone_crock_food_shelf_ticks / registries.core().calendar().ticks_per_day(),
            review.field_box_food_shelf_ticks / registries.core().calendar().ticks_per_day(),
        ),
    }
    if let Some(shelter) = review.shelter {
        std::println!(
            "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} decision=make-adze-for-roof consequence=[posts:{} cover:{}g attention-saved:{} less-exposure:{}ppm/t]",
            shelter.support_count,
            shelter.panel_mass_mg / 1_000,
            physical_minutes_tenths(registries, shelter.saved_ticks),
            shelter.open_loss_ppm_per_tick - shelter.covered_loss_ppm_per_tick,
        );
        if shelter.postfire_deferred_refuel {
            std::println!(
                "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} discovered-option=roof-can-bridge-fuel-expiry choice=defer-refuel-for-an-hour feedback=[covered:{}ppm unbuilt:{}ppm fuel-only:{}ppm fuel-alternative:1kg-staged-timber]",
                shelter.postfire_covered_comfort_ppm,
                shelter.postfire_open_comfort_ppm,
                shelter.postfire_fuel_only_comfort_ppm,
            );
        }
        let later_comfort = if review.campfire_refueled {
            format!(
                "[with-refuel:{}ppm without-refuel:{}ppm]",
                review.campfire_one_hour_comfort_with_refuel_ppm,
                review.campfire_one_hour_comfort_without_refuel_ppm,
            )
        } else {
            "not-compared-no-refuel-needed".to_owned()
        };
        std::println!(
            "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} new-need=another-hour-at-camp observed-fire-runway:{} choice={} response=[fuel-haul:{} material:{}g recovered-chips:{}g fire-runway-after:{} source-remaining:{}g light-before:{} light-after:{} initial-light:{}] matched-hour-comfort={later_comfort}",
            physical_minutes_tenths(registries, review.campfire_runway_before_refuel_ticks),
            if review.campfire_refueled {
                if review.campfire_recycled_chips_mg > 0 {
                    "burn-existing-woodworking-chips"
                } else {
                    "resupply-from-known-timber"
                }
            } else {
                "defer-enough-fire"
            },
            physical_minutes_tenths(registries, review.campfire_refuel_retrieval_ticks),
            if review.campfire_refueled {
                (review.campfire_fuel_mg - CAMPFIRE_FUEL_MASS.milligrams()) / 1_000
            } else {
                0
            },
            review.campfire_recycled_chips_mg / 1_000,
            physical_minutes_tenths(registries, review.campfire_runway_at_end_ticks),
            review.selected_source_remaining_mg / 1_000,
            review.fire_light_before_refuel,
            review.fire_light_after_refuel,
            review.campfire_light_level,
        );
    }
    let remaining_fire = if review.campfire_built {
        format!(
            " fire=[runway:{} energy-after-warming:{}MJ energy-now:{}MJ]",
            physical_minutes_tenths(registries, review.campfire_runway_at_end_ticks),
            format_megajoules_tenths(review.campfire_heat_remaining_nj),
            format_megajoules_tenths(review.campfire_heat_at_end_nj),
        )
    } else {
        String::new()
    };
    let conclusion = if review.mobile_adaptation.is_some() {
        "opening-snapshot"
    } else {
        "final"
    };
    std::println!(
        "PLAYER JOURNEY seed=0x{seed:016X} episode={episode} goal={goal} {conclusion}=[elapsed:{} peak-load:{}g/{}g warning-safe:{} comfort:{}ppm{remaining_fire}] mode=single-continuous-world",
        physical_minutes_tenths(registries, review.camp_ticks),
        review.peak_carried_mass_mg / 1_000,
        review.carried_capacity_mg / 1_000,
        review.final_warning_safe,
        review.final_thermal_comfort_ppm,
    );
}

pub(super) fn print_wilderness_experience(review: WildernessOpeningReview, sample: &str) {
    let registries = build_registries();
    if sample == "maintained"
        || (sample == "coverage"
            && review.start_profile == WildernessStartProfile::Rested
            && matches!(
                review.camp_goal,
                WildernessCampGoal::WorksiteCache
                    | WildernessCampGoal::LongStayShelter
                    | WildernessCampGoal::PreserveProvisions
            ))
    {
        print_player_journey(&registries, review, sample);
    }
    if let Some(adaptation) = review.mobile_adaptation {
        // Retain the same actor's later, organically triggered decision in episode-filtered
        // replays. The original aggregate adaptation record has no episode key and is omitted
        // by design when the reader asks for one continuous player experience.
        std::println!(
            "PLAYER JOURNEY seed=0x{:016X} episode={sample}/{}/{} phase=observed-chill after:{} response=[reactive-fire:{}t recovered:{}t comfort:{}ppm no-fire:{}ppm] alternative=[prepare-at:{}t comfort:{}ppm remaining-fuel:{}t vs-reactive:{}t] tradeoff=prepare-early-for-warmth-or-react-later-to-save-fuel",
            review.seed,
            review.start_profile.label(),
            review.camp_goal.label(),
            physical_minutes_tenths(&registries, adaptation.exposed_ticks),
            adaptation.first_fueled_tick,
            adaptation.recovery_ticks,
            adaptation.warm_comfort_ppm,
            adaptation.without_fire_comfort_ppm,
            adaptation.planned_start_tick,
            adaptation.planned_comfort_ppm,
            adaptation.planned_fuel_runway_ticks,
            adaptation.reactive_fuel_runway_ticks,
        );
        mobile_continuation::print_mobile_cold_response(&registries, adaptation);
    }
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
    let storage_choice = match review.camp_goal {
        WildernessCampGoal::StayMobile => format!(
            "[selected:defer candidates=[field-box:build:{}t/capacity:{}mg/shelf:{}d stone-crock:build:{}t/capacity:{}mg/shelf:{}d] ambient-shelf:{}d carried-food:{}mg local-surplus-left:{}mg reason:no-current-capacity-or-preservation-pressure]",
            review.field_box_build_ticks,
            review.field_box_capacity_mg,
            review.field_box_food_shelf_ticks / ticks_per_day,
            review.stone_crock_build_ticks,
            review.stone_crock_capacity_mg,
            review.stone_crock_food_shelf_ticks / ticks_per_day,
            review.ambient_food_shelf_ticks / ticks_per_day,
            review.carried_food_mass_mg,
            review.forage_remaining_mass_mg,
        ),
        WildernessCampGoal::WorksiteCache => format!(
            "[selected:field-box project:{}t capacity:{}mg preservation:{}ppm cached-food:{}mg cached-total:{}mg staged-timber:{}mg retained-residue:{}mg grounded-support:{}mg/{}t goal:centralize-worksite source:{}mg->{}mg camp:junction alternative-stone-capacity:{}mg]",
            review.storage_project_ticks,
            review.selected_storage_capacity_mg,
            review.selected_storage_preservation_ppm,
            review.cached_food_mass_mg,
            review.cached_total_mass_mg,
            review.staged_worksite_timber_mg,
            review.worksite_construction_residue_mg,
            review.worksite_support_mass_mg,
            review.worksite_support_ticks,
            review.selected_source_opportunity_mg,
            review.selected_source_remaining_mg,
            review.stone_crock_capacity_mg,
        ),
        WildernessCampGoal::LongStayShelter => {
            let shelter = review
                .shelter
                .unwrap_or_else(|| panic!("long-stay camp report lost shelter evidence"));
            format!(
                "[selected:field-box project:{}t capacity:{}mg preservation:{}ppm cached-food:{}mg cached-total:{}mg staged-timber:{}mg shelter-used:{}mg retained-residue:{}mg grounded-support:{}mg/{}t goal:long-stay-shelter source:{}mg->{}mg camp:junction]",
                review.storage_project_ticks,
                review.selected_storage_capacity_mg,
                review.selected_storage_preservation_ppm,
                review.cached_food_mass_mg,
                review.cached_total_mass_mg,
                review.staged_worksite_timber_mg,
                shelter.staged_timber_used_mg,
                review.worksite_construction_residue_mg,
                review.worksite_support_mass_mg,
                review.worksite_support_ticks,
                review.selected_source_opportunity_mg,
                review.selected_source_remaining_mg,
            )
        }
        WildernessCampGoal::PreserveProvisions => format!(
            "[selected:stone-crock project:{}t capacity:{}mg preservation:{}ppm cached-food:{}mg cached-total:{}mg goal:preserve-provisions source:{}mg->{}mg camp:junction target:8d field-box:{}d stone-crock:{}d]",
            review.storage_project_ticks,
            review.selected_storage_capacity_mg,
            review.selected_storage_preservation_ppm,
            review.cached_food_mass_mg,
            review.cached_total_mass_mg,
            review.selected_source_opportunity_mg,
            review.selected_source_remaining_mg,
            review.field_box_food_shelf_ticks / ticks_per_day,
            review.stone_crock_food_shelf_ticks / ticks_per_day,
        ),
    };
    let tool_choice = match review.camp_goal {
        WildernessCampGoal::WorksiteCache => format!(
            "[selected:defer counterfactual-field-box=[board-batches:{} setup:{}t bare:{}t adze:{}t crossover:{}batches] reason:first-box-below-adze-crossover]",
            review.field_box_board_batches,
            review.adze_setup_ticks,
            review.bare_board_ticks,
            review.adze_board_ticks,
            review.adze_crossover_batches,
        ),
        WildernessCampGoal::LongStayShelter => {
            let shelter = review
                .shelter
                .unwrap_or_else(|| panic!("long-stay tool report lost shelter evidence"));
            format!(
                "[selected:stone-adze roof-work=[board-batches:{} setup:{}t bare:{}t adze:{}t with-setup:{}t saved:{}t crossover:{}batches] reason:roof-workload-repays-tool]",
                shelter.board_batches,
                shelter.setup_ticks,
                shelter.bare_ticks,
                shelter.assisted_ticks,
                shelter.with_setup_ticks,
                shelter.saved_ticks,
                review.adze_crossover_batches,
            )
        }
        WildernessCampGoal::StayMobile | WildernessCampGoal::PreserveProvisions => format!(
            "[selected:defer counterfactual-field-box=[board-batches:{} setup:{}t bare:{}t adze:{}t crossover:{}batches] reason:no-current-woodworking-workload]",
            review.field_box_board_batches,
            review.adze_setup_ticks,
            review.bare_board_ticks,
            review.adze_board_ticks,
            review.adze_crossover_batches,
        ),
    };
    let material_choice = match review.camp_goal {
        WildernessCampGoal::StayMobile => {
            "[stone+timber:simultaneous selected:defer reason:no-current-workload]".to_owned()
        }
        WildernessCampGoal::WorksiteCache => {
            "[stone+timber:simultaneous selected:timber reason:worksite-cache-goal]".to_owned()
        }
        WildernessCampGoal::LongStayShelter => {
            "[stone+timber:simultaneous selected:timber+stone reason:long-stay-shelter-goal]"
                .to_owned()
        }
        WildernessCampGoal::PreserveProvisions => {
            "[stone+timber:simultaneous selected:stone reason:preservation-horizon]".to_owned()
        }
    };
    let selected_storage = match review.selected_storage {
        None => "none",
        Some(STORAGE_ROUGH_TIMBER_FIELD_BOX) => "field-box",
        Some(STORAGE_CARVED_STONE_PROVISIONS_CROCK) => "stone-crock",
        Some(_) => "other",
    };
    let selected_material = match review.selected_material {
        None => "none",
        Some(commodity) if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG) => "timber",
        Some(commodity) if commodity == CommodityKey::new(MATERIAL_STONE, FORM_LUMP) => "stone",
        Some(_) => "other",
    };
    let camp_project = format!(
        "[goal:{} storage:{} material:{} work:{}t logistics-legs:{} source:{}mg->{}mg project-upper-budget:{}nJ/{}uL project-meal:{}mg cached-total:{}mg staged-timber:{}mg retained-residue:{}mg grounded-support:{}:{}mg/{}t]",
        review.camp_goal.label(),
        selected_storage,
        selected_material,
        review.storage_project_ticks,
        review.camp_logistics_legs,
        review.selected_source_opportunity_mg,
        review.selected_source_remaining_mg,
        review.project_metabolic_budget_nj,
        review.project_hydration_budget_ul,
        review.project_meal_consumed_mg,
        review.cached_total_mass_mg,
        review.staged_worksite_timber_mg,
        review.worksite_construction_residue_mg,
        review.worksite_support_built,
        review.worksite_support_mass_mg,
        review.worksite_support_ticks,
    );
    let warmth = if review.campfire_built {
        format!(
            "[fire:true direct-control:true fuel:{}mg logistics-legs:{} recovery:{}t burn-runway:{}t/{} comfort:{}->{}ppm heat-remaining:{}nJ light:{}]",
            review.campfire_fuel_mg,
            review.campfire_logistics_legs,
            review.campfire_recovery_ticks,
            review.campfire_single_charge_runway_ticks,
            physical_minutes_tenths(&registries, review.campfire_single_charge_runway_ticks),
            review.thermal_before_fire_ppm,
            review.thermal_after_fire_ppm,
            review.campfire_heat_remaining_nj,
            review.campfire_light_level,
        )
    } else {
        "[fire:false]".to_owned()
    };
    if let Some(shelter) = review.shelter {
        let tool_name = registries
            .equipment()
            .get_equipment(shelter.tool)
            .map(|definition| definition.name())
            .unwrap_or_else(|| panic!("wilderness shelter tool definition disappeared"));
        std::println!(
            "WILDERNESS SHELTER EXPERIENCE source=fresh-world goal=long-stay selection=player-catalog tool=\"{tool_name}\" travel=continuous/{}legs materials=[stone:{}mg timber:{}mg] shelter=[supports:{}x{}mg panel:{}mg board-work:{}b] attention=[setup:{}t bare:{}t assisted:{}t with-setup:{}t saved:{}t] exposure=[open:{}ppm/t covered:{}ppm/t observed:{}ppm/t] output={}mg wear={} matter-conserved=true continuity=same-opening-state staged-timber-used:{}mg fresh-timber:{}mg postfire=[deferred:{} covered:{}ppm unbuilt:{}ppm fuel-only:{}ppm recycled-chips:{}mg]",
            shelter.logistics_legs,
            shelter.stone_gathered_mg,
            shelter
                .staged_timber_used_mg
                .checked_add(shelter.fresh_timber_gathered_mg)
                .unwrap_or_else(|| panic!("wilderness shelter timber report overflowed")),
            shelter.support_count,
            shelter.support_mass_mg,
            shelter.panel_mass_mg,
            shelter.board_batches,
            shelter.setup_ticks,
            shelter.bare_ticks,
            shelter.assisted_ticks,
            shelter.with_setup_ticks,
            shelter.saved_ticks,
            shelter.open_loss_ppm_per_tick,
            shelter.covered_loss_ppm_per_tick,
            shelter.observed_loss_ppm_per_tick,
            shelter.board_output_mg,
            shelter.tool_worn,
            shelter.staged_timber_used_mg,
            shelter.fresh_timber_gathered_mg,
            shelter.postfire_deferred_refuel,
            shelter.postfire_covered_comfort_ppm,
            shelter.postfire_open_comfort_ppm,
            shelter.postfire_fuel_only_comfort_ppm,
            review.campfire_recycled_chips_mg,
        );
    }
    std::println!(
        "WILDERNESS EXPERIENCE sample={} seed=0x{:016X} episode={} start={} goal={} evidence=generated+exact-local generation=[surface:ordinary water:ordinary] movement=[graph:6-edges body:continuous legs:{} choices:{}-{} graph-unadvertised:{}/1 water-branch:{} material-junction:reached material-choices:{}] timing=[food-secured:{}t/{} stable:{}t/{}] forage=[opportunity:{}mg secured:{}mg left:{}mg rest-runway:{}] provisioning=[carried:{}mg cached:{}mg meal:{} consumed:{}mg warning-safe:{}] carrying=[peak:{}mg capacity:{}mg utilization:{}ppm] water=[observed:{} visited:{} {} consumed:{}uL decision:{}] warmth={} camp-project={} crafting=[storage-via-player-catalog:{}] storage-choice={} tool-choice={} material-choice={}",
        sample,
        review.seed,
        format!(
            "{sample}/{}/{}",
            review.start_profile.label(),
            review.camp_goal.label()
        ),
        review.start_profile.label(),
        review.camp_goal.label(),
        review.total_legs,
        review.available_route_choices,
        review.max_route_choices,
        review.unadvertised_adjacent_probes,
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
        review.cached_food_mass_mg,
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
        warmth,
        camp_project,
        review.camp_goal != WildernessCampGoal::StayMobile,
        storage_choice,
        tool_choice,
        material_choice,
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

fn organic_camp_goal(behavior_root: u64, index: usize) -> WildernessCampGoal {
    match (seed::mix64(behavior_root ^ 0x4341_4D50_474F_414C) as usize).wrapping_add(index) % 3 {
        0 => WildernessCampGoal::StayMobile,
        1 => WildernessCampGoal::WorksiteCache,
        _ => WildernessCampGoal::PreserveProvisions,
    }
}

/// A lived, optional protein route within the same ordinary six-edge opening.
fn exercise_opening_two_food_ration(report: bool) {
    let registries = build_registries();
    let seed = WILDERNESS_WORLD_SEED;
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("two-food generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("two-food generation commit failed: {error}"));
    let carried =
        admit_player_at_hunger_warning_boundary(&registries, &mut state, spawn).carried_stockpile();
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("two-food terrain demand failed: {error}"))
    {
        demand
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("two-food terrain commit failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("two-food initial accounting failed: {error}"))
        .total();
    let berry = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let forage = adjacent_surface_resource(&state, berry);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage.position(),
        "two-food berry approach",
    );
    let berry_resource = local_surface_resource(&state, berry);
    gather(
        &registries,
        &mut state,
        berry_resource,
        Mass::from_milligrams(250_000),
        "two-food berry portion",
    );
    let next_food = available_adjacent_surface_resources(&state)
        .find(|resource| resource.commodity() == nuts)
        .unwrap_or_else(|| panic!("two-food actor cannot see optional local protein"));
    let nut_position = next_food.position();
    let nut_resource = next_food.resource();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        nut_position,
        "two-food junction approach",
    );
    let before_nuts = state
        .available_surface_resources()
        .find(|resource| resource.id() == nut_resource)
        .unwrap_or_else(|| panic!("two-food local nuts disappeared"))
        .remaining_mass();
    let collected = Mass::from_milligrams(250_000);
    gather(
        &registries,
        &mut state,
        nut_resource,
        collected,
        "two-food nut portion",
    );
    let after_nuts = before_nuts
        .checked_sub(collected)
        .unwrap_or_else(|| unreachable!("generated nut source funds its first quarter kilogram"));
    assert_eq!(
        state
            .available_surface_resources()
            .find(|resource| resource.id() == nut_resource)
            .map(|resource| resource.remaining_mass()),
        Some(after_nuts),
        "uncollected protein remains conserved and available at its world source"
    );
    let berries = carried_material_stack(&registries, &state, berry, "two-food berry stack");
    let nuts_stack = carried_material_stack(&registries, &state, nuts, "two-food nut stack");
    assert_eq!(berries.stockpile(), carried);
    assert_eq!(nuts_stack.stockpile(), carried);
    let before_meal = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("two-food pre-meal survival missing"));
    let portion = Mass::from_milligrams(150_000);
    let meal = validate_eat_stack_portions(
        &registries,
        &state,
        &[
            FoodStackPortion::new(&berries, portion),
            FoodStackPortion::new(&nuts_stack, portion),
        ],
    )
    .unwrap_or_else(|error| panic!("two-food mixed stack meal rejected: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("two-food meal commit failed: {error}"));
    let duration = meal.completes_at().value() - state.tick().value();
    let fruit_nutrition = meal.nutrition_offered().get(FoodCategory::Fruit);
    let protein_nutrition = meal.nutrition_offered().get(FoodCategory::Protein);
    assert_eq!(
        (fruit_nutrition, protein_nutrition),
        (300_000, 300_000),
        "equally sized fruit and protein portions must replenish the same amount of dietary reserve despite different calories"
    );
    advance_to(
        &registries,
        &mut state,
        meal.completes_at(),
        TickEventAllowance::default(),
        "two-food meal",
    );
    assert!(
        assess_survival(&registries, &state).is_some_and(|assessment| assessment
            .metabolic_energy()
            > before_meal.metabolic_energy()),
        "the protein-and-fruit meal must actually restore metabolic energy"
    );
    for commodity in [berry, nuts] {
        assert_eq!(
            state
                .inventory()
                .get_stockpile(carried)
                .map(|stock| stock.get_mass(commodity)),
            Some(Mass::from_milligrams(100_000)),
            "both chosen food categories must be partly consumed"
        );
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("two-food completed accounting failed: {error}"))
            .total(),
        matter_before,
        "world, carried, and consumed matter must be conserved"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("two-food final state failed validation: {error}"));
    if report {
        std::println!(
            "WILDERNESS NUTRITION seed=0x{:016X} episode=coverage/hunger-pressure/two-food-ration evidence=generated-exact-local path=forage->junction choice=fruit-and-protein gather=[berries:250000mg nuts:250000mg] meal=[berries:150000mg nuts:150000mg duration:{}t] energy-replenished=true nutrition-offered=[fruit:{fruit_nutrition}ppm protein:{protein_nutrition}ppm] nuts-left={}mg matter=conserved",
            seed.value(),
            duration,
            after_nuts.milligrams(),
        );
    }
}

pub(super) fn run_wilderness_report() {
    kobold_encounter::exercise_kobold_scavenging_encounter(true);
    assert_generated_wilderness_resources_are_directly_usable_with_empty_hands(true);
    exercise_opening_two_food_ration(true);
    clay_progression::exercise_generated_clay_casting_bed(true);
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
        "PROBE INPUT name=wilderness mode=explore samples={} maintained=1 coverage=6 organic={} world_root=0x{world_root:016X} behavior_root=0x{behavior_root:016X} replay=generated-opening-graph",
        ORGANIC_SAMPLES + 7,
        ORGANIC_SAMPLES,
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::StayMobile,
        ),
        "maintained",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HydrationPressure,
            WildernessCampGoal::StayMobile,
        ),
        "coverage",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::WorksiteCache,
        ),
        "coverage",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::LongStayShelter,
        ),
        "coverage",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::PreserveProvisions,
        ),
        "coverage",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HungerPressure,
            WildernessCampGoal::PreserveProvisions,
        ),
        "coverage",
    );
    print_wilderness_experience(
        evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HydrationPressure,
            WildernessCampGoal::WorksiteCache,
        ),
        "coverage",
    );
    for index in 0..ORGANIC_SAMPLES {
        let ordinal = u64::try_from(index + 1)
            .unwrap_or_else(|_| unreachable!("bounded wilderness organic index fits u64"));
        let world_seed = seed::mix64(world_root ^ ordinal.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let profile = organic_start_profile(behavior_root, index);
        let goal = organic_camp_goal(behavior_root, index);
        print_wilderness_experience(
            evaluate_generated_wilderness_opening(
                WorldGenerationSeed::new(world_seed),
                profile,
                goal,
            ),
            "organic",
        );
    }
}

#[test]
fn generated_wilderness_opening_stabilizes_without_premature_investment() {
    let registries = build_registries();
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some() {
        run_wilderness_report();
    } else {
        kobold_encounter::exercise_kobold_scavenging_encounter(false);
        assert_generated_wilderness_resources_are_directly_usable_with_empty_hands(false);
        exercise_opening_two_food_ration(false);
        clay_progression::exercise_generated_clay_casting_bed(false);
        let _ = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::StayMobile,
        );
        let hunger = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HungerPressure,
            WildernessCampGoal::StayMobile,
        );
        assert!(hunger.meal_consumed_mg > 0);
        assert!(hunger.carried_food_mass_mg < hunger.gathered_food_mass_mg);
        assert_eq!(hunger.total_legs, 2);
        assert!(!hunger.water_visited);
        assert!(hunger.final_warning_safe);

        let cache = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::WorksiteCache,
        );
        assert_eq!(cache.selected_storage, Some(STORAGE_ROUGH_TIMBER_FIELD_BOX));
        assert_eq!(cache.staged_worksite_timber_mg, 6_000_000);
        assert!(cache.worksite_support_built);
        assert!(
            (1_000_000..2_000_000).contains(&cache.worksite_support_mass_mg),
            "worksite field-box support should remain a low-mass camp fixture rather than cabin-scale structure"
        );
        assert!(cache.worksite_support_ticks > 0);
        assert_eq!(cache.camp_logistics_legs, 4);
        assert!(cache.campfire_built);
        assert_eq!(cache.campfire_fuel_mg, CAMPFIRE_FUEL_MASS.milligrams());
        assert_eq!(cache.campfire_logistics_legs, 2);
        assert!(cache.campfire_recovery_ticks > 0);
        assert!(cache.campfire_recovery_ticks <= 64);
        assert!(cache.thermal_before_fire_ppm < cache.thermal_after_fire_ppm);
        assert!(cache.thermal_after_fire_ppm >= 800_000);
        assert!(cache.campfire_heat_remaining_nj > 0);
        assert!(cache.campfire_light_level > 0);
        assert!(
            cache.selected_source_opportunity_mg > cache.carried_capacity_mg,
            "the lived worksite timber reserve should exceed one unaided carrying load"
        );
        assert!(cache.selected_source_remaining_mg < cache.selected_source_opportunity_mg);
        assert!(cache.cached_total_mass_mg > cache.stone_crock_capacity_mg);
        assert!(cache.cached_total_mass_mg <= cache.selected_storage_capacity_mg);
        assert!(cache.final_warning_safe);

        let long_stay = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::Rested,
            WildernessCampGoal::LongStayShelter,
        );
        assert_eq!(
            long_stay.selected_storage,
            Some(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        );
        assert!(long_stay.campfire_built);
        assert!(long_stay.campfire_refueled);
        assert!(long_stay.fire_light_after_refuel > long_stay.fire_light_before_refuel);
        assert!(long_stay.fire_light_after_refuel <= long_stay.campfire_light_level);
        let shelter = long_stay
            .shelter
            .unwrap_or_else(|| panic!("long-stay opening lost integrated shelter evidence"));
        assert_eq!(shelter.tool, EQUIPMENT_STONE_WOODWORKING_ADZE);
        assert!(shelter.staged_timber_used_mg > 0);
        assert!(shelter.fresh_timber_gathered_mg > 0);
        assert!(shelter.saved_ticks > 0);
        assert_eq!(shelter.support_count, 3);
        assert_eq!(
            shelter.observed_loss_ppm_per_tick,
            shelter.covered_loss_ppm_per_tick
        );
        assert!(shelter.covered_loss_ppm_per_tick < shelter.open_loss_ppm_per_tick);
        assert!(shelter.tool_worn);
        assert!(
            shelter.postfire_deferred_refuel,
            "the roof should let the actor defer fuel replenishment for the first projected hour"
        );
        assert!(
            shelter.postfire_covered_comfort_ppm
                >= registries
                    .survival()
                    .physiology()
                    .thermal_comfort()
                    .warning_below_ppm()
        );
        assert!(
            shelter.postfire_open_comfort_ppm
                < registries
                    .survival()
                    .physiology()
                    .thermal_comfort()
                    .warning_below_ppm()
        );
        assert!(shelter.postfire_fuel_only_comfort_ppm >= shelter.postfire_covered_comfort_ppm);
        assert_eq!(long_stay.camp_logistics_legs, 8);
        assert!(long_stay.campfire_refueled);
        assert!(
            long_stay.campfire_runway_before_refuel_ticks
                < registries.core().calendar().ticks_per_day() / 24
        );
        assert!(
            long_stay.campfire_runway_at_end_ticks > 0,
            "a recycled half-kilogram charge must provide finite useful warmth"
        );
        assert_eq!(long_stay.campfire_refuel_retrieval_ticks, 0);
        assert_eq!(
            long_stay.campfire_recycled_chips_mg,
            CAMPFIRE_CHIP_FUEL_MASS.milligrams()
        );
        assert_eq!(
            long_stay.selected_source_remaining_mg, shelter.timber_source_remaining_mg,
            "recycling chips must leave the surviving finite timber source untouched"
        );
        assert_eq!(
            long_stay.campfire_fuel_mg,
            CAMPFIRE_FUEL_MASS.milligrams() + CAMPFIRE_CHIP_FUEL_MASS.milligrams()
        );
        assert!(
            long_stay.campfire_one_hour_comfort_with_refuel_ppm
                > long_stay.campfire_one_hour_comfort_without_refuel_ppm
        );
        assert!(long_stay.final_warning_safe);

        // The roof's fuel-deferral advantage is a property of runtime heat and cover,
        // not an accident of the maintained world's one material layout.
        for alternate_seed in [0x59ED_9AC0_0A00_00AB, 0x9E37_79B9_7F4A_7C15] {
            let other = evaluate_generated_wilderness_opening(
                WorldGenerationSeed::new(alternate_seed),
                WildernessStartProfile::Rested,
                WildernessCampGoal::LongStayShelter,
            );
            let shelter = other.shelter.unwrap_or_else(|| {
                panic!("generated seed {alternate_seed:#x} did not complete an ordinary shelter")
            });
            assert!(shelter.postfire_deferred_refuel);
            assert!(shelter.postfire_covered_comfort_ppm > shelter.postfire_open_comfort_ppm);
            assert!(shelter.postfire_fuel_only_comfort_ppm >= shelter.postfire_covered_comfort_ppm);
            assert!(other.campfire_refueled);
            assert_eq!(
                other.campfire_recycled_chips_mg,
                CAMPFIRE_CHIP_FUEL_MASS.milligrams()
            );
            assert!(other.final_warning_safe);
        }

        let preserved = evaluate_generated_wilderness_opening(
            WILDERNESS_WORLD_SEED,
            WildernessStartProfile::HungerPressure,
            WildernessCampGoal::PreserveProvisions,
        );
        assert_eq!(
            preserved.selected_storage,
            Some(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        );
        assert_eq!(preserved.camp_logistics_legs, 2);
        assert!(preserved.selected_source_remaining_mg < preserved.selected_source_opportunity_mg);
        assert!(preserved.stone_crock_food_shelf_ticks > preserved.field_box_food_shelf_ticks);
        assert!(preserved.project_meal_consumed_mg > 0);
        assert!(preserved.final_warning_safe);
    }
}
