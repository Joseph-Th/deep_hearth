//! Contract tests for direct player-power execution and persistence.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_MECHANICAL_SMALL_DRIVE,
    ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE, ENERGY_THERMAL_SINK,
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_STONE_HAND_CRANK,
    EQUIPMENT_TIMBER_TREADLE_DRIVE, FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP,
    FORM_REINFORCEMENT, MANUAL_POWER_FOOT_TREADLE, MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_SHAPE_STONE_FLYWHEEL, PROCESS_SHAPE_TIMBER_FLYWHEEL,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    build_registries,
};
use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::TickSpan;
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::{
    EnergySinkError, EnergyStoreRecord, EnergySupplyError, PowerRemainder, add_energy_store,
    add_energy_store_with_initial_for_fixture, integrate_power, validate_assemble_energy_store,
    validate_energy_supply,
};
use crate::equipment::{
    validate_assemble_equipment, validate_mount_equipment, validate_unmount_equipment,
};
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError, PlayerWorkValidationError};
use crate::logistics::{
    PlayerEnergyStoreAccessError, validate_allocate_ground_stockpile,
    validate_initialize_player_logistics,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::survival::{Vitality, assess_survival, initialize_player_survival, player_record};

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("manual power setup tick failed: {error}"));
    }
}

#[test]
fn manual_power_rejects_known_remote_destination_store() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote power survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote power logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote power logistics commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        crank,
        player_position,
    );
    let store_position = VoxelCoord::new(1, 0, 0);
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        drive,
        store_position,
    );
    let before = state.clone();

    assert_eq!(
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(
                MANUAL_POWER_HAND_CRANK,
                crank,
                drive,
                Energy::from_nanojoules(100_000_000_000),
            ),
        )
        .err(),
        Some(ManualPowerError::DestinationAccess(
            PlayerEnergyStoreAccessError::RemoteKnownEnergyStore {
                store: drive,
                store_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn manual_power_token_rejects_logistics_change_before_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale power survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale power logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale power logistics commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        crank,
        player_position,
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        drive,
        player_position,
    );
    let validated = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            Energy::from_nanojoules(100_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("stale power validation failed: {error}"));
    let expected = state.logistics().revision();
    validate_allocate_ground_stockpile(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale power allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale power allocation commit failed: {error}"));
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(ManualPowerCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_active_manual_power_with_remote_destination() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote-load power survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-load power logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-load power logistics commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        crank,
        player_position,
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        drive,
        player_position,
    );
    let _ = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            Energy::from_nanojoules(100_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("remote-load power validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote-load power commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let store_position = VoxelCoord::new(1, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("remote-load power serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["energy_store_locations"][drive.value().to_string()] =
        serde_json::json!({"x": 1, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote-load power decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ManualPowerDestinationAccess(
                PlayerEnergyStoreAccessError::RemoteKnownEnergyStore {
                    store: drive,
                    store_position,
                    player_position,
                }
            )
        )))
    );
}

fn make_next_tick_fatal(registries: &Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal manual-power fixture player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

#[test]
fn fatal_tick_cancels_unfinished_manual_power_without_energy_or_wear() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal manual-power survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
        .unwrap_or_else(|error| panic!("fatal manual-power drive failed: {error}"));
    let requested = Energy::from_nanojoules(300_000_000_000);
    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("fatal manual-power validation failed: {error}"));
    let work = start.work();
    assert!(work.completes_at().value() > state.tick().value() + 1);
    let condition_before = state
        .equipment()
        .get_equipment(crank)
        .unwrap_or_else(|| panic!("fatal manual-power crank disappeared"))
        .condition();
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal manual-power commit failed: {error}"));
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal manual-power tick failed: {error}"));

    assert_eq!(outcome.manual_power(), None);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO),
        "unfinished manual power must not materialize generated work on fatal interruption"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(condition_before),
        "unfinished manual power must not apply completion wear on fatal interruption"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("fatal manual-power state audit failed: {error}"));

    let post_death = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("post-death manual-power tick failed: {error}"));
    assert_eq!(post_death.manual_power(), None);
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
}

