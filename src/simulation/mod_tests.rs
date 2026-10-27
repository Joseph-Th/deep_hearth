//! Contract tests for canonical simulation-tick orchestration.

use super::*;
use crate::content::{
    BLOCK_STATE_STONE, FORM_LOG, MATERIAL_WOOD, build_registries,
    make_test_registries_with_energy_store,
};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::apply_clock_advance;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId,
    add_energy_store_with_initial_for_fixture,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    HorizontalControlInput, PlayerControlInput, PlayerOverheadCoverAssessment, PlayerStance,
    PlayerWalkProjectionError, assess_player_carrying, assess_player_overhead_cover,
    project_player_adjacent_walk, project_player_walk_path, project_player_walk_path_for_load,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::registry::Registries;
use crate::spatial::{
    NANOMETERS_PER_VOXEL, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord,
};
use crate::survival::{
    SurvivalAssessment, Vitality, initialize_player_survival, player_record,
    player_record_with_thermal_comfort,
};
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk, validate_generate_player_neighborhood,
};

#[path = "mod_tests/direct_gathering.rs"]
mod direct_gathering;
#[path = "mod_tests/overhead_cover.rs"]
mod overhead_cover;
#[path = "mod_tests/structural_placement.rs"]
mod structural_placement;
#[path = "mod_tests/structural_targeting.rs"]
mod structural_targeting;

const DISSIPATIVE_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(510_001);

#[test]
fn direct_interactions_reject_missing_and_dead_player_before_context_resolution() {
    let registries = build_registries();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("direct interaction view is invalid: {error}"));
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction))
        .with_primary(PrimaryControlIntent::break_target(direction))
        .with_drop_active_selection();

    let mut absent = AppState::new();
    let absent_before = absent.clone();
    let absent_action = advance_control_tick_with_direct_input(&registries, &mut absent, input)
        .unwrap_or_else(|error| panic!("unadmitted contextual-use control failed: {error}"));
    assert_eq!(
        absent_action.secondary_action(),
        Some(&SecondaryActionOutcome::PlayerNotAdmitted)
    );
    assert_eq!(
        absent_action.primary_action(),
        Some(&PrimaryActionOutcome::PlayerNotAdmitted)
    );
    assert_eq!(
        absent_action.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::PlayerNotAdmitted)
    );
    assert_eq!(absent.control_tick(), absent_action.control_tick());
    assert!(absent.logistics().player().is_none());
    assert_eq!(absent.inventory(), absent_before.inventory());
    assert_eq!(absent.survival(), absent_before.survival());
    assert_eq!(absent.player_work(), absent_before.player_work());

    let mut dead = AppState::new();
    let _ = validate_admit_player(&registries, &dead, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("dead contextual-use admission failed: {error}"))
        .commit(&mut dead)
        .unwrap_or_else(|error| panic!("dead contextual-use admission commit failed: {error}"));
    let before = dead
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("dead contextual-use player missing"));
    let revision = dead.survival().revision();
    dead.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::ZERO,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let dead_before = dead.clone();
    let dead_action = advance_control_tick_with_direct_input(&registries, &mut dead, input)
        .unwrap_or_else(|error| panic!("dead contextual-use control failed: {error}"));
    assert_eq!(
        dead_action.secondary_action(),
        Some(&SecondaryActionOutcome::PlayerDead)
    );
    assert_eq!(
        dead_action.primary_action(),
        Some(&PrimaryActionOutcome::PlayerDead)
    );
    assert_eq!(
        dead_action.active_selection_drop(),
        Some(&ActiveSelectionDropOutcome::PlayerDead)
    );
    assert_eq!(dead.player_work().active(), None);
    assert!(!dead.logistics().has_pending_direct_action());
    assert_eq!(dead.inventory(), dead_before.inventory());
    assert_eq!(dead.survival(), dead_before.survival());
    assert_eq!(dead.player_work(), dead_before.player_work());
}

#[test]
fn fatal_simulation_boundary_clears_buffered_direct_actions_before_state_validation() {
    use crate::logistics::{
        validate_buffer_player_primary_break, validate_buffer_player_secondary_use,
    };
    use crate::structural::StructuralElementId;

    let registries = build_registries();
    let direction = ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("fatal-buffer direction invalid: {error}"));
    for primary in [false, true] {
        let mut state = AppState::new();
        let _ = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
            .unwrap_or_else(|error| panic!("fatal-buffer admission rejected: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("fatal-buffer admission commit failed: {error}"));
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal-buffer off-boundary setup failed: {error}"));
        if primary {
            validate_buffer_player_primary_break(&state, StructuralElementId::new(1), direction)
                .unwrap_or_else(|error| panic!("primary buffering failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("primary buffering commit failed: {error}"));
        } else {
            validate_buffer_player_secondary_use(&state, direction)
                .unwrap_or_else(|error| panic!("secondary buffering failed: {error}"))
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("secondary buffering commit failed: {error}"));
        }
        let before = state
            .survival()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("fatal-buffer survival player missing"));
        let revision = state.survival().revision();
        state.survival_state_mut().apply_player(
            revision,
            revision + 1,
            player_record_with_thermal_comfort(
                Energy::ZERO,
                before.hydration(),
                Vitality::from_parts_per_million_unchecked(
                    registries
                        .survival()
                        .physiology()
                        .starvation_vitality_loss_ppm_per_tick(),
                ),
                before.nutrition(),
                before.vitality_recovery_remainder(),
                before.thermal_comfort_ppm(),
            ),
        );
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );

        let mut boundary = None;
        for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
            boundary = Some(
                advance_control_tick(&registries, &mut state)
                    .unwrap_or_else(|error| panic!("fatal-buffer control failed: {error}")),
            );
        }
        let boundary = boundary.unwrap_or_else(|| panic!("fatal-buffer boundary not reached"));
        assert!(boundary.simulation().is_some());
        assert_eq!(
            state.survival().player().map(|player| player.vitality()),
            Some(Vitality::ZERO)
        );
        assert!(!state.logistics().has_pending_direct_action());
        if primary {
            assert_eq!(
                boundary.primary_action(),
                Some(&PrimaryActionOutcome::PlayerDead)
            );
            assert_eq!(boundary.secondary_action(), None);
        } else {
            assert_eq!(
                boundary.secondary_action(),
                Some(&SecondaryActionOutcome::PlayerDead)
            );
            assert_eq!(boundary.primary_action(), None);
        }
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("fatal-buffer serialization failed: {error}"));
        let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("fatal-buffer decoding failed: {error}"));
        assert_eq!(
            loaded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("fatal-buffer trusted load failed: {error}")),
            state
        );
    }
}

fn passive_dissipation_registries() -> Registries {
    make_test_registries_with_energy_store(
        EnergyStoreDefinition::new_with_transfer_limits(
            DISSIPATIVE_STORE,
            "simulation passive dissipation fixture",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(10_000_000_000_000_000),
            Power::from_microwatts(1_000_000_000_000),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_microwatts(1_000_000_000_000)),
    )
}

#[test]
fn local_warmth_aggregation_caps_before_energy_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX - 10);
    let almost_maximum = Energy::from_nanojoules(u128::MAX - 20);

    let first = accumulate_relevant_local_warmth(Energy::ZERO, almost_maximum, maximum);
    assert_eq!(first, almost_maximum);
    assert_eq!(
        accumulate_relevant_local_warmth(first, Energy::from_nanojoules(100), maximum),
        maximum
    );
    assert_eq!(
        accumulate_relevant_local_warmth(maximum, Energy::from_nanojoules(u128::MAX), maximum),
        maximum
    );
}

