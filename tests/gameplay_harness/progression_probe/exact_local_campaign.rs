//! Bounded exact-local replay of primitive progression through first mechanized convergence.
//!
//! The long progression campaign still owns sustained stockpiling, maintenance, and reinvestment
//! evidence. This episode asks a narrower question: does the same generated world and actor-visible
//! evidence still produce the same scarce-copper sequencing and first mechanized convergence when
//! prospecting, tools, mining output, workshop stock, processing equipment, and the first delegated
//! feed trip all have ordinary locality?

use deep_hearth::content::gameplay_fixture::{locate_ground_stockpile, seed_traversable_edge};
use deep_hearth::content::{
    BLOCK_STATE_STONE, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_STONE_GEOLOGICAL_HAMMER, EQUIPMENT_STONE_PICK, FORM_NATIVE_METAL, FORM_ORE,
    MATERIAL_COPPER, MINING_METHOD_HAND_PICK,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::equipment::{
    EquipmentDefinitionId, EquipmentId, player_equipment_assembly_options,
    validate_assemble_player_equipment_option,
};
use deep_hearth::inventory::project_material_stacks;
use deep_hearth::logistics::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerControlInput,
    assess_player_carrying, project_player_adjacent_walk, validate_drop_equipment,
    validate_drop_stack_to_ground, validate_pickup_equipment,
};
use deep_hearth::mining::{
    MiningStartError, MiningTargetRequest, MiningTargetResolutionError, resolve_mining_target,
    validate_start_mining,
};
use deep_hearth::persistence::validate_persistable_state;
use deep_hearth::player::validate_admit_player;
use deep_hearth::production::{ProductionJobId, validate_start_player_process};
use deep_hearth::registry::Registries;
use deep_hearth::simulation::{
    advance_control_tick, advance_control_tick_with_input, advance_tick,
};
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::world::{BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

use super::super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::episode::{ProgressionWorldSetup, build_progression_world_unadmitted};
use super::*;

const WORKSHOP: VoxelCoord = VoxelCoord::new(-1, PROGRESSION_CORRIDOR_Y, 0);
const CORRIDOR_MAX_X: i64 = 6;
const PROGRESSION_TERRAIN_SEED: WorldGenerationSeed = WorldGenerationSeed::new(26);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExactLocalProgressionCampaignReview {
    pub(super) natural_priority: PrimitivePriority,
    pub(super) regional_upper_bounds_ppm: [u32; PROGRESSION_REGIONAL_ZONE_COUNT],
    pub(super) direct_copper_evidence_lower_ppm: u32,
    pub(super) direct_copper_evidence_upper_ppm: u32,
    pub(super) bulk_ore_evidence_lower_ppm: u32,
    pub(super) bulk_ore_evidence_upper_ppm: u32,
    pub(super) hard_ore_evidence_lower_ppm: u32,
    pub(super) hard_ore_evidence_upper_ppm: u32,
    pub(super) bulk_sample_copper_ppm: u32,
    pub(super) information_refinement_required: bool,
    pub(super) refined_sample_copper_ppm: u32,
    pub(super) direct_second_upgrade_blocked: bool,
    pub(super) movement_steps: u64,
    pub(super) haul_trips: u64,
    pub(super) hard_sample_returned: Mass,
    pub(super) selected_processing_feed_copper_ppm: u32,
    pub(super) selected_processing_feed_is_hard: bool,
    pub(super) separation_feed_mass: Mass,
    pub(super) recovered_copper_mass: Mass,
    pub(super) autonomous_overlap_ticks: u64,
    pub(super) autonomous_idle_ticks: u64,
    pub(super) replenishment_returned: Mass,
    pub(super) converged_both_upgrades: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactLocalCrushWork {
    job: ProductionJobId,
    ticks: u64,
    completes_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactLocalAutonomyWork {
    crush_ticks: u64,
    overlap_ticks: u64,
    idle_ticks: u64,
    mining_ticks: u64,
    returned_feed: Mass,
}

fn assert_exact_local_progression_state(registries: &Registries, state: &AppState, context: &str) {
    assert!(
        state.logistics().player().is_some() && state.survival().player().is_some(),
        "exact-local progression {context} requires complete player admission"
    );
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local progression {context} lost carried custody"));
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(carrying.position())
    );
    validate_persistable_state(registries, state).unwrap_or_else(|error| {
        panic!("exact-local progression {context} state is not persistable: {error}")
    });
}

fn locate_workshop_stockpiles(
    state: &mut AppState,
    stockpiles: [deep_hearth::inventory::StockpileId; 8],
) {
    for stockpile in stockpiles {
        locate_ground_stockpile(state, stockpile, WORKSHOP);
    }
}

fn seed_progression_corridor(registries: &Registries, state: &mut AppState) {
    for chunk in [
        ChunkCoord::containing(WORKSHOP),
        ChunkCoord::containing(VoxelCoord::new(
            CORRIDOR_MAX_X,
            PROGRESSION_CORRIDOR_Y,
            WORKSHOP.z(),
        )),
    ] {
        validate_generate_chunk(
            registries,
            state,
            PROGRESSION_TERRAIN_SEED,
            chunk,
            BLOCK_STATE_STONE,
        )
        .unwrap_or_else(|error| {
            panic!("exact-local progression terrain generation failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| panic!("exact-local progression terrain commit failed: {error}"));
    }
    for x in WORKSHOP.x()..CORRIDOR_MAX_X {
        seed_traversable_edge(
            state,
            VoxelCoord::new(x, WORKSHOP.y(), WORKSHOP.z()),
            VoxelCoord::new(x + 1, WORKSHOP.y(), WORKSHOP.z()),
        );
    }
    for x in WORKSHOP.x()..=CORRIDOR_MAX_X {
        assert_eq!(
            state
                .world()
                .block_at(VoxelCoord::new(x, PROGRESSION_CORRIDOR_Y - 1, WORKSHOP.z(),)),
            Some(BLOCK_STATE_STONE),
            "exact-local progression terrain seed must support every corridor voxel"
        );
        assert_eq!(
            state
                .world()
                .block_at(VoxelCoord::new(x, PROGRESSION_CORRIDOR_Y, WORKSHOP.z())),
            Some(BlockStateId::AIR)
        );
        assert_eq!(
            state
                .world()
                .block_at(VoxelCoord::new(x, PROGRESSION_CORRIDOR_Y + 1, WORKSHOP.z(),)),
            Some(BlockStateId::AIR),
            "exact-local progression corridor must retain standing clearance"
        );
    }
}

fn walk_step_with_production(
    registries: &Registries,
    state: &mut AppState,
    destination: VoxelCoord,
    concurrent_production: Option<ProductionJobId>,
) -> u64 {
    assert!(
        state.control_tick().is_simulation_boundary(),
        "exact-local progression body leg must begin at a settled simulation boundary"
    );
    let origin = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("exact-local progression movement lost player"));
    let projection = project_player_adjacent_walk(registries, state, destination)
        .unwrap_or_else(|error| panic!("exact-local progression walk projection failed: {error}"));
    let x = i32::try_from(destination.x() - origin.x())
        .unwrap_or_else(|_| unreachable!("adjacent progression x delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let z = i32::try_from(destination.z() - origin.z())
        .unwrap_or_else(|_| unreachable!("adjacent progression z delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let horizontal = HorizontalControlInput::new(x, z)
        .unwrap_or_else(|error| panic!("exact-local progression movement input failed: {error}"));
    let movement = PlayerControlInput::new(horizontal);
    let started_at = state.tick().value();
    let production_jobs = concurrent_production.as_slice();
    let mut body_ticks = 0_u64;
    while state
        .logistics()
        .player()
        .is_some_and(|player| player.position() != destination)
    {
        let control = advance_control_tick_with_input(registries, state, movement)
            .unwrap_or_else(|error| panic!("exact-local progression movement failed: {error}"));
        body_ticks += 1;
        if let Some(outcome) = control.simulation() {
            assert_tick_events_within(
                outcome,
                TickEventAllowance {
                    production_jobs,
                    ..TickEventAllowance::default()
                },
                "exact-local primitive progression movement",
            );
        }
    }
    while state.logistics().player().is_some_and(|player| {
        let velocity = player.body().horizontal_velocity();
        velocity.x_nanometers_per_second() != 0 || velocity.z_nanometers_per_second() != 0
    }) {
        let control = advance_control_tick(registries, state)
            .unwrap_or_else(|error| panic!("exact-local progression braking failed: {error}"));
        body_ticks += 1;
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(destination),
            "exact-local progression braking overshot its destination"
        );
        if let Some(outcome) = control.simulation() {
            assert_tick_events_within(
                outcome,
                TickEventAllowance {
                    production_jobs,
                    ..TickEventAllowance::default()
                },
                "exact-local primitive progression braking",
            );
        }
    }
    assert_eq!(
        body_ticks,
        projection.control_ticks(),
        "exact-local progression execution diverged from continuous-walk projection"
    );
    while !state.control_tick().is_simulation_boundary() {
        let control = advance_control_tick(registries, state).unwrap_or_else(|error| {
            panic!("exact-local progression movement settlement failed: {error}")
        });
        assert_eq!(
            state.logistics().player().map(|player| player.position()),
            Some(destination),
            "exact-local progression settlement changed locality"
        );
        if let Some(outcome) = control.simulation() {
            assert_tick_events_within(
                outcome,
                TickEventAllowance {
                    production_jobs,
                    ..TickEventAllowance::default()
                },
                "exact-local primitive progression movement settlement",
            );
        }
    }
    state
        .tick()
        .value()
        .checked_sub(started_at)
        .unwrap_or_else(|| unreachable!("player step cannot complete before it starts"))
}

fn walk_step(registries: &Registries, state: &mut AppState, destination: VoxelCoord) {
    let _ = walk_step_with_production(registries, state, destination, None);
}

fn walk_to_x(registries: &Registries, state: &mut AppState, x: i64, steps: &mut u64) {
    loop {
        let current = state
            .logistics()
            .player()
            .map(|player| player.position())
            .unwrap_or_else(|| panic!("exact-local progression movement lost player"));
        if current.x() == x {
            break;
        }
        let next_x = if current.x() < x {
            current.x() + 1
        } else {
            current.x() - 1
        };
        walk_step(
            registries,
            state,
            VoxelCoord::new(next_x, current.y(), current.z()),
        );
        *steps = steps
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local progression movement count overflowed"));
    }
}

fn overlap_with_job(before: u64, elapsed: u64, completes_at: u64) -> u64 {
    if before >= completes_at {
        return 0;
    }
    elapsed.min(completes_at - before)
}

fn walk_to_x_during_production(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
    job: ProductionJobId,
    completes_at: u64,
    steps: &mut u64,
) -> u64 {
    let mut overlap = 0_u64;
    loop {
        let current = state
            .logistics()
            .player()
            .map(|player| player.position())
            .unwrap_or_else(|| panic!("exact-local progression movement lost player"));
        if current.x() == x {
            return overlap;
        }
        let next_x = if current.x() < x {
            current.x() + 1
        } else {
            current.x() - 1
        };
        let before = state.tick().value();
        let elapsed = walk_step_with_production(
            registries,
            state,
            VoxelCoord::new(next_x, current.y(), current.z()),
            Some(job),
        );
        overlap = overlap
            .checked_add(overlap_with_job(before, elapsed, completes_at))
            .unwrap_or_else(|| panic!("exact-local progression overlap duration overflowed"));
        *steps = steps
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local progression movement count overflowed"));
    }
}

fn walk_to_region_during_production(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    job: ProductionJobId,
    completes_at: u64,
    steps: &mut u64,
) -> u64 {
    let overlap = walk_to_x_during_production(
        registries,
        state,
        region.min().x(),
        job,
        completes_at,
        steps,
    );
    assert!(
        region.has_voxel(
            state
                .logistics()
                .player()
                .unwrap_or_else(|| panic!("exact-local progression movement lost player"))
                .position()
        ),
        "exact-local progression concurrent route did not enter requested mining region"
    );
    overlap
}

fn walk_to_region(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    steps: &mut u64,
) {
    let x = region.min().x();
    walk_to_x(registries, state, x, steps);
    assert!(
        region.has_voxel(
            state
                .logistics()
                .player()
                .unwrap_or_else(|| panic!("exact-local progression movement lost player"))
                .position()
        ),
        "exact-local progression route did not enter requested prospecting/mining region"
    );
}

fn drop_all_carried_material(
    registries: &Registries,
    state: &mut AppState,
    destination: deep_hearth::inventory::StockpileId,
) -> Mass {
    let mut dropped = Mass::ZERO;
    loop {
        let carrying = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local progression drop lost carrying state"));
        let mut stacks = project_material_stacks(registries, state, carrying.stockpile())
            .unwrap_or_else(|error| {
                panic!("exact-local progression carried-stack projection failed: {error}")
            });
        let Some(stack) = stacks.pop() else {
            return dropped;
        };
        let mass = stack.mass();
        validate_drop_stack_to_ground(registries, state, destination, &stack, mass)
            .unwrap_or_else(|error| panic!("exact-local progression material drop failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("exact-local progression material drop commit failed: {error}")
            });
        dropped = dropped
            .checked_add(mass)
            .unwrap_or_else(|| panic!("exact-local progression dropped material mass overflowed"));
    }
}

fn extract_and_haul(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    destination: deep_hearth::inventory::StockpileId,
    equipment: EquipmentId,
    requested: Mass,
    batch_limit: Mass,
    steps: &mut u64,
    haul_trips: &mut u64,
) -> (Mass, u64) {
    walk_to_region(registries, state, target.region(), steps);
    let mut remaining = requested;
    let mut total = Mass::ZERO;
    let mut ticks = 0_u64;
    while !remaining.is_zero() {
        let carrying = assess_player_carrying(state)
            .unwrap_or_else(|| panic!("exact-local progression extraction lost carried storage"));
        let batch = remaining.min(batch_limit).min(carrying.available());
        assert!(
            !batch.is_zero(),
            "exact-local progression field kit left no carrying room for extraction"
        );
        let attempt = try_mine_and_claim(
            registries,
            state,
            target,
            carrying.stockpile(),
            equipment,
            batch,
        )
        .unwrap_or_else(|error| panic!("exact-local progression mining failed: {error}"));
        ticks = ticks
            .checked_add(attempt.ticks)
            .unwrap_or_else(|| panic!("exact-local progression mining time overflowed"));
        total = total
            .checked_add(attempt.output)
            .unwrap_or_else(|| panic!("exact-local progression output mass overflowed"));
        remaining = remaining
            .checked_sub(attempt.output)
            .unwrap_or_else(|| unreachable!("mining output cannot exceed requested mass"));
        if attempt.output < batch {
            break;
        }
    }
    walk_to_x(registries, state, WORKSHOP.x(), steps);
    let dropped = drop_all_carried_material(registries, state, destination);
    assert_eq!(
        dropped, total,
        "exact-local progression haul must return exactly the extracted material"
    );
    *haul_trips = haul_trips
        .checked_add(1)
        .unwrap_or_else(|| panic!("exact-local progression haul count overflowed"));
    (total, ticks)
}

fn assert_local_machine(state: &AppState, machine: PrimitiveMachine) {
    for equipment in [machine.crank, machine.crusher, machine.separator] {
        assert_eq!(
            state.logistics().equipment_position(equipment),
            Some(WORKSHOP),
            "exact-local progression processing equipment must remain at the workshop"
        );
        assert!(
            state
                .available_local_equipment()
                .any(|record| record.id() == equipment),
            "exact-local progression processing equipment must be actor-visible at the workshop"
        );
    }
    assert_eq!(
        state.logistics().energy_store_position(machine.drive),
        Some(WORKSHOP),
        "exact-local progression flywheel must remain at the workshop"
    );
    assert!(
        state
            .available_local_energy_stores()
            .any(|store| store.id() == machine.drive),
        "exact-local progression flywheel must be actor-visible at the workshop"
    );
}

fn start_exact_local_crush(
    registries: &Registries,
    state: &mut AppState,
    source: deep_hearth::inventory::StockpileId,
    destination: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    mass: Mass,
    expected_energy: deep_hearth::core::quantity::Energy,
    feed_copper_ppm: u32,
) -> ExactLocalCrushWork {
    assert_eq!(state.logistics().stockpile_position(source), Some(WORKSHOP));
    assert_eq!(
        state.logistics().stockpile_position(destination),
        Some(WORKSHOP)
    );
    assert_local_machine(state, machine);
    let selection = select_observed_copper_grade(
        state,
        source,
        mass,
        feed_copper_ppm,
        "exact-local progression primary crusher feed",
    );
    let resolved = resolve_comminution_process(
        registries,
        state,
        ComminutionRequest::new(
            PROCESS_CRUSH_ORE,
            source,
            selection.as_slice(),
            machine.crusher,
            machine.drive,
        ),
    )
    .unwrap_or_else(|error| panic!("exact-local progression primary crushing failed: {error}"));
    assert_eq!(resolved.required_energy(), expected_energy);
    let ticks = resolved.process_resolution().duration().value();
    let started_at = state.tick().value();
    let job = validate_start_player_process(
        registries,
        state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("exact-local progression primary crush start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("exact-local progression primary crush commit failed: {error}"));
    ExactLocalCrushWork {
        job,
        ticks,
        completes_at: started_at
            .checked_add(ticks)
            .unwrap_or_else(|| panic!("exact-local progression primary crush schedule overflowed")),
    }
}

fn finish_exact_local_crush(
    registries: &Registries,
    state: &mut AppState,
    work: ExactLocalCrushWork,
) -> u64 {
    let mut ticks = 0_u64;
    while state.production().get_job(work.job).is_some() {
        let outcome = advance_tick(registries, state).unwrap_or_else(|error| {
            panic!("exact-local progression autonomous crush failed: {error}")
        });
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[work.job],
                ..TickEventAllowance::default()
            },
            "exact-local primitive progression autonomous crush",
        );
        ticks = ticks
            .checked_add(1)
            .unwrap_or_else(|| panic!("exact-local progression idle crush duration overflowed"));
    }
    ticks
}

fn mine_one_batch_during_production(
    registries: &Registries,
    state: &mut AppState,
    target: MiningTargetRequest,
    equipment: EquipmentId,
    mass: Mass,
    production: ExactLocalCrushWork,
) -> (Mass, u64, u64) {
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| panic!("exact-local progression autonomous mining lost carrying state"));
    assert!(
        carrying.available() >= mass,
        "exact-local progression autonomous feed order must fit the ordinary carrying envelope"
    );
    let resolved = resolve_progression_mining_target(state, target);
    let start = validate_start_mining(
        registries,
        state,
        MINING_METHOD_HAND_PICK,
        resolved,
        carrying.stockpile(),
        equipment,
        mass,
    )
    .unwrap_or_else(|error| {
        panic!("exact-local progression autonomous feed mining failed: {error}")
    });
    let job = start.commit(state).unwrap_or_else(|error| {
        panic!("exact-local progression autonomous mining commit failed: {error}")
    });
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("exact-local progression autonomous mining job disappeared"));
    let mining_ticks = duration(record.started_at().value(), record.completes_at().value());
    let before = state.tick().value();
    let overlap = overlap_with_job(before, mining_ticks, production.completes_at);
    let concurrent = state
        .production()
        .get_job(production.job)
        .is_some()
        .then_some(production.job);
    let (completed_ticks, receipt) = finish_mining_work(
        registries,
        state,
        job,
        concurrent,
        "exact-local progression autonomous feed mining",
    );
    assert_eq!(completed_ticks, mining_ticks);
    (receipt.output().mass(), mining_ticks, overlap)
}

fn run_exact_local_autonomous_replenishment(
    registries: &Registries,
    state: &mut AppState,
    source: deep_hearth::inventory::StockpileId,
    crushed_storage: deep_hearth::inventory::StockpileId,
    machine: PrimitiveMachine,
    feed_copper_ppm: u32,
    bulk_target: MiningTargetRequest,
    ore_storage: deep_hearth::inventory::StockpileId,
    pick: EquipmentId,
    crush_mass: Mass,
    replenish_mass: Mass,
    steps: &mut u64,
    haul_trips: &mut u64,
) -> ExactLocalAutonomyWork {
    let crush = start_exact_local_crush(
        registries,
        state,
        source,
        crushed_storage,
        machine,
        crush_mass,
        machine.required_energy,
        feed_copper_ppm,
    );
    let mut overlap_ticks = walk_to_region_during_production(
        registries,
        state,
        bulk_target.region(),
        crush.job,
        crush.completes_at,
        steps,
    );
    let (mined, mining_ticks, mining_overlap) = mine_one_batch_during_production(
        registries,
        state,
        bulk_target,
        pick,
        replenish_mass,
        crush,
    );
    assert_eq!(
        mined, replenish_mass,
        "exact-local progression autonomous feed trip must satisfy its disclosed replenishment order"
    );
    overlap_ticks = overlap_ticks
        .checked_add(mining_overlap)
        .unwrap_or_else(|| panic!("exact-local progression overlap duration overflowed"));
    overlap_ticks = overlap_ticks
        .checked_add(walk_to_x_during_production(
            registries,
            state,
            WORKSHOP.x(),
            crush.job,
            crush.completes_at,
            steps,
        ))
        .unwrap_or_else(|| panic!("exact-local progression overlap duration overflowed"));
    let idle_ticks = finish_exact_local_crush(registries, state, crush);
    let dropped = drop_all_carried_material(registries, state, ore_storage);
    assert_eq!(dropped, mined);
    *haul_trips = haul_trips
        .checked_add(1)
        .unwrap_or_else(|| panic!("exact-local progression haul count overflowed"));
    assert_eq!(
        overlap_ticks.checked_add(idle_ticks),
        Some(crush.ticks),
        "exact-local crusher window must partition into productive field overlap and post-return idle time"
    );
    assert!(
        overlap_ticks > 0,
        "exact-local progression automation must release some player time for the next feed trip"
    );
    ExactLocalAutonomyWork {
        crush_ticks: crush.ticks,
        overlap_ticks,
        idle_ticks,
        mining_ticks,
        returned_feed: mined,
    }
}

fn assemble_local_field_kit(
    registries: &Registries,
    state: &mut AppState,
    raw: deep_hearth::inventory::StockpileId,
    native: deep_hearth::inventory::StockpileId,
    shaped: deep_hearth::inventory::StockpileId,
) -> (EquipmentId, EquipmentId) {
    let assemble_visible =
        |state: &mut AppState, definition: EquipmentDefinitionId, context: &'static str| {
            let options = player_equipment_assembly_options(registries, state, shaped)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression {context} assembly catalog failed: {error}")
                });
            let matches = options
                .into_iter()
                .filter(|option| option.definition() == definition && option.is_ready())
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "exact-local progression {context} requires one ready actor-visible assembly option"
            );
            validate_assemble_player_equipment_option(registries, state, &matches[0])
                .unwrap_or_else(|error| {
                    panic!("exact-local progression {context} assembly failed: {error}")
                })
                .commit(state)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression {context} assembly commit failed: {error}")
                })
        };
    craft_for_profile(
        registries,
        state,
        raw,
        native,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_PICK),
    );
    let pick = assemble_visible(state, EQUIPMENT_STONE_PICK, "stone-pick");
    craft_for_profile(
        registries,
        state,
        raw,
        native,
        shaped,
        equipment_assembly_profile(registries, EQUIPMENT_STONE_GEOLOGICAL_HAMMER),
    );
    let hammer = assemble_visible(state, EQUIPMENT_STONE_GEOLOGICAL_HAMMER, "sampling-hammer");
    for equipment in [pick, hammer] {
        assert!(
            state
                .available_local_equipment()
                .any(|record| record.id() == equipment),
            "exact-local progression must rediscover newly assembled local field equipment"
        );
        validate_pickup_equipment(registries, state, equipment)
            .unwrap_or_else(|error| {
                panic!("exact-local progression field-equipment pickup failed: {error}")
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!("exact-local progression field-equipment pickup commit failed: {error}")
            });
    }
    (pick, hammer)
}