#[test]
fn manual_power_due_on_fatal_tick_completes_before_attention_is_released() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due manual-power survival setup failed: {error}"));
    let crank = assemble_crank_fixture(
        &registries,
        &mut state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        true,
    );
    let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
        .unwrap_or_else(|error| panic!("fatal due manual-power drive failed: {error}"));
    let requested = Energy::from_nanojoules(300_000_000_000);
    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("fatal due manual-power validation failed: {error}"));
    let work = start.work();
    assert_eq!(work.completes_at().value(), state.tick().value() + 1);
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fatal due manual-power commit failed: {error}"));
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due manual-power tick failed: {error}"));

    assert_eq!(
        outcome.manual_power().map(ManualPowerOutcome::energy),
        Some(requested)
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(crank)
            .map(|record| record.condition()),
        Some(work.condition_after())
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("fatal due manual-power state audit failed: {error}"));
}

#[test]
fn manual_power_rejects_completion_owner_revision_exhaustion_at_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual power revision survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("manual power revision drive failed: {error}"));
    let requested = Energy::from_nanojoules(100_000_000_000);
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual power revision serialization failed: {error}"));

    for (owner, expected) in [
        ("equipment", ManualPowerError::EquipmentRevisionExhausted),
        ("energy", ManualPowerError::EnergyRevisionExhausted),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("manual power revision decode failed: {error}"));
        let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
            panic!("idle exhausted manual power owner should load: {error}")
        });
        let before = loaded.clone();
        assert_eq!(
            validate_start_manual_power(
                &registries,
                &loaded,
                ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
            )
            .err(),
            Some(expected)
        );
        assert_eq!(loaded, before);
    }
}

#[test]
fn active_manual_power_load_requires_completion_owner_revisions() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual power load revision survival failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("manual power load revision drive failed: {error}"));
    validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            drive,
            Energy::from_nanojoules(100_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("manual power load revision start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual power load revision commit failed: {error}"));
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual power load revision serialization failed: {error}"));

    for (owner, expected) in [
        (
            "equipment",
            PlayerWorkValidationError::ManualPowerEquipmentRevisionExhausted,
        ),
        (
            "energy",
            PlayerWorkValidationError::ManualPowerEnergyRevisionExhausted,
        ),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("manual power load revision decode failed: {error}"));
        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::PlayerWork(
                expected
            )))
        );
    }
}

#[path = "power_execution_tests/treadle_package.rs"]
mod treadle_package;

#[path = "power_execution_tests/current_projection.rs"]
mod current_projection;

fn assemble_crank_fixture(
    registries: &Registries,
    state: &mut AppState,
    definition: crate::equipment::EquipmentDefinitionId,
    with_copper: bool,
) -> EquipmentId {
    let capacity = if with_copper {
        Mass::from_milligrams(1_120_000)
    } else {
        Mass::from_milligrams(1_100_000)
    };
    let source = add_solid_stockpile_for_test(state, capacity)
        .unwrap_or_else(|error| panic!("crank comparison source failed: {error}"));
    for (commodity, mass) in [
        Some((
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        )),
        Some((
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        )),
        with_copper.then_some((
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
        )),
    ]
    .into_iter()
    .flatten()
    {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("crank comparison material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("crank comparison assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("crank comparison assembly commit failed: {error}"))
}

fn assemble_flywheel_fixture(registries: &Registries, state: &mut AppState) -> EnergyStoreId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("flywheel recharge source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("flywheel recharge material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
        .unwrap_or_else(|error| panic!("flywheel recharge assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("flywheel recharge assembly commit failed: {error}"))
}

fn active_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("manual power support bounds failed: {error}"));
    let element = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1_000_000),
            Area::from_square_millimeters(10_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("manual power support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, element)
        .unwrap_or_else(|error| panic!("manual power support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual power support commit failed: {error}"));
    element
}

#[test]
fn manual_power_requires_portable_unmounted_equipment_and_rejects_mounted_work_on_load() {
    let registries = build_registries();
    let mut mounted = AppState::new();
    initialize_player_survival(&registries, &mut mounted)
        .unwrap_or_else(|error| panic!("mounted manual power survival setup failed: {error}"));
    let crank =
        assemble_crank_fixture(&registries, &mut mounted, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = add_energy_store(&registries, &mut mounted, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("mounted manual power drive failed: {error}"));
    let support = active_support(&registries, &mut mounted);
    let _ = validate_mount_equipment(&registries, &mounted, crank, support)
        .unwrap_or_else(|error| panic!("manual power crank mount failed: {error}"))
        .commit(&mut mounted)
        .unwrap_or_else(|error| panic!("manual power crank mount commit failed: {error}"));
    let requested = Energy::from_nanojoules(100_000_000_000);
    let before = mounted.clone();

    assert_eq!(
        validate_start_manual_power(
            &registries,
            &mounted,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
        )
        .err(),
        Some(ManualPowerError::EquipmentMounted { equipment: crank })
    );
    assert_eq!(mounted, before);

    let mut active = mounted.clone();
    let _ = validate_unmount_equipment(&registries, &active, crank)
        .unwrap_or_else(|error| panic!("manual power crank unmount failed: {error}"))
        .commit(&mut active)
        .unwrap_or_else(|error| panic!("manual power crank unmount commit failed: {error}"));
    validate_start_manual_power(
        &registries,
        &active,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("portable manual power validation failed: {error}"))
    .commit(&mut active)
    .unwrap_or_else(|error| panic!("portable manual power commit failed: {error}"));

    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &mounted))
        .unwrap_or_else(|error| panic!("mounted manual power save failed: {error}"));
    let active_save = serde_json::to_value(SaveEnvelope::new(&registries, &active))
        .unwrap_or_else(|error| panic!("active manual power save failed: {error}"));
    forged["state"]["systems"]["player_work"] =
        active_save["state"]["systems"]["player_work"].clone();
    let forged: LoadedSaveEnvelope = serde_json::from_value(forged)
        .unwrap_or_else(|error| panic!("forged mounted manual power decode failed: {error}"));
    assert_eq!(
        forged.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ManualPowerEquipmentMounted
        )))
    );
}