#[test]
fn walk_path_projection_prices_future_load_without_mutating_custody() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let path = (1..=4)
        .map(|x| VoxelCoord::new(x, floor_y, 0))
        .collect::<Vec<_>>();
    for destination in &path {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(destination.x(), floor_y - 1, 0),
            BLOCK_STATE_STONE,
        );
        for y in floor_y..=floor_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(destination.x(), y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let carrying = assess_player_carrying(&state)
        .unwrap_or_else(|| panic!("future-haul projection lost carrying assessment"));
    assert_eq!(carrying.stored(), Mass::ZERO);
    let unloaded = project_player_walk_path(&registries, &state, &path)
        .unwrap_or_else(|error| panic!("unloaded walk projection failed: {error}"));
    let future_load = carrying.capacity();
    let loaded = project_player_walk_path_for_load(
        &registries,
        &state,
        &path,
        future_load,
        carrying.capacity(),
    )
    .unwrap_or_else(|error| panic!("future loaded walk projection failed: {error}"));
    // Stop-and-go routes include neutral braking: the slower pack can sometimes stop in fewer
    // control samples. Sustained travel is checked independently below.
    assert!(unloaded.control_ticks() > 0 && loaded.control_ticks() > 0);
    assert!(
        loaded.resource_budget().metabolic_energy() > unloaded.resource_budget().metabolic_energy()
    );
    assert!(loaded.resource_budget().hydration() > unloaded.resource_budget().hydration());
    assert_eq!(
        assess_player_carrying(&state)
            .unwrap_or_else(|| panic!("future-haul projection lost carrying after read"))
            .stored(),
        Mass::ZERO,
        "future-haul projection must not fabricate carried custody"
    );
    let over_capacity = carrying
        .capacity()
        .checked_add(Mass::from_milligrams(1))
        .unwrap_or_else(|| panic!("future-haul over-capacity fixture overflowed"));
    assert_eq!(
        project_player_walk_path_for_load(
            &registries,
            &state,
            &path,
            over_capacity,
            carrying.capacity(),
        ),
        Err(PlayerWalkProjectionError::ProjectedLoadExceedsCapacity {
            carried: over_capacity,
            capacity: carrying.capacity(),
        })
    );
}

#[test]
fn sustained_haul_walk_takes_more_control_time_than_empty_handed_travel() {
    let (registries, mut empty, floor_y) = generated_grounded_player_at(0, 0);
    for x in 1..=7 {
        set_generated_block_for_test(
            registries.world(),
            &mut empty,
            VoxelCoord::new(x, floor_y - 1, 0),
            BLOCK_STATE_STONE,
        );
        for y in floor_y..=floor_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut empty,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let mut loaded = empty.clone();
    let carried = loaded
        .logistics()
        .player()
        .unwrap_or_else(|| panic!("haul test lost admitted player"))
        .carried_stockpile();
    let capacity = assess_player_carrying(&loaded)
        .unwrap_or_else(|| panic!("haul test lost carrying assessment"))
        .capacity();
    deposit_lot_for_test(
        &registries,
        &mut loaded,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        capacity,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("haul test pack filling failed: {error}"));
    let movement = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("haul test control input invalid: {error}")),
    );
    let travel_ticks = |state: &mut AppState| {
        let mut ticks = 0_u64;
        while state
            .logistics()
            .player()
            .is_some_and(|player| player.position().x() < 6)
        {
            let _ = advance_control_tick_with_input(&registries, state, movement)
                .unwrap_or_else(|error| panic!("haul travel failed: {error}"));
            ticks += 1;
            assert!(
                ticks < 70,
                "haul test route did not finish before next slow tick"
            );
        }
        ticks
    };
    let empty_ticks = travel_ticks(&mut empty);
    let full_ticks = travel_ticks(&mut loaded);
    assert!(
        full_ticks > empty_ticks,
        "carrying a full pack must visibly delay sustained travel: full {full_ticks} vs empty {empty_ticks}"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &loaded),
        Ok(())
    );
}

#[test]
fn adjacent_walk_projection_matches_control_execution_and_boundary_cost() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let path = [
        VoxelCoord::new(1, floor_y, 0),
        VoxelCoord::new(2, floor_y, 0),
    ];
    for destination in path {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(destination.x(), floor_y - 1, destination.z()),
            BLOCK_STATE_STONE,
        );
        for y in floor_y..=floor_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(destination.x(), y, destination.z()),
                BlockStateId::AIR,
            );
        }
    }
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walk projection player disappeared"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(5_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("walk projection load fixture failed: {error}"));
    let projection = project_player_walk_path(&registries, &state, &path)
        .unwrap_or_else(|error| panic!("walk path projection failed: {error}"));
    assert_eq!(projection.origin(), VoxelCoord::new(0, floor_y, 0));
    assert_eq!(projection.destination(), path[1]);
    assert!(projection.control_ticks() > 0);
    assert!(projection.control_ticks() < CONTROL_TICKS_PER_SIMULATION_TICK);
    assert!(projection.locomotion_exertion().energy_cost_per_tick() > Energy::ZERO);
    let survival_before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walk projection survival player disappeared"));
    let mut actual_control_ticks = 0_u64;
    let mut passive_braking_translation_ticks = 0_u64;
    for destination in path {
        let movement = PlayerControlInput::new(
            HorizontalControlInput::new(1_000_000, 0)
                .unwrap_or_else(|error| panic!("walk projection execution input failed: {error}")),
        );
        while state
            .logistics()
            .player()
            .is_some_and(|player| player.position() != destination)
        {
            let _ = advance_control_tick_with_input(&registries, &mut state, movement)
                .unwrap_or_else(|error| panic!("walk projection execution failed: {error}"));
            actual_control_ticks += 1;
        }
        while state.logistics().player().is_some_and(|player| {
            let velocity = player.body().horizontal_velocity();
            velocity.x_nanometers_per_second() != 0 || velocity.z_nanometers_per_second() != 0
        }) {
            let before = state
                .logistics()
                .player()
                .unwrap_or_else(|| panic!("braking player disappeared"))
                .body()
                .foot_position();
            let _ = advance_control_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("walk projection braking failed: {error}"));
            actual_control_ticks += 1;
            let after = state
                .logistics()
                .player()
                .unwrap_or_else(|| panic!("braking player disappeared"))
                .body()
                .foot_position();
            passive_braking_translation_ticks += u64::from(before != after);
            assert_eq!(
                state.logistics().player().map(|player| player.position()),
                Some(destination)
            );
        }
    }
    assert!(
        passive_braking_translation_ticks > 0,
        "test route must exercise passive braking displacement without charged locomotion"
    );
    assert_eq!(actual_control_ticks, projection.control_ticks());
    while state.tick() == SimulationTick::new(0) {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("walk projection boundary settlement failed: {error}"));
    }
    let survival_after = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walk projection survival result disappeared"));
    assert_eq!(
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy()),
        Some(projection.resource_budget().metabolic_energy())
    );
    assert_eq!(
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration()),
        Some(projection.resource_budget().hydration())
    );
}