pub(super) fn run_exact_local_progression_campaign(
    registries: &Registries,
    case: FocusedProbeCase,
) -> ExactLocalProgressionCampaignReview {
    assert!(matches!(
        case.role(),
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay
    ));
    let (low_trace_grade_regime, ore_opportunity_batch_budget) = progression_world_parameters(case);
    let (world, _diagnostics) = build_progression_world_unadmitted(
        registries,
        case.seed(),
        low_trace_grade_regime,
        ore_opportunity_batch_budget,
    );
    let ProgressionWorldSetup {
        mut state,
        mined_mass,
        raw_surplus: _,
        stone_pick_batch_limit,
        stone_hardness_limit,
        reinforced_hardness_limit: _,
        pick_upgrade_native,
        crank_upgrade_native,
        concurrent_soft_mass,
        raw,
        shaped,
        ore_storage,
        hard_ore_storage,
        refined_clue_storage,
        native_storage,
        crushed_storage,
        separation_residue_storage,
        visible_clue_requests,
        refined_clue_sample_mass,
    } = world;
    locate_workshop_stockpiles(
        &mut state,
        [
            raw,
            shaped,
            ore_storage,
            hard_ore_storage,
            refined_clue_storage,
            native_storage,
            crushed_storage,
            separation_residue_storage,
        ],
    );
    seed_progression_corridor(registries, &mut state);
    validate_admit_player(registries, &state, WORKSHOP)
        .unwrap_or_else(|error| panic!("exact-local progression player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("exact-local progression player admission commit failed: {error}")
        });
    assert_exact_local_progression_state(registries, &state, "start");
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("exact-local progression matter audit failed: {error}"))
        .total();
    let mut movement_steps = 0_u64;
    let mut haul_trips = 0_u64;

    for request in visible_clue_requests {
        assert!(matches!(
            resolve_mining_target(&state, request),
            Err(MiningTargetResolutionError::NoEvidence { .. })
        ));
    }

    let regional_zones = std::array::from_fn(progression_regional_bounds);
    for region in regional_zones {
        walk_to_region(registries, &mut state, region, &mut movement_steps);
        acquire_copper_evidence(
            registries,
            &mut state,
            GeologicalEvidenceKind::LooseIndicator,
            region,
        );
    }
    let regional_upper_bounds_ppm = regional_zones.map(|region| {
        let (lower, upper) =
            observed_copper_bounds(&state, MiningTargetRequest::new(region, MATERIAL_COPPER));
        assert_eq!(lower, 0);
        upper
    });
    let mut clue_requests = visible_clue_requests.to_vec();
    clue_requests.sort_by_key(|request| {
        let zone = regional_zone_for_clue(request.region(), &regional_zones);
        (
            Reverse(regional_upper_bounds_ppm[zone]),
            request.region().min(),
            request.region().max_exclusive(),
        )
    });
    for request in clue_requests.iter().copied() {
        walk_to_region(
            registries,
            &mut state,
            request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence(
            registries,
            &mut state,
            GeologicalEvidenceKind::SurfaceExposure,
            request.region(),
        );
    }
    let mut surface_clues = Vec::new();
    let mut refinement = None;
    for request in clue_requests.iter().copied() {
        let (lower_ppm, upper_ppm) = observed_copper_bounds(&state, request);
        match resolve_mining_target(&state, request) {
            Ok(_) => surface_clues.push(ObservedCopperClue {
                request,
                lower_ppm,
                upper_ppm,
            }),
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget {
                material: MATERIAL_COPPER,
                region,
            }) if region == request.region() && lower_ppm == 0 => {
                assert!(refinement.is_none());
                refinement = Some((request, lower_ppm, upper_ppm));
            }
            Err(error) => panic!("exact-local progression surface evidence failed: {error}"),
        }
    }
    let information_refinement_required = refinement.is_some();

    walk_to_x(registries, &mut state, WORKSHOP.x(), &mut movement_steps);
    let (pick, hammer) =
        assemble_local_field_kit(registries, &mut state, raw, native_storage, shaped);
    let mut mineable_clues = Vec::new();
    let mut hardness_blocked_clues = Vec::new();
    for clue in surface_clues.iter().copied() {
        walk_to_region(
            registries,
            &mut state,
            clue.request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence_with_equipment(
            registries,
            &mut state,
            GeologicalEvidenceKind::ExcavationSample,
            clue.request.region(),
            Some(hammer),
        );
        let sampled = observed_resolved_copper_clue(&state, clue.request);
        let hardness = resolve_progression_mining_target(&state, clue.request)
            .excavation_hardness()
            .unwrap_or_else(|| panic!("exact-local progression sample lost hardness"));
        if hardness.upper() <= stone_hardness_limit {
            mineable_clues.push(sampled);
        } else {
            hardness_blocked_clues.push(sampled);
        }
    }
    let hard_clue = strongest_observed_copper_clue(hardness_blocked_clues.iter().copied());
    let direct_copper_clue = strongest_observed_copper_clue(mineable_clues.iter().copied());
    let bulk_ore_clue = strongest_observed_copper_clue(
        mineable_clues
            .iter()
            .copied()
            .filter(|clue| clue.request != direct_copper_clue.request),
    );

    walk_to_region(
        registries,
        &mut state,
        hard_clue.request.region(),
        &mut movement_steps,
    );
    let hard_target = resolve_progression_mining_target(&state, hard_clue.request);
    let carried = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("exact-local progression hard-block test lost carrying state"));
    let hard_upper = hard_target
        .excavation_hardness()
        .unwrap_or_else(|| panic!("exact-local progression hard clue lost hardness"))
        .upper();
    assert_eq!(
        validate_start_mining(
            registries,
            &state,
            MINING_METHOD_HAND_PICK,
            hard_target,
            carried.stockpile(),
            pick,
            mined_mass,
        )
        .err(),
        Some(
            MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
                observed_upper: hard_upper,
                maximum: stone_hardness_limit,
            }
        )
    );

    let (bulk_output, _) = extract_and_haul(
        registries,
        &mut state,
        bulk_ore_clue.request,
        ore_storage,
        pick,
        mined_mass,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    assert_eq!(bulk_output, mined_mass);
    let bulk_sample = observe_material_sample(&state, ore_storage, "exact-local bulk ore");
    assert_eq!(bulk_sample.commodity.form(), FORM_ORE);
    let primitive_sorting = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("exact-local progression separator process disappeared"));
    let soft_separation_feed_mass = primitive_sorting
        .minimum_homogeneous_feed_mass_for_target_recovery(
            crank_upgrade_native,
            bulk_sample.copper_ppm,
        )
        .unwrap_or_else(|| {
            panic!("exact-local bulk ore cannot recover the second reinforcement parcel")
        });
    assert!(soft_separation_feed_mass <= mined_mass);

    let (first_direct, _) = extract_and_haul(
        registries,
        &mut state,
        direct_copper_clue.request,
        native_storage,
        pick,
        pick_upgrade_native,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    assert_eq!(first_direct, pick_upgrade_native);
    let direct_sample =
        observe_material_sample(&state, native_storage, "exact-local direct copper");
    assert_eq!(direct_sample.commodity.form(), FORM_NATIVE_METAL);
    let (second_direct, _) = extract_and_haul(
        registries,
        &mut state,
        direct_copper_clue.request,
        native_storage,
        pick,
        crank_upgrade_native,
        stone_pick_batch_limit,
        &mut movement_steps,
        &mut haul_trips,
    );
    let direct_second_upgrade_blocked = second_direct < crank_upgrade_native;
    assert!(direct_second_upgrade_blocked);

    let refined_sample_copper_ppm = if let Some((request, coarse_lower, coarse_upper)) = refinement
    {
        assert!(matches!(
            resolve_mining_target(&state, request),
            Err(MiningTargetResolutionError::EvidenceInsufficientToResolveTarget { .. })
        ));
        walk_to_region(
            registries,
            &mut state,
            request.region(),
            &mut movement_steps,
        );
        acquire_copper_evidence_with_equipment(
            registries,
            &mut state,
            GeologicalEvidenceKind::ExcavationSample,
            request.region(),
            Some(hammer),
        );
        let refined = observed_resolved_copper_clue(&state, request);
        assert!(refined.lower_ppm > coarse_lower && refined.upper_ppm < coarse_upper);
        let (sampled, _) = extract_and_haul(
            registries,
            &mut state,
            request,
            refined_clue_storage,
            pick,
            refined_clue_sample_mass,
            stone_pick_batch_limit,
            &mut movement_steps,
            &mut haul_trips,
        );
        assert_eq!(sampled, refined_clue_sample_mass);
        let sample =
            observe_material_sample(&state, refined_clue_storage, "exact-local refined clue");
        assert_eq!(sample.commodity.form(), FORM_ORE);
        assert!(bulk_sample.copper_ppm > sample.copper_ppm);
        sample.copper_ppm
    } else {
        0
    };

    walk_to_x(registries, &mut state, WORKSHOP.x(), &mut movement_steps);
    let natural_priority = observed_primitive_priority(hard_clue, bulk_sample);
    let (
        mut machine,
        mut hard_sample_returned,
        first_investment_handling,
        selected_processing_source,
        selected_processing_feed_copper_ppm,
        selected_processing_feed_is_hard,
        selected_separation_feed_mass,
    ) = match natural_priority {
        PrimitivePriority::PickFirst => {
            validate_drop_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression pick drop before reinforcement failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!(
                        "exact-local progression pick drop before reinforcement commit failed: {error}"
                    )
                });
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            assert!(
                state.available_local_equipment().any(|record| {
                    record.id() == pick && record.definition() == EQUIPMENT_COPPER_REINFORCED_PICK
                }),
                "exact-local progression must rediscover the locally reinforced pick before reuse"
            );
            validate_pickup_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression reinforced-pick pickup failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression reinforced-pick pickup commit failed: {error}")
                });
            let (returned, _) = extract_and_haul(
                registries,
                &mut state,
                hard_clue.request,
                hard_ore_storage,
                pick,
                mined_mass,
                reinforced_pick_mining_batch_limit(registries),
                &mut movement_steps,
                &mut haul_trips,
            );
            assert_eq!(returned, mined_mass);
            let hard_sample =
                observe_material_sample(&state, hard_ore_storage, "exact-local hard-seam ore");
            assert_eq!(hard_sample.commodity.form(), FORM_ORE);
            let hard_feed_is_better = hard_sample.copper_ppm > bulk_sample.copper_ppm;
            let (source, feed_ppm, separation_feed_mass) = if hard_feed_is_better {
                (
                    hard_ore_storage,
                    hard_sample.copper_ppm,
                    primitive_sorting
                        .minimum_homogeneous_feed_mass_for_target_recovery(
                            crank_upgrade_native,
                            hard_sample.copper_ppm,
                        )
                        .unwrap_or_else(|| {
                            panic!(
                                "exact-local hard-seam ore cannot recover the second reinforcement parcel"
                            )
                        }),
                )
            } else {
                (
                    ore_storage,
                    bulk_sample.copper_ppm,
                    soft_separation_feed_mass,
                )
            };
            let base_machine = build_primitive_machine(
                registries,
                &mut state,
                PrimitiveMachineBuildPlan {
                    raw,
                    native_storage,
                    shaped,
                    mined_mass,
                    separation_feed_mass,
                    seed: case.seed(),
                },
            );
            assert_local_machine(&state, base_machine);
            let machine = charge_primitive_machine(registries, &mut state, base_machine);
            assert_local_machine(&state, machine);
            (
                machine,
                returned,
                "drop-modify-repickup-pick",
                source,
                feed_ppm,
                hard_feed_is_better,
                separation_feed_mass,
            )
        }
        PrimitivePriority::CrankFirst => {
            let base_machine = build_primitive_machine(
                registries,
                &mut state,
                PrimitiveMachineBuildPlan {
                    raw,
                    native_storage,
                    shaped,
                    mined_mass,
                    separation_feed_mass: soft_separation_feed_mass,
                    seed: case.seed(),
                },
            );
            assert_local_machine(&state, base_machine);
            reinforce_crank(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                base_machine.crank,
            );
            let machine = PrimitiveMachine {
                crank_reinforced: true,
                ..base_machine
            };
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(machine.crank)
                    .map(|record| record.definition()),
                Some(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK)
            );
            assert_local_machine(&state, machine);
            let machine = charge_primitive_machine(registries, &mut state, machine);
            assert_local_machine(&state, machine);
            (
                machine,
                Mass::ZERO,
                "stationary-local-machine-crank",
                ore_storage,
                bulk_sample.copper_ppm,
                false,
                soft_separation_feed_mass,
            )
        }
    };
    assert!(
        machine.player_catalog_assembly,
        "exact-local progression mechanization must assemble actor-visible equipment through the player catalog"
    );

    let autonomy = run_exact_local_autonomous_replenishment(
        registries,
        &mut state,
        selected_processing_source,
        crushed_storage,
        machine,
        selected_processing_feed_copper_ppm,
        bulk_ore_clue.request,
        ore_storage,
        pick,
        mined_mass,
        concurrent_soft_mass,
        &mut movement_steps,
        &mut haul_trips,
    );
    assert_local_machine(&state, machine);
    let separation = separate_native_copper(
        registries,
        &mut state,
        PrimitiveSeparationPlan {
            crushed_storage,
            native_storage,
            residue_storage: separation_residue_storage,
            machine,
            feed_mass: selected_separation_feed_mass,
            feed_copper_ppm: selected_processing_feed_copper_ppm,
            expected_target: crank_upgrade_native,
        },
    );
    assert_eq!(separation.target_mass, crank_upgrade_native);

    let second_investment_handling = match natural_priority {
        PrimitivePriority::PickFirst => {
            reinforce_crank(
                registries,
                &mut state,
                raw,
                native_storage,
                shaped,
                machine.crank,
            );
            machine = PrimitiveMachine {
                crank_reinforced: true,
                ..machine
            };
            assert_eq!(
                state
                    .equipment()
                    .get_equipment(machine.crank)
                    .map(|record| record.definition()),
                Some(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK)
            );
            "stationary-local-machine-crank"
        }
        PrimitivePriority::CrankFirst => {
            validate_drop_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression second pick drop failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression second pick drop commit failed: {error}")
                });
            reinforce_pick(registries, &mut state, raw, native_storage, shaped, pick);
            assert!(
                state.available_local_equipment().any(|record| {
                    record.id() == pick && record.definition() == EQUIPMENT_COPPER_REINFORCED_PICK
                }),
                "exact-local progression must rediscover the second reinforced pick"
            );
            validate_pickup_equipment(registries, &state, pick)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression second reinforced-pick pickup failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| {
                    panic!("exact-local progression second reinforced-pick pickup commit failed: {error}")
                });
            let (returned, _) = extract_and_haul(
                registries,
                &mut state,
                hard_clue.request,
                hard_ore_storage,
                pick,
                mined_mass,
                reinforced_pick_mining_batch_limit(registries),
                &mut movement_steps,
                &mut haul_trips,
            );
            assert_eq!(returned, mined_mass);
            hard_sample_returned = returned;
            let hard_sample = observe_material_sample(
                &state,
                hard_ore_storage,
                "exact-local crank-first hard-seam ore",
            );
            assert_eq!(hard_sample.commodity.form(), FORM_ORE);
            "drop-modify-repickup-pick"
        }
    };

    let converged_both_upgrades = state
        .equipment()
        .get_equipment(pick)
        .is_some_and(|record| record.definition() == EQUIPMENT_COPPER_REINFORCED_PICK)
        && state
            .equipment()
            .get_equipment(machine.crank)
            .is_some_and(|record| record.definition() == EQUIPMENT_COPPER_REINFORCED_HAND_CRANK)
        && hard_sample_returned == mined_mass;
    assert!(
        converged_both_upgrades,
        "exact-local progression must converge on both scarce-copper upgrades and return a hard-seam sample"
    );
    assert_local_machine(&state, machine);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "exact-local progression final matter audit failed: {error}"
            ))
            .total(),
        matter_before,
        "exact-local progression must conserve matter through travel, processing, and convergence"
    );
    assert_exact_local_progression_state(registries, &state, "completed campaign");

    reviewln!(
        "PROGRESSION LOCAL CAMPAIGN seed=0x{:016X} sample={} evidence=ordinary-exact-local generation=matched-progression-world corridor=[body:continuous-generated-terrain workshop:-1 clues:0,2,4,6 max-distance:7 edges:{}] information=[regional:{}..{}ppm surface:{} resolved:{} refinement:{} hard:{}..{}ppm bulk:{}..{}ppm direct:{}..{}ppm] field-kit=[assembled-after-admission:true catalog:true rediscovered:true carried:true] extraction=[bulk:{}mg direct-first:{}mg direct-second:{}mg shortage:{} haul-trips:{}] decision=[selected:{} rule:hard-lower>owned-bulk bulk-grade:{}ppm] first-investment=[local:true handling:{}] mechanization=[line-local:true catalog:true feed:{} grade:{}ppm prep:{}t charge:{}t crush:{}t separation:{}t recovered:{}mg] autonomy=[next-feed:{}mg mining:{}t overlap:{}t idle:{}t] second-investment=[local:true handling:{}] convergence=[both-upgrades:{} hard-sample-returned:{}mg] matter=conserved",
        case.seed(),
        case.role().label(),
        movement_steps,
        regional_upper_bounds_ppm[0],
        regional_upper_bounds_ppm[1],
        clue_requests.len(),
        surface_clues.len(),
        information_refinement_required,
        hard_clue.lower_ppm,
        hard_clue.upper_ppm,
        bulk_ore_clue.lower_ppm,
        bulk_ore_clue.upper_ppm,
        direct_copper_clue.lower_ppm,
        direct_copper_clue.upper_ppm,
        bulk_output.milligrams(),
        first_direct.milligrams(),
        second_direct.milligrams(),
        direct_second_upgrade_blocked,
        haul_trips,
        natural_priority.label(),
        bulk_sample.copper_ppm,
        first_investment_handling,
        if selected_processing_feed_is_hard {
            "hard-sample"
        } else {
            "owned-bulk"
        },
        selected_processing_feed_copper_ppm,
        machine.processing_line_preparation_ticks,
        machine.charge_ticks,
        autonomy.crush_ticks,
        separation.ticks,
        separation.target_mass.milligrams(),
        autonomy.returned_feed.milligrams(),
        autonomy.mining_ticks,
        autonomy.overlap_ticks,
        autonomy.idle_ticks,
        second_investment_handling,
        converged_both_upgrades,
        hard_sample_returned.milligrams(),
    );

    ExactLocalProgressionCampaignReview {
        natural_priority,
        regional_upper_bounds_ppm,
        direct_copper_evidence_lower_ppm: direct_copper_clue.lower_ppm,
        direct_copper_evidence_upper_ppm: direct_copper_clue.upper_ppm,
        bulk_ore_evidence_lower_ppm: bulk_ore_clue.lower_ppm,
        bulk_ore_evidence_upper_ppm: bulk_ore_clue.upper_ppm,
        hard_ore_evidence_lower_ppm: hard_clue.lower_ppm,
        hard_ore_evidence_upper_ppm: hard_clue.upper_ppm,
        bulk_sample_copper_ppm: bulk_sample.copper_ppm,
        information_refinement_required,
        refined_sample_copper_ppm,
        direct_second_upgrade_blocked,
        movement_steps,
        haul_trips,
        hard_sample_returned,
        selected_processing_feed_copper_ppm,
        selected_processing_feed_is_hard,
        separation_feed_mass: selected_separation_feed_mass,
        recovered_copper_mass: separation.target_mass,
        autonomous_overlap_ticks: autonomy.overlap_ticks,
        autonomous_idle_ticks: autonomy.idle_ticks,
        replenishment_returned: autonomy.returned_feed,
        converged_both_upgrades,
    }
}