#[test]
fn copper_reinforced_crank_halves_manual_charge_time_without_changing_energy_yield() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("crank comparison survival setup failed: {error}"));
    let stone_crank =
        assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let reinforced_crank = assemble_crank_fixture(
        &registries,
        &mut state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        true,
    );
    let bottleneck_drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("bottleneck crank comparison drive failed: {error}"));
    let stone_drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
        .unwrap_or_else(|error| panic!("stone crank comparison drive failed: {error}"));
    let reinforced_drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
        .unwrap_or_else(|error| panic!("reinforced crank comparison drive failed: {error}"));
    let requested = Energy::from_nanojoules(300_000_000_000);

    let bottlenecked = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            reinforced_crank,
            bottleneck_drive,
            requested,
        ),
    )
    .unwrap_or_else(|error| panic!("bottleneck crank comparison validation failed: {error}"));
    let stone = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, stone_crank, stone_drive, requested),
    )
    .unwrap_or_else(|error| panic!("stone crank comparison validation failed: {error}"));
    let reinforced = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            reinforced_crank,
            reinforced_drive,
            requested,
        ),
    )
    .unwrap_or_else(|error| panic!("reinforced crank comparison validation failed: {error}"));

    assert_eq!(
        bottlenecked.work().completes_at().value() - bottlenecked.work().started_at().value(),
        2
    );
    assert_eq!(
        stone.work().completes_at().value() - stone.work().started_at().value(),
        2
    );
    assert_eq!(
        reinforced.work().completes_at().value() - reinforced.work().started_at().value(),
        1
    );
    reinforced
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reinforced crank comparison commit failed: {error}"));
    let mut completion = None;
    for _ in 0..1 {
        completion = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("reinforced crank comparison tick failed: {error}"))
            .manual_power();
    }
    assert_eq!(completion.map(ManualPowerOutcome::energy), Some(requested));
    assert_eq!(
        state
            .energy()
            .get_store(reinforced_drive)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("reinforced crank comparison audit failed: {error}"));
}