#[test]
fn dead_player_walk_projection_reports_player_state_before_route_physics() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("dead-walk player missing"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::ZERO,
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let unchanged = state.clone();
    assert_eq!(
        project_player_walk_path(&registries, &state, &[VoxelCoord::new(1, floor_y, 0)]),
        Err(PlayerWalkProjectionError::PlayerDead)
    );
    assert_eq!(state, unchanged);
}

#[test]
fn adjacent_walk_projection_rejects_a_horizontal_request_that_would_drop() {
    let (registries, state, floor_y) = generated_grounded_player_at(0, 0);
    let destination = VoxelCoord::new(1, floor_y, 0);
    assert!(matches!(
        project_player_adjacent_walk(&registries, &state, destination),
        Err(PlayerWalkProjectionError::DestinationOvershot {
            destination: rejected,
            actual,
        }) if rejected == destination && actual.y() < floor_y
    ));
}

#[test]
fn grounded_stationary_generated_body_can_fast_forward_one_slow_tick() {
    let (registries, mut state, _floor_y) = generated_grounded_player_at(0, 0);
    let body_before = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("stationary fast-forward player disappeared"))
        .body();

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stationary generated-body fast-forward failed: {error}"));

    assert_eq!(outcome.tick(), SimulationTick::new(1));
    assert_eq!(
        state.control_tick(),
        ControlTick::new(CONTROL_TICKS_PER_SIMULATION_TICK)
    );
    assert_eq!(
        state
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("stationary fast-forward player disappeared after tick"))
            .body(),
        body_before
    );
}

#[test]
fn generated_body_fast_forward_rejects_motion_and_crouch() {
    let (registries, mut moving, _floor_y) = generated_grounded_player_at(0, 0);
    let movement = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("fast-forward motion input failed: {error}")),
    );
    let _ = advance_control_tick_with_input(&registries, &mut moving, movement)
        .unwrap_or_else(|error| panic!("fast-forward motion setup failed: {error}"));
    let before_moving_fast_forward = moving.clone();
    assert_eq!(
        advance_tick(&registries, &mut moving),
        Err(TickError::ControlSteppingRequired {
            current: ControlTick::new(1),
        })
    );
    assert_eq!(moving, before_moving_fast_forward);

    let (_, mut crouching, _floor_y) = generated_grounded_player_at(0, 0);
    let _ = advance_control_tick_with_input(
        &registries,
        &mut crouching,
        PlayerControlInput::NEUTRAL.with_crouch(true),
    )
    .unwrap_or_else(|error| panic!("fast-forward crouch setup failed: {error}"));
    assert_eq!(
        crouching
            .logistics()
            .player()
            .map(|player| player.body().stance()),
        Some(PlayerStance::Crouching)
    );
    let before_crouched_fast_forward = crouching.clone();
    assert_eq!(
        advance_tick(&registries, &mut crouching),
        Err(TickError::ControlSteppingRequired {
            current: ControlTick::new(1),
        })
    );
    assert_eq!(crouching, before_crouched_fast_forward);
}

#[test]
fn generated_neighborhood_demand_removes_the_missing_chunk_movement_wall() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(15, 0);
    let demand = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("movement-demand validation failed: {error}"))
        .unwrap_or_else(|| panic!("movement-demand fixture had no missing chunks"));
    demand
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("movement-demand commit failed: {error}"));

    for x in 15..=18 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, floor_y - 1, 0),
            BLOCK_STATE_STONE,
        );
        for y in floor_y..=floor_y + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("movement-demand input fixture failed: {error}")),
    );
    for _ in 0..8 {
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("movement-demand control tick failed: {error}"));
    }
    let body = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("movement-demand player disappeared"))
        .body();
    assert!(body.foot_position().x_nanometers() > 16 * NANOMETERS_PER_VOXEL);
    assert!(body.voxel_position().x() >= 16);
}

#[test]
fn sprint_and_crouch_use_distinct_authored_speeds_and_persist_stance() {
    let (registries, mut sprinting, _) = generated_grounded_player_at(0, 0);
    let (_, mut crouching, _) = generated_grounded_player_at(0, 0);
    let (_, mut loaded_sprinting, _) = generated_grounded_player_at(0, 0);
    let carried = loaded_sprinting
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded sprint player disappeared"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        &registries,
        &mut loaded_sprinting,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        registries.survival().physiology().carried_mass_capacity(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("loaded sprint carried matter failed: {error}"));
    let horizontal = HorizontalControlInput::new(1_000_000, 0)
        .unwrap_or_else(|error| panic!("locomotion mode input failed: {error}"));
    let sprint = PlayerControlInput::new(horizontal).with_sprint(true);
    let crouch = PlayerControlInput::new(horizontal)
        .with_sprint(true)
        .with_crouch(true);

    for _ in 0..6 {
        let _ = advance_control_tick_with_input(&registries, &mut sprinting, sprint)
            .unwrap_or_else(|error| panic!("sprint control tick failed: {error}"));
        let _ = advance_control_tick_with_input(&registries, &mut loaded_sprinting, sprint)
            .unwrap_or_else(|error| panic!("loaded sprint control tick failed: {error}"));
    }
    for _ in 0..2 {
        let _ = advance_control_tick_with_input(&registries, &mut crouching, crouch)
            .unwrap_or_else(|error| panic!("crouch control tick failed: {error}"));
    }

    let sprinting_body = sprinting
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("sprint player disappeared"))
        .body();
    let loaded_body = loaded_sprinting
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded sprint player disappeared after motion"))
        .body();
    let crouching_body = crouching
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("crouch player disappeared"))
        .body();
    assert_eq!(
        sprinting_body
            .horizontal_velocity()
            .x_nanometers_per_second(),
        5_600_000_000
    );
    assert_eq!(
        loaded_body.horizontal_velocity().x_nanometers_per_second(),
        4_300_000_000
    );
    assert_eq!(crouching_body.stance(), PlayerStance::Crouching);
    assert_eq!(
        crouching_body
            .horizontal_velocity()
            .x_nanometers_per_second(),
        1_300_000_000
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &crouching))
        .unwrap_or_else(|error| panic!("crouched body serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("crouched body decode failed: {error}"));
    assert_eq!(
        decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("crouched body load failed: {error}")),
        crouching
    );
}

