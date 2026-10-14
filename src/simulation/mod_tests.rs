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
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::registry::Registries;
use crate::spatial::{NANOMETERS_PER_VOXEL, VoxelCoord};
use crate::survival::{SurvivalAssessment, Vitality, initialize_player_survival, player_record};
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
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