#[test]
fn shared_energy_revision_budget_rejects_manual_power_plus_passive_loss_atomically() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("shared-energy survival setup failed: {error}"));
    let crank = assemble_crank_fixture(
        &registries,
        &mut state,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        true,
    );
    let manual_destination =
        add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
            .unwrap_or_else(|error| panic!("shared-energy manual destination failed: {error}"));
    let _passive_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::from_nanojoules(1_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("shared-energy passive store failed: {error}"));
    let requested = Energy::from_nanojoules(300_000_000_000);
    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            manual_destination,
            requested,
        ),
    )
    .unwrap_or_else(|error| panic!("shared-energy manual power validation failed: {error}"));
    assert_eq!(
        start.work().completes_at().value() - state.tick().value(),
        1
    );
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("shared-energy manual power commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("shared-energy save failed: {error}"));
    encoded["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("shared-energy save decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("shared-energy exhausted revision should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(crate::simulation::TickError::EnergyRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn passive_loss_cannot_spend_energy_revision_reserved_for_later_manual_power_completion() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("reserved-energy survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let manual_destination =
        add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_LARGE_DRIVE)
            .unwrap_or_else(|error| panic!("reserved-energy manual destination failed: {error}"));
    let _passive_store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::from_nanojoules(1_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("reserved-energy passive store failed: {error}"));
    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(
            MANUAL_POWER_HAND_CRANK,
            crank,
            manual_destination,
            Energy::from_nanojoules(300_000_000_000),
        ),
    )
    .unwrap_or_else(|error| panic!("reserved-energy manual power validation failed: {error}"));
    assert!(
        start.work().completes_at().value() > state.tick().value() + 1,
        "fixture requires passive loss before manual-power completion becomes due"
    );
    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("reserved-energy manual power commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("reserved-energy save failed: {error}"));
    encoded["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("reserved-energy save decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one reserved manual-power completion must remain load-valid: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(crate::simulation::TickError::EnergyRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn partial_flywheel_recharge_preserves_passive_loss_of_preexisting_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial-recharge survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let initial = Energy::from_nanojoules(100_000_000_000);
    let requested = Energy::from_nanojoules(300_000_000_000);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let initial_charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, initial),
    )
    .unwrap_or_else(|error| panic!("partial-recharge initial charge failed: {error}"));
    let initial_duration = initial_charge.work().completes_at().value() - state.tick().value();
    initial_charge
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-recharge initial commit failed: {error}"));
    advance_exact(&registries, &mut state, initial_duration);
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(initial),
        "initial work must exist before the partial recharge begins"
    );
    let definition = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("partial-recharge flywheel definition disappeared"));
    let passive_per_tick = integrate_power(
        definition.passive_dissipation_power(),
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| panic!("partial-recharge passive loss failed: {error}"));
    assert_eq!(passive_per_tick.remainder(), PowerRemainder::ZERO);

    let start = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("partial-recharge validation failed: {error}"));
    let duration = start.work().completes_at().value() - state.tick().value();
    assert!(
        duration > 1,
        "fixture must exercise passive loss before completion"
    );
    let total_passive_loss = Energy::from_nanojoules(
        passive_per_tick
            .energy()
            .nanojoules()
            .checked_mul(u128::from(duration))
            .unwrap_or_else(|| panic!("partial-recharge passive loss overflowed")),
    );
    let expected = initial
        .checked_sub(total_passive_loss)
        .and_then(|stored| stored.checked_add(requested))
        .unwrap_or_else(|| panic!("partial-recharge expected energy overflowed"));

    start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-recharge commit failed: {error}"));
    advance_exact(&registries, &mut state, duration);

    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(expected),
        "preexisting flywheel work must keep dissipating while newly generated work arrives only at completion"
    );
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("partial-recharge final audit failed: {error}"));
}

#[test]
fn manual_power_topoff_credits_guaranteed_pre_completion_flywheel_loss() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("topoff survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    let initial = Energy::from_nanojoules(300_100_000_000);
    let requested = Energy::from_nanojoules(200_000_000_000);
    let initial_charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, initial),
    )
    .unwrap_or_else(|error| panic!("topoff initial charge failed: {error}"));
    let initial_duration = initial_charge.work().completes_at().value() - state.tick().value();
    initial_charge
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("topoff initial commit failed: {error}"));
    advance_exact(&registries, &mut state, initial_duration);

    let capacity = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("topoff flywheel definition disappeared"))
        .capacity();
    assert!(
        initial
            .checked_add(requested)
            .is_some_and(|sum| sum > capacity),
        "fixture must exceed capacity if deferred passive recovery is ignored"
    );
    let topoff = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("physically feasible deferred topoff was rejected: {error}"));
    let duration = topoff.work().completes_at().value() - state.tick().value();
    assert!(
        duration > 1,
        "topoff fixture requires a pre-completion loss tick"
    );
    topoff
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("topoff commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("topoff active-work save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("topoff active-work decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("topoff active-work replay validation failed: {error}"));
    advance_exact(&registries, &mut loaded, duration);
    assert!(
        loaded
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() <= capacity),
        "deferred topoff must never overfill the finite flywheel"
    );
    validate_loaded_state(&registries, &loaded)
        .unwrap_or_else(|error| panic!("topoff final audit failed: {error}"));
}