#[test]
fn continuous_walking_settles_higher_survival_cost_under_carried_load() {
    let (registries, mut unloaded, _) = generated_grounded_player_at(0, 0);
    let mut loaded = unloaded.clone();
    let carried = loaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking player disappeared"))
        .carried_stockpile();
    let load = Mass::from_milligrams(12_500_000);
    let _ = deposit_lot_for_test(
        &registries,
        &mut loaded,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        load,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("loaded walking material fixture failed: {error}"));
    let before_unloaded = unloaded
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("unloaded walking survival player disappeared"));
    let before_loaded = loaded
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking survival player disappeared"));
    let walk = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("load-sensitive walking input failed: {error}")),
    );

    for control in 1..=CONTROL_TICKS_PER_SIMULATION_TICK {
        let input = if control <= 10 {
            walk
        } else {
            PlayerControlInput::NEUTRAL
        };
        let _ = advance_control_tick_with_input(&registries, &mut unloaded, input)
            .unwrap_or_else(|error| panic!("unloaded walking control tick failed: {error}"));
        let _ = advance_control_tick_with_input(&registries, &mut loaded, input)
            .unwrap_or_else(|error| panic!("loaded walking control tick failed: {error}"));
    }

    let unloaded_body = unloaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("unloaded walking body disappeared"))
        .body();
    let loaded_body = loaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking body disappeared"))
        .body();
    assert!(
        loaded_body.foot_position().x_nanometers() < unloaded_body.foot_position().x_nanometers(),
        "a pack should reduce actual travel distance under identical held walking input"
    );
    let after_unloaded = unloaded
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("unloaded walking survival result disappeared"));
    let after_loaded = loaded
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking survival result disappeared"));
    let unloaded_energy = before_unloaded.metabolic_energy().nanojoules()
        - after_unloaded.metabolic_energy().nanojoules();
    let loaded_energy = before_loaded.metabolic_energy().nanojoules()
        - after_loaded.metabolic_energy().nanojoules();
    let unloaded_hydration =
        before_unloaded.hydration().microliters() - after_unloaded.hydration().microliters();
    let loaded_hydration =
        before_loaded.hydration().microliters() - after_loaded.hydration().microliters();
    assert!(loaded_energy > unloaded_energy);
    assert!(loaded_hydration > unloaded_hydration);
}

#[test]
fn grounded_jump_is_load_sensitive_and_does_not_retrigger_in_air() {
    let (registries, mut unloaded, floor_y) = generated_grounded_player_at(0, 0);
    let mut loaded = unloaded.clone();
    let carried = loaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded jump player disappeared"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        &registries,
        &mut loaded,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        registries.survival().physiology().carried_mass_capacity(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("loaded jump carried matter failed: {error}"));
    let jump = PlayerControlInput::NEUTRAL.with_jump(true);

    let first_unloaded = advance_control_tick_with_input(&registries, &mut unloaded, jump)
        .unwrap_or_else(|error| panic!("unloaded jump failed: {error}"))
        .body_motion()
        .unwrap_or_else(|| panic!("unloaded jump produced no body motion"));
    let first_loaded = advance_control_tick_with_input(&registries, &mut loaded, jump)
        .unwrap_or_else(|error| panic!("loaded jump failed: {error}"))
        .body_motion()
        .unwrap_or_else(|| panic!("loaded jump produced no body motion"));
    let unloaded_jump_exertion = unloaded
        .logistics()
        .player()
        .map(|player| player.locomotion_exertion().pending())
        .unwrap_or_else(|| panic!("unloaded jumping player disappeared"));
    let loaded_jump_exertion = loaded
        .logistics()
        .player()
        .map(|player| player.locomotion_exertion().pending())
        .unwrap_or_else(|| panic!("loaded jumping player disappeared"));
    let physiology = registries.survival().physiology();
    let locomotion = physiology.locomotion();
    let capacity = physiology.carried_mass_capacity();
    assert_eq!(
        unloaded_jump_exertion,
        locomotion.jump_exertion_for_load(Mass::ZERO, capacity),
        "stationary jump launch must settle its authored one-shot exertion without control-rate dilution"
    );
    assert_eq!(
        loaded_jump_exertion,
        locomotion.jump_exertion_for_load(capacity, capacity),
        "jump launch must preserve carried-load pressure"
    );
    assert!(
        first_unloaded
            .after()
            .vertical_velocity()
            .nanometers_per_second()
            > first_loaded
                .after()
                .vertical_velocity()
                .nanometers_per_second()
    );
    assert!(
        first_loaded
            .after()
            .vertical_velocity()
            .nanometers_per_second()
            > 0
    );
    assert!(first_unloaded.after().foot_position().y_nanometers() > floor_y * NANOMETERS_PER_VOXEL);

    let second = advance_control_tick_with_input(&registries, &mut unloaded, jump)
        .unwrap_or_else(|error| panic!("airborne held-jump tick failed: {error}"))
        .body_motion()
        .unwrap_or_else(|| panic!("airborne held-jump produced no motion"));
    assert_eq!(
        unloaded
            .logistics()
            .player()
            .map(|player| player.locomotion_exertion().pending()),
        Some(unloaded_jump_exertion),
        "holding jump while airborne must not repeatedly charge launch exertion"
    );
    assert!(
        second.after().vertical_velocity().nanometers_per_second()
            < first_unloaded
                .after()
                .vertical_velocity()
                .nanometers_per_second()
    );

    for _ in 0..40 {
        let outcome = advance_control_tick(&registries, &mut unloaded)
            .unwrap_or_else(|error| panic!("jump landing tick failed: {error}"));
        let body = unloaded
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("jumping player disappeared before landing"))
            .body();
        if body.foot_position().y_nanometers() == floor_y * NANOMETERS_PER_VOXEL
            && body.vertical_velocity().nanometers_per_second() == 0
        {
            let impact = outcome
                .body_motion()
                .and_then(|motion| motion.fall_impact())
                .unwrap_or_else(|| panic!("ordinary jump landing produced no impact receipt"));
            assert!(
                impact.impact_speed()
                    <= registries
                        .survival()
                        .physiology()
                        .locomotion()
                        .safe_landing_speed()
            );
            assert_eq!(impact.vitality_loss_ppm(), 0);
            assert_eq!(
                unloaded
                    .logistics()
                    .player()
                    .map(|player| player.pending_fall_vitality_loss_ppm()),
                Some(0)
            );
            return;
        }
    }
    panic!("jump did not return to exact floor contact within bounded control ticks");
}

#[test]
fn repeated_loaded_jumps_remain_persistable_before_slow_settlement() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("repeated-jump player disappeared"))
        .carried_stockpile();
    let capacity = registries.survival().physiology().carried_mass_capacity();
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        capacity,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("repeated-jump carried load failed: {error}"));
    let jump = PlayerControlInput::NEUTRAL.with_jump(true);
    let mut launches = 0_u32;

    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let body = state
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("repeated-jump player disappeared during motion"))
            .body();
        let grounded = body.foot_position().y_nanometers() == floor_y * NANOMETERS_PER_VOXEL
            && body.vertical_velocity().nanometers_per_second() == 0;
        let input = if grounded {
            launches += 1;
            jump
        } else {
            PlayerControlInput::NEUTRAL
        };
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("repeated-jump control tick failed: {error}"));
        if launches < 2 {
            continue;
        }

        assert!(
            state.control_tick().value() < CONTROL_TICKS_PER_SIMULATION_TICK,
            "two ordinary jumps must fit before the first slow settlement for this contract"
        );
        let pending = state
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("repeated-jump player disappeared before persistence"))
            .locomotion_exertion()
            .pending();
        let old_single_interval_cap = registries
            .survival()
            .physiology()
            .locomotion()
            .maximum_exertion();
        assert!(
            pending.energy_cost_per_tick() > old_single_interval_cap.energy_cost_per_tick(),
            "two fully loaded jump impulses should exceed the obsolete single-interval persistence cap"
        );

        let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
            .unwrap_or_else(|error| panic!("repeated-jump serialization failed: {error}"));
        let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
            .unwrap_or_else(|error| panic!("repeated-jump decode failed: {error}"));
        assert_eq!(
            decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("repeated-jump trusted load failed: {error}")),
            state
        );
        return;
    }
    panic!("fully loaded player did not launch twice before the first slow settlement");
}

