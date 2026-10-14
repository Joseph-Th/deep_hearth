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
use crate::logistics::{HorizontalControlInput, PlayerControlInput, PlayerStance};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::registry::Registries;
use crate::spatial::{NANOMETERS_PER_VOXEL, VoxelCoord};
use crate::survival::{SurvivalAssessment, Vitality, initialize_player_survival, player_record};
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk, validate_generate_player_neighborhood,
};

const DISSIPATIVE_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(510_001);

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
    assert!(
        second.after().vertical_velocity().nanometers_per_second()
            < first_unloaded
                .after()
                .vertical_velocity()
                .nanometers_per_second()
    );

    for _ in 0..40 {
        let _ = advance_control_tick(&registries, &mut unloaded)
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
            return;
        }
    }
    panic!("jump did not return to exact floor contact within bounded control ticks");
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