#[test]
fn manual_power_topoff_does_not_credit_completion_tick_passive_loss() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("overfill-boundary survival setup failed: {error}"));
    let crank = assemble_crank_fixture(&registries, &mut state, EQUIPMENT_STONE_HAND_CRANK, false);
    let drive = assemble_flywheel_fixture(&registries, &mut state);
    // With the authored 1 W flywheel drag, one pre-completion tick frees 3.6 J. Keep the
    // over-capacity margin above that amount so only an incorrect completion-tick credit can
    // authorize the top-off.
    let initial = Energy::from_nanojoules(304_000_000_000);
    let requested = Energy::from_nanojoules(200_000_000_000);
    let initial_charge = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, initial),
    )
    .unwrap_or_else(|error| panic!("overfill-boundary initial charge failed: {error}"));
    let initial_duration = initial_charge.work().completes_at().value() - state.tick().value();
    initial_charge
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("overfill-boundary initial commit failed: {error}"));
    advance_exact(&registries, &mut state, initial_duration);
    let before = state.clone();

    assert!(matches!(
        validate_start_manual_power(
            &registries,
            &state,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
        ),
        Err(ManualPowerError::EnergySink(
            EnergySinkError::InsufficientCapacity { .. }
        ))
    ));
    assert_eq!(
        state, before,
        "manual power must not borrow capacity from passive loss that occurs after same-tick ingress"
    );
}