#[test]
fn crouching_guards_ledges_and_standing_motion_falls() {
    let (registries, mut crouching, floor_y) = generated_grounded_player_at(0, 0);
    let mut standing = crouching.clone();
    for x in 1..=6 {
        set_generated_block_for_test(
            registries.world(),
            &mut crouching,
            VoxelCoord::new(x, floor_y - 1, 0),
            BlockStateId::AIR,
        );
        set_generated_block_for_test(
            registries.world(),
            &mut standing,
            VoxelCoord::new(x, floor_y - 1, 0),
            BlockStateId::AIR,
        );
    }
    let horizontal = HorizontalControlInput::new(1_000_000, 0)
        .unwrap_or_else(|error| panic!("ledge input fixture failed: {error}"));
    let crouch = PlayerControlInput::new(horizontal).with_crouch(true);
    let walk = PlayerControlInput::new(horizontal);

    for _ in 0..20 {
        let _ = advance_control_tick_with_input(&registries, &mut crouching, crouch)
            .unwrap_or_else(|error| panic!("crouched ledge tick failed: {error}"));
        let _ = advance_control_tick_with_input(&registries, &mut standing, walk)
            .unwrap_or_else(|error| panic!("standing ledge tick failed: {error}"));
    }
    let crouched_body = crouching
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("crouched ledge player disappeared"))
        .body();
    let standing_body = standing
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("standing ledge player disappeared"))
        .body();
    assert_eq!(crouched_body.stance(), PlayerStance::Crouching);
    assert_eq!(
        crouched_body.foot_position().y_nanometers(),
        floor_y * NANOMETERS_PER_VOXEL
    );
    assert!(standing_body.foot_position().y_nanometers() < floor_y * NANOMETERS_PER_VOXEL);
}

#[test]
fn sprinting_settles_more_locomotion_exertion_than_walking_at_equal_load() {
    let (registries, mut walking, _) = generated_grounded_player_at(0, 0);
    let mut sprinting = walking.clone();
    let horizontal = HorizontalControlInput::new(1_000_000, 0)
        .unwrap_or_else(|error| panic!("sprint-cost input fixture failed: {error}"));
    let walk = PlayerControlInput::new(horizontal);
    let sprint = walk.with_sprint(true);

    for _ in 0..6 {
        let _ = advance_control_tick_with_input(&registries, &mut walking, walk)
            .unwrap_or_else(|error| panic!("sprint-cost walking tick failed: {error}"));
        let _ = advance_control_tick_with_input(&registries, &mut sprinting, sprint)
            .unwrap_or_else(|error| panic!("sprint-cost sprinting tick failed: {error}"));
    }
    let walking_pending = walking
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("sprint-cost walking player disappeared"))
        .locomotion_exertion()
        .pending();
    let sprinting_pending = sprinting
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("sprint-cost sprinting player disappeared"))
        .locomotion_exertion()
        .pending();
    assert!(sprinting_pending.energy_cost_per_tick() > walking_pending.energy_cost_per_tick());
    assert!(
        sprinting_pending.hydration_loss_per_tick() > walking_pending.hydration_loss_per_tick()
    );

    for _ in 6..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut walking)
            .unwrap_or_else(|error| panic!("sprint-cost walking settle tick failed: {error}"));
        let _ = advance_control_tick(&registries, &mut sprinting)
            .unwrap_or_else(|error| panic!("sprint-cost sprinting settle tick failed: {error}"));
    }
    let walking_survival = walking
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("sprint-cost walking survival disappeared"));
    let sprinting_survival = sprinting
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("sprint-cost sprinting survival disappeared"));
    assert!(sprinting_survival.metabolic_energy() < walking_survival.metabolic_energy());
    assert!(sprinting_survival.hydration() < walking_survival.hydration());
}

#[test]
fn crouched_body_refuses_to_stand_into_a_low_ceiling() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(0, floor_y + 2, 0),
        BLOCK_STATE_STONE,
    );
    let crouch_jump = PlayerControlInput::NEUTRAL
        .with_crouch(true)
        .with_jump(true);
    let first = advance_control_tick_with_input(&registries, &mut state, crouch_jump)
        .unwrap_or_else(|error| panic!("crouched jump failed: {error}"))
        .body_motion()
        .unwrap_or_else(|| panic!("crouched jump produced no motion"));
    assert_eq!(first.after().stance(), PlayerStance::Crouching);

    let second = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("blocked stand tick failed: {error}"))
        .body_motion()
        .unwrap_or_else(|| panic!("blocked stand tick produced no motion"));
    assert_eq!(second.after().stance(), PlayerStance::Crouching);
}

fn generated_grounded_player_at(x: i64, z: i64) -> (Registries, AppState, i64) {
    let registries = build_registries();
    let mut state = AppState::new();
    let coord = ChunkCoord::containing(VoxelCoord::new(x, 0, z));
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0D1_0001),
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("horizontal-motion terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("horizontal-motion terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(x, *y, z)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("horizontal-motion terrain column has no solid surface"));
    let floor_y = surface_y + 1;
    validate_admit_player(&registries, &state, VoxelCoord::new(x, floor_y, z))
        .unwrap_or_else(|error| panic!("horizontal-motion player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("horizontal-motion player commit failed: {error}"));
    (registries, state, floor_y)
}

#[test]
fn opaque_overhead_cover_reduces_thermal_exposure_without_warming() {
    let (registries, mut open, floor_y) = generated_grounded_player_at(0, 0);
    let mut covered = open.clone();
    let roof = VoxelCoord::new(0, floor_y + 2, 0);
    set_generated_block_for_test(registries.world(), &mut covered, roof, BLOCK_STATE_STONE);
    assert_eq!(
        assess_player_overhead_cover(&registries, &open),
        Some(PlayerOverheadCoverAssessment::Open)
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &covered),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    let before = open
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("overhead-cover survival player disappeared"))
        .thermal_comfort_ppm();
    let _ = advance_tick(&registries, &mut open)
        .unwrap_or_else(|error| panic!("open thermal exposure tick failed: {error}"));
    let _ = advance_tick(&registries, &mut covered)
        .unwrap_or_else(|error| panic!("covered thermal exposure tick failed: {error}"));
    let open_after = open
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("open thermal exposure player disappeared"))
        .thermal_comfort_ppm();
    let covered_after = covered
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("covered thermal exposure player disappeared"))
        .thermal_comfort_ppm();
    let thermal = registries.survival().physiology().thermal_comfort();
    assert_eq!(before - open_after, thermal.unsheltered_loss_ppm_per_tick());
    assert_eq!(
        before - covered_after,
        thermal.overhead_cover_loss_ppm_per_tick()
    );
    assert!(
        covered_after < before,
        "overhead cover must not create heat"
    );
    assert!(covered_after > open_after);
}

#[test]
fn overhead_cover_can_be_above_standing_headroom_without_an_immediate_ceiling() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let raised_roof = VoxelCoord::new(0, floor_y + 4, 0);
    let neighboring_roof = VoxelCoord::new(1, floor_y + 4, 0);
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open)
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        neighboring_roof,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "a neighboring canopy cannot protect the player's actual body footprint"
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        raised_roof,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Covered),
        "a raised solid roof still blocks vertical exposure to the sky"
    );
    let before = state.survival().player().unwrap().thermal_comfort_ppm();
    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(
        before - state.survival().player().unwrap().thermal_comfort_ppm(),
        registries
            .survival()
            .physiology()
            .thermal_comfort()
            .overhead_cover_loss_ppm_per_tick()
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        raised_roof,
        BlockStateId::AIR,
    );
    assert_eq!(
        assess_player_overhead_cover(&registries, &state),
        Some(PlayerOverheadCoverAssessment::Open)
    );
}