#[test]
fn primitive_hand_crank_turns_player_work_into_finite_mechanical_energy() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual power survival initialization failed: {error}"));
    let raw = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual power raw stockpile failed: {error}"));
    let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual power shaped stockpile failed: {error}"));
    let stone = deposit_lot_for_test(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual power stone fixture failed: {error}"));
    let wood = deposit_lot_for_test(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual power wood fixture failed: {error}"));

    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_STONE_FLYWHEEL,
            raw,
            MaterialLotSelection::new(stone, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("flywheel shaping validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("flywheel shaping commit failed: {error}"));
    advance_exact(&registries, &mut state, 60);
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_HANDLE,
            raw,
            MaterialLotSelection::new(wood, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("crank handle shaping validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("crank handle shaping commit failed: {error}"));
    advance_exact(&registries, &mut state, 40);

    let crank =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_HAND_CRANK, shaped)
            .unwrap_or_else(|error| panic!("hand crank assembly validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("hand crank assembly commit failed: {error}"));
    let drive = add_energy_store(&registries, &mut state, ENERGY_MECHANICAL_SMALL_DRIVE)
        .unwrap_or_else(|error| panic!("manual power drive allocation failed: {error}"));
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual power survival state disappeared"));
    let condition_before = state
        .equipment()
        .get_equipment(crank)
        .unwrap_or_else(|| panic!("assembled hand crank disappeared"))
        .condition();

    let requested = Energy::from_nanojoules(1_700_000_000_000);
    let base_save = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual power reserve serialization failed: {error}"));
    let mut low_energy = base_save.clone();
    low_energy["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    let low_energy: LoadedSaveEnvelope = serde_json::from_value(low_energy)
        .unwrap_or_else(|error| panic!("low-energy manual power decode failed: {error}"));
    let low_energy = low_energy
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("low-energy manual power load failed: {error}"));
    assert!(matches!(
        validate_start_manual_power(
            &registries,
            &low_energy,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested,),
        ),
        Err(ManualPowerError::Work(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));

    let mut low_hydration = base_save;
    low_hydration["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let low_hydration: LoadedSaveEnvelope = serde_json::from_value(low_hydration)
        .unwrap_or_else(|error| panic!("low-hydration manual power decode failed: {error}"));
    let low_hydration = low_hydration
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("low-hydration manual power load failed: {error}"));
    assert!(matches!(
        validate_start_manual_power(
            &registries,
            &low_hydration,
            ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested,),
        ),
        Err(ManualPowerError::Work(
            PlayerWorkStartError::InsufficientHydration { .. }
        ))
    ));

    let mut stale_survival_state = state.clone();
    let stale_survival = validate_start_manual_power(
        &registries,
        &stale_survival_state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("stale-survival manual power setup failed: {error}"));
    let _ = advance_tick(&registries, &mut stale_survival_state)
        .unwrap_or_else(|error| panic!("stale-survival setup tick failed: {error}"));
    assert_eq!(
        stale_survival.commit(&mut stale_survival_state),
        Err(ManualPowerCommitError::Work(
            PlayerWorkCommitError::StaleSurvivalRevision {
                expected: state.survival().revision(),
                actual: stale_survival_state.survival().revision(),
            }
        ))
    );

    let token = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested),
    )
    .unwrap_or_else(|error| panic!("manual power validation failed: {error}"));
    let projected_resource_budget = token.resource_budget();
    assert!(!projected_resource_budget.metabolic_energy().is_zero());
    assert!(!projected_resource_budget.hydration().is_zero());
    let work = token.work();
    assert_eq!(work.completes_at().value() - work.started_at().value(), 10);
    token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual power commit failed: {error}"));

    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    assert_eq!(
        validate_energy_supply(&registries, &state, drive, Energy::from_nanojoules(1)),
        Err(EnergySupplyError::StoreBusyManualPower { store: drive })
    );

    advance_exact(&registries, &mut state, 5);
    assert_eq!(
        state
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual power tamper serialization failed: {error}"));
    tampered["state"]["systems"]["player_work"]["active"]["ManualPower"]["work"]["completes_at"] =
        serde_json::json!(work.completes_at().value() + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("manual power tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ManualPowerDurationMismatch
        )))
    );
    let mut tampered =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("manual power exertion tamper serialization failed: {error}")
        });
    tampered["state"]["systems"]["player_work"]["active"]["ManualPower"]["work"]["exertion"] =
        serde_json::to_value(crate::survival::SurvivalExertion::REST)
            .unwrap_or_else(|error| panic!("resting exertion serialization failed: {error}"));
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("manual power exertion tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ManualPowerExertionMismatch
        )))
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active manual power serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active manual power decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("active manual power load validation failed: {error}"));
    assert_eq!(loaded, state);

    let mut completion = None;
    for _ in 0..5 {
        completion = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("manual power completion tick failed: {error}"))
            .manual_power();
    }
    assert_eq!(completion.map(ManualPowerOutcome::energy), Some(requested));
    assert_eq!(
        loaded
            .energy()
            .get_store(drive)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    assert_eq!(loaded.player_work().active(), None);
    assert!(
        loaded
            .equipment()
            .get_equipment(crank)
            .unwrap_or_else(|| panic!("hand crank disappeared after completion"))
            .condition()
            < condition_before
    );
    assert!(
        assess_survival(&registries, &loaded)
            .unwrap_or_else(|| panic!("manual power survival state disappeared after completion"))
            .metabolic_energy()
            < survival_before.metabolic_energy()
    );
    let survival_after = assess_survival(&registries, &loaded)
        .unwrap_or_else(|| panic!("manual power survival state disappeared after completion"));
    assert_eq!(
        survival_before
            .metabolic_energy()
            .checked_sub(survival_after.metabolic_energy()),
        Some(projected_resource_budget.metabolic_energy()),
        "manual-power admission must expose the exact metabolic reserve consumed by completion"
    );
    assert_eq!(
        survival_before
            .hydration()
            .checked_sub(survival_after.hydration()),
        Some(projected_resource_budget.hydration()),
        "manual-power admission must expose the exact hydration reserve consumed by completion"
    );
    let generated_supply = validate_energy_supply(&registries, &loaded, drive, requested)
        .unwrap_or_else(|error| panic!("generated mechanical energy was not consumable: {error}"));
    assert_eq!(generated_supply.trace().energy(), requested);
    validate_loaded_state(&registries, &loaded)
        .unwrap_or_else(|error| panic!("manual power final audit failed: {error}"));
}