fn generated_airborne_player(clearance_voxels: i64) -> (Registries, AppState, i64) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0D1_0001),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("body-motion terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("body-motion terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("body-motion terrain column has no solid surface"));
    let floor_y = surface_y + 1;
    let spawn_y = floor_y
        .checked_add(clearance_voxels)
        .unwrap_or_else(|| panic!("body-motion spawn height overflowed"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, spawn_y, 0))
        .unwrap_or_else(|error| panic!("body-motion player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("body-motion player commit failed: {error}"));
    (registries, state, floor_y)
}

#[test]
fn direct_horizontal_control_accelerates_persists_and_brakes_on_ground() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("horizontal input fixture failed: {error}")),
    );
    let before = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("horizontal-motion player disappeared"))
        .body();

    let first = advance_control_tick_with_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("horizontal control tick failed: {error}"));
    let motion = first
        .body_motion()
        .unwrap_or_else(|| panic!("horizontal control produced no body motion"));
    assert_eq!(
        motion
            .after()
            .horizontal_velocity()
            .x_nanometers_per_second(),
        1_000_000_000
    );
    assert_eq!(
        motion.after().foot_position().x_nanometers(),
        before.foot_position().x_nanometers() + 50_000_000
    );
    assert_eq!(
        motion.after().foot_position().y_nanometers(),
        floor_y * NANOMETERS_PER_VOXEL
    );
    assert!(motion.grounded());

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("horizontal body serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("horizontal body decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("horizontal body trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let braking = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("horizontal braking tick failed: {error}"));
    let after = braking
        .body_motion()
        .unwrap_or_else(|| panic!("horizontal braking produced no body change"))
        .after();
    assert_eq!(after.horizontal_velocity().x_nanometers_per_second(), 0);
    assert_eq!(
        after.foot_position().x_nanometers(),
        motion.after().foot_position().x_nanometers()
    );
}

#[test]
fn horizontal_walking_settles_speed_and_load_scaled_exertion_on_slow_boundary() {
    let (registries, mut moving, _) = generated_grounded_player_at(0, 0);
    let mut resting = moving.clone();
    let mut loaded = moving.clone();
    let carried = loaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking-cost player disappeared"))
        .carried_stockpile();
    let _ = deposit_lot_for_test(
        &registries,
        &mut loaded,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        registries.survival().physiology().carried_mass_capacity(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("loaded walking-cost carried matter failed: {error}"));
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("walking-cost input fixture failed: {error}")),
    );

    for _ in 0..2 {
        let _ = advance_control_tick_with_input(&registries, &mut moving, input)
            .unwrap_or_else(|error| panic!("walking-cost active control tick failed: {error}"));
        let _ = advance_control_tick_with_input(&registries, &mut loaded, input)
            .unwrap_or_else(|error| panic!("loaded walking-cost active tick failed: {error}"));
    }
    let pending = moving
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walking-cost player disappeared"))
        .locomotion_exertion()
        .pending();
    assert!(!pending.energy_cost_per_tick().is_zero());
    assert!(!pending.hydration_loss_per_tick().is_zero());
    let loaded_pending = loaded
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("loaded walking-cost player disappeared after motion"))
        .locomotion_exertion()
        .pending();
    assert!(loaded_pending.energy_cost_per_tick() > pending.energy_cost_per_tick());
    assert!(loaded_pending.hydration_loss_per_tick() > pending.hydration_loss_per_tick());

    for _ in 2..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut moving)
            .unwrap_or_else(|error| panic!("walking-cost moving control tick failed: {error}"));
    }
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut resting)
            .unwrap_or_else(|error| panic!("walking-cost resting control tick failed: {error}"));
    }

    let moving_survival = moving
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walking-cost moving survival disappeared"));
    let resting_survival = resting
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("walking-cost resting survival disappeared"));
    assert_eq!(
        resting_survival
            .metabolic_energy()
            .checked_sub(moving_survival.metabolic_energy()),
        Some(pending.energy_cost_per_tick())
    );
    assert_eq!(
        resting_survival
            .hydration()
            .checked_sub(moving_survival.hydration()),
        Some(pending.hydration_loss_per_tick())
    );
    assert_eq!(
        moving
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("walking-cost settled player disappeared"))
            .locomotion_exertion()
            .pending(),
        SurvivalExertion::REST
    );
}

#[test]
fn fatal_slow_boundary_discards_current_direct_horizontal_input() {
    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal-boundary setup control tick failed: {error}"));
    }
    let survival = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal-boundary survival player disappeared"));
    let survival_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        survival_revision,
        survival_revision + 1,
        player_record(
            Energy::ZERO,
            survival.hydration(),
            Vitality::from_parts_per_million_unchecked(1),
            survival.nutrition(),
            survival.vitality_recovery_remainder(),
        ),
    );
    let before = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal-boundary logistics player disappeared"))
        .body();
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("fatal-boundary input fixture failed: {error}")),
    );

    let outcome = advance_control_tick_with_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("fatal-boundary control tick failed: {error}"));
    assert_eq!(
        outcome
            .simulation()
            .and_then(TickOutcome::survival)
            .map(SurvivalAssessment::vitality),
        Some(Vitality::ZERO)
    );
    let after = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal-boundary logistics player disappeared after tick"))
        .body();
    assert_eq!(after.foot_position(), before.foot_position());
    assert_eq!(after.horizontal_velocity(), before.horizontal_velocity());
}

#[path = "mod_tests/vertical_boundaries.rs"]
mod vertical_boundaries;

#[test]
fn horizontal_motion_sweeps_to_exact_full_cube_wall_contact() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, floor_y, 0),
        BLOCK_STATE_STONE,
    );
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("wall-collision input fixture failed: {error}")),
    );
    for _ in 0..2 {
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("wall-collision control tick failed: {error}"));
    }
    let accumulator_before_contact = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("wall-collision player disappeared before contact"))
        .locomotion_exertion();
    let exertion_before_contact = accumulator_before_contact.pending();
    let _ = advance_control_tick_with_input(&registries, &mut state, input)
        .unwrap_or_else(|error| panic!("wall-contact control tick failed: {error}"));
    let accumulator_after_contact = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("wall-collision player disappeared after contact"))
        .locomotion_exertion();
    let exertion_after_contact = accumulator_after_contact.pending();
    assert_ne!(accumulator_after_contact, accumulator_before_contact);
    assert!(
        exertion_after_contact.energy_cost_per_tick()
            > exertion_before_contact.energy_cost_per_tick()
    );
    for _ in 3..8 {
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("wall-collision control tick failed: {error}"));
    }
    let body = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("wall-collision player disappeared"))
        .body();
    assert_eq!(body.foot_position().x_nanometers(), 700_000_000);
    assert_eq!(body.horizontal_velocity().x_nanometers_per_second(), 0);
    assert_eq!(
        body.foot_position().y_nanometers(),
        floor_y * NANOMETERS_PER_VOXEL
    );
}

#[test]
fn horizontal_motion_stops_at_missing_chunk_authority_boundary() {
    let (registries, mut state, _) = generated_grounded_player_at(15, 0);
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("chunk-boundary input fixture failed: {error}")),
    );
    for _ in 0..20 {
        let _ = advance_control_tick_with_input(&registries, &mut state, input)
            .unwrap_or_else(|error| panic!("chunk-boundary control tick failed: {error}"));
    }
    let body = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("chunk-boundary player disappeared"))
        .body();
    assert_eq!(body.foot_position().x_nanometers(), 15_700_000_000);
    assert_eq!(body.horizontal_velocity().x_nanometers_per_second(), 0);
    assert_eq!(
        body.voxel_position(),
        VoxelCoord::new(15, body.voxel_position().y(), 0)
    );
}

#[test]
fn unreachable_inventory_storage_history_is_rejected_during_decode() {
    let registries = build_registries();
    let mut source = AppState::new();
    let stockpile = add_solid_stockpile_for_test(&mut source, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("storage-age stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut source,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("storage-age lot fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &source))
        .unwrap_or_else(|error| panic!("storage-age fixture serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["ambient_age_parts"] = serde_json::json!(u64::MAX);
    let serialized = serde_json::to_string(&encoded)
        .unwrap_or_else(|error| panic!("storage-age fixture encoding failed: {error}"));
    let sentinel = format!("\"ambient_age_parts\":{}", u64::MAX);
    assert_eq!(serialized.matches(&sentinel).count(), 1);
    let replacement = "\"ambient_age_parts\":1";
    let encoded = serialized.replacen(&sentinel, replacement, 1);
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&encoded).is_err());
}

#[test]
fn canonical_tick_advances_exactly_once() {
    let registries = build_registries();
    let mut state = AppState::new();

    let result = advance_tick(&registries, &mut state);
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => panic!("tick unexpectedly failed: {error}"),
    };

    assert_eq!(outcome.tick(), SimulationTick::new(1));
    assert_eq!(state.tick(), SimulationTick::new(1));
    assert_eq!(
        state.control_tick(),
        ControlTick::from_simulation_tick(SimulationTick::new(1))
            .unwrap_or_else(|| panic!("first simulation tick must have a control boundary"))
    );
}

#[test]
fn control_ticks_hold_slow_simulation_until_exact_boundary() {
    let registries = build_registries();
    let mut state = AppState::new();

    for expected in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("control tick {expected} failed: {error}"));
        assert_eq!(outcome.control_tick(), ControlTick::new(expected));
        assert!(outcome.simulation().is_none());
        assert_eq!(state.tick(), SimulationTick::ZERO);
    }

    let boundary = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("control boundary failed: {error}"));
    assert_eq!(
        boundary.control_tick(),
        ControlTick::new(CONTROL_TICKS_PER_SIMULATION_TICK)
    );
    assert_eq!(
        boundary.simulation().map(TickOutcome::tick),
        Some(SimulationTick::new(1))
    );
    assert_eq!(state.tick(), SimulationTick::new(1));
}

#[test]
fn generated_world_body_falls_on_control_ticks_and_lands_exactly_on_full_cube_floor() {
    let (registries, mut state, floor_y) = generated_airborne_player(100);
    let before_slow_helper = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired {
            current: ControlTick::ZERO,
        })
    );
    assert_eq!(state, before_slow_helper);

    let first = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("first body control tick failed: {error}"));
    let motion = first
        .body_motion()
        .unwrap_or_else(|| panic!("airborne player did not receive body motion"));
    assert!(first.simulation().is_none());
    assert!(!motion.grounded());
    assert_eq!(
        motion.before().foot_position().y_nanometers(),
        (floor_y + 100) * NANOMETERS_PER_VOXEL
    );
    assert_eq!(
        motion.after().vertical_velocity().nanometers_per_second(),
        -490_332_500
    );
    assert_eq!(
        motion.after().foot_position().y_nanometers(),
        motion.before().foot_position().y_nanometers() - 24_516_625
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("airborne body serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("airborne body decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("airborne body trusted load failed: {error}"));
    assert_eq!(loaded, state);

    for expected in 2..=CONTROL_TICKS_PER_SIMULATION_TICK {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("body control tick {expected} failed: {error}"));
        assert_eq!(outcome.control_tick(), ControlTick::new(expected));
        if expected == CONTROL_TICKS_PER_SIMULATION_TICK {
            assert_eq!(
                outcome.simulation().map(TickOutcome::tick),
                Some(SimulationTick::new(1))
            );
        } else {
            assert!(outcome.simulation().is_none());
        }
    }
    assert!(
        state.logistics().player().is_some_and(|player| player
            .body()
            .foot_position()
            .y_nanometers()
            > floor_y * NANOMETERS_PER_VOXEL),
        "high-altitude body should still be airborne at the first slow boundary"
    );

    let expected_floor = floor_y * NANOMETERS_PER_VOXEL;
    let mut landed = false;
    for _ in 0..200 {
        let body = state
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| panic!("falling player disappeared"))
            .body();
        if body.foot_position().y_nanometers() == expected_floor
            && body.vertical_velocity().nanometers_per_second() == 0
        {
            landed = true;
            break;
        }
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fall-to-floor control tick failed: {error}"));
    }
    assert!(landed, "player body did not settle on generated terrain");
    let body = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("landed player disappeared"))
        .body();
    assert_eq!(body.foot_position().y_nanometers(), expected_floor);
    assert_eq!(body.vertical_velocity().nanometers_per_second(), 0);
}

#[test]
fn hard_fall_queues_persists_and_settles_exact_vitality_loss() {
    let (registries, mut state, floor_y) = generated_airborne_player(8);
    let vitality_before = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fall-injury survival player disappeared"))
        .vitality();
    let mut impact = None;
    for _ in 0..200 {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fall-injury control tick failed: {error}"));
        if let Some(found) = outcome
            .body_motion()
            .and_then(|motion| motion.fall_impact())
        {
            impact = Some(found);
            break;
        }
    }
    let impact = impact.unwrap_or_else(|| panic!("hard fall produced no landing impact"));
    assert!(
        impact.impact_speed()
            > registries
                .survival()
                .physiology()
                .locomotion()
                .safe_landing_speed(),
        "hard-fall fixture must cross the authored safe landing threshold"
    );
    assert!(impact.vitality_loss_ppm() > 0);
    assert!(impact.vitality_loss_ppm() < Vitality::MAXIMUM.parts_per_million());
    assert_eq!(
        state
            .logistics()
            .player()
            .map(|player| player.pending_fall_vitality_loss_ppm()),
        Some(impact.vitality_loss_ppm())
    );
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(vitality_before),
        "landing records the body consequence immediately but survival owns vitality settlement"
    );
    assert_eq!(
        state
            .logistics()
            .player()
            .map(|player| player.body().foot_position().y_nanometers()),
        Some(floor_y * NANOMETERS_PER_VOXEL)
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending fall injury serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("pending fall injury decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("pending fall injury trusted load failed: {error}"));
    assert_eq!(loaded, state);
    assert!(matches!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired { .. })
    ));

    let landed_at = state.control_tick();
    let mut survival_outcome = None;
    for _ in 0..=CONTROL_TICKS_PER_SIMULATION_TICK {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("pending fall injury settlement failed: {error}"));
        if let Some(survival) = outcome.simulation().and_then(TickOutcome::survival) {
            survival_outcome = Some(survival);
            break;
        }
    }
    let after = survival_outcome
        .unwrap_or_else(|| panic!("pending fall injury did not settle at the next slow boundary"));
    assert!(
        state
            .control_tick()
            .checked_steps_since(landed_at)
            .is_some_and(|ticks| ticks <= CONTROL_TICKS_PER_SIMULATION_TICK)
    );
    assert_eq!(
        after.vitality().parts_per_million(),
        vitality_before
            .parts_per_million()
            .saturating_sub(impact.vitality_loss_ppm())
    );
    assert_eq!(
        state
            .logistics()
            .player()
            .map(|player| player.pending_fall_vitality_loss_ppm()),
        Some(0)
    );
}

#[test]
fn canonical_tick_applies_exact_passive_energy_dissipation() {
    let registries = passive_dissipation_registries();
    let mut state = AppState::new();
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        DISSIPATIVE_STORE,
        Energy::from_nanojoules(5_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("passive-dissipation tick fixture failed: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("passive-dissipation tick failed: {error}"));

    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored()),
        Some(Energy::from_nanojoules(1_400_000_000_000_000))
    );
}

#[test]
fn clock_exhaustion_leaves_state_unchanged() {
    let registries = build_registries();
    let mut state = AppState::new();
    apply_clock_advance(&mut state, SimulationTick::MAX_CONTROL_ALIGNED);
    let before = state.clone();

    let result = advance_tick(&registries, &mut state);

    assert_eq!(
        result,
        Err(TickError::ControlClockExhausted {
            current: ControlTick::MAX_COMPLETE,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn exhausted_logistics_revision_rejects_boundary_motion_without_committing_slow_tick() {
    let (registries, mut state, _) = generated_grounded_player_at(0, 0);
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("boundary-motion setup failed: {error}"));
    }
    assert_eq!(
        state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK - 1
    );
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("boundary-motion save encoding failed: {error}"));
    encoded["state"]["systems"]["logistics"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("boundary-motion save decoding failed: {error}"));
    state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("boundary-motion exhausted save invalid: {error}"));
    let before = state.clone();
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("boundary-motion control invalid: {error}")),
    );
    assert_eq!(
        advance_control_tick_with_input(&registries, &mut state, input),
        Err(TickError::PlayerBodyMotion(
            crate::logistics::PlayerBodyMotionError::LogisticsRevisionExhausted
        ))
    );
    assert_eq!(
        state, before,
        "an error must not commit survival, work or clock advancement"
    );
    // Exhausted logistics may still simulate a fully neutral interval; it
    // must not permanently freeze independent survival and production owners.
    let neutral = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("neutral boundary should remain admissible: {error}"));
    assert_eq!(
        neutral.simulation().map(TickOutcome::tick),
        Some(SimulationTick::new(1))
    );
    assert_eq!(state.logistics().revision(), u64::MAX);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn exhausted_logistics_revision_preserves_due_gathering_on_boundary_motion_error() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let resource = crate::surface::insert_surface_resource_for_test(
        &registries,
        &mut state,
        crate::surface::SurfaceResourceSpec::new(
            crate::surface::SurfaceResourceKind::Opportunity,
            VoxelCoord::new(0, floor_y, 0),
            CommodityKey::new(crate::content::MATERIAL_STONE, crate::content::FORM_CHIP),
            Mass::from_milligrams(50_000),
            Temperature::from_millikelvin(293_150),
            crate::material::MaterialComposition::pure(crate::content::MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("boundary-gathering surface fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("boundary-gathering surface insertion failed: {error}"));
    crate::surface::validate_start_player_surface_gathering(
        &registries,
        &state,
        crate::content::SURFACE_GATHERING_HAND_COLLECT_STONE_CHIPS,
        resource,
        Mass::from_milligrams(50_000),
    )
    .unwrap_or_else(|error| panic!("boundary-gathering work admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("boundary-gathering work commit failed: {error}"));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::SurfaceGathering { work }) if work.completes_at() == SimulationTick::new(1)
    ));
    for _ in 1..CONTROL_TICKS_PER_SIMULATION_TICK {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("boundary-gathering setup step failed: {error}"));
    }
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("boundary-gathering save encoding failed: {error}"));
    encoded["state"]["systems"]["logistics"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("boundary-gathering save decoding failed: {error}"));
    let mut state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("boundary-gathering trusted load failed: {error}"));
    let before = state.clone();
    let input = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("boundary-gathering control invalid: {error}")),
    );
    assert_eq!(
        advance_control_tick_with_input(&registries, &mut state, input),
        Err(TickError::PlayerBodyMotion(
            crate::logistics::PlayerBodyMotionError::LogisticsRevisionExhausted
        ))
    );
    assert_eq!(
        state, before,
        "the rejected control must not gather the source or release reserved work"
    );
    let neutral = advance_control_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("neutral due-gathering step failed: {error}"));
    assert_eq!(
        neutral
            .simulation()
            .and_then(TickOutcome::surface_gathering)
            .map(|g| g.resource()),
        Some(resource)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn exhausted_survival_revision_reloaded_from_save_rejects_tick_atomically() {
    let registries = build_registries();
    let mut source = AppState::new();
    validate_admit_player(&registries, &source, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("survival-revision fixture admission failed: {error}"))
        .commit(&mut source)
        .unwrap_or_else(|error| {
            panic!("survival-revision fixture admission commit failed: {error}")
        });
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &source))
        .unwrap_or_else(|error| panic!("survival-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("survival-revision fixture decode failed: {error}"));
    let mut state = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exhausted survival revision should remain load-valid: {error}")
    });
    let before = state.clone();

    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::SurvivalRevisionExhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn exhausted_energy_revision_reloaded_from_save_rejects_passive_tick_atomically() {
    let registries = passive_dissipation_registries();
    let mut source = AppState::new();
    let _store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut source,
        DISSIPATIVE_STORE,
        Energy::from_nanojoules(5_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("energy-revision fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &source))
        .unwrap_or_else(|error| panic!("energy-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("energy-revision fixture decode failed: {error}"));
    let mut state = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exhausted energy revision should remain load-valid: {error}")
    });
    let before = state.clone();

    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::EnergyRevisionExhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn shared_owner_revision_capacity_accounts_for_all_same_tick_mutations() {
    assert_eq!(
        require_revision_capacity(u64::MAX - 2, [1, 1], TickError::EquipmentRevisionExhausted,),
        Ok(())
    );
    assert_eq!(
        require_revision_capacity(
            u64::MAX - 2,
            [1, 1, 1],
            TickError::EquipmentRevisionExhausted,
        ),
        Err(TickError::EquipmentRevisionExhausted)
    );
    assert_eq!(
        require_revision_capacity(u64::MAX - 1, [1, 1], TickError::EquipmentRevisionExhausted,),
        Err(TickError::EquipmentRevisionExhausted)
    );
}

#[test]
fn dead_player_remains_visible_in_tick_survival_outcome_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-player survival initialization failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("initialized dead-player fixture is missing survival state"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let frozen_revision = state.survival().revision();
    let frozen_player = state.survival().player().copied();

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-player tick failed: {error}"));

    assert_eq!(
        outcome.survival().map(SurvivalAssessment::vitality),
        Some(Vitality::ZERO)
    );
    assert_eq!(state.survival().revision(), frozen_revision);
    assert_eq!(state.survival().player().copied(), frozen_player);
}
