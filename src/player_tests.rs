//! Atomic ordinary-player admission and stale-owner rejection contracts.

use crate::content::{
    BLOCK_STATE_STONE, EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_LOG, FORM_TOOL, MATERIAL_STONE,
    MATERIAL_WOOD, STANDARD_TEST_HEATER, STANDARD_TEST_HEATING_ENERGY, build_registries,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::{add_equipment, validate_assemble_equipment};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test,
};
use crate::logistics::{
    GroundStockpileAllocationError, HorizontalControlInput, LogisticsValidationError,
    PlayerActiveSelection, PlayerControlInput, project_player_inventory, relocate_player_for_test,
    validate_allocate_ground_stockpile, validate_pickup_equipment,
    validate_player_logistics_initialization, validate_quick_move_player_stack,
    validate_set_active_selection,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessId, validate_start_process};
use crate::simulation::{advance_control_tick, advance_control_tick_with_input, advance_tick};
use crate::spatial::VoxelCoord;
use crate::survival::{Vitality, assess_survival, initialize_player_survival, player_record};
use crate::thermal::{SensibleHeatingRequest, resolve_sensible_heating_process};
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

use super::{
    PlayerAdmissionCommitError, PlayerAdmissionError, PlayerRespawnCommitError, PlayerRespawnError,
    validate_admit_player, validate_respawn_player,
};

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal respawn fixture lost survival player"));
    let physiology = registries.survival().physiology();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
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

fn mark_player_dead(state: &mut AppState) {
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("respawn fixture lost survival player"));
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

#[test]
fn respawn_falls_back_to_death_location_when_spawn_anchor_becomes_solid() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5253_5041_574E),
        ChunkCoord::containing(VoxelCoord::new(0, 0, 0)),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("respawn fallback terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("respawn fallback terrain commit failed: {error}"));
    let surface = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("respawn fallback terrain has no surface"));
    let floor_y = surface + 1;
    for x in 0..=1 {
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
    let spawn = VoxelCoord::new(0, floor_y, 0);
    let death = VoxelCoord::new(1, floor_y, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("respawn fallback admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn fallback admission commit failed: {error}"));
    let movement = PlayerControlInput::new(
        HorizontalControlInput::new(1_000_000, 0)
            .unwrap_or_else(|error| panic!("respawn fallback movement input failed: {error}")),
    );
    for _ in 0..40 {
        if state
            .logistics()
            .player()
            .is_some_and(|player| player.position() == death)
        {
            break;
        }
        let _ = advance_control_tick_with_input(&registries, &mut state, movement)
            .unwrap_or_else(|error| panic!("respawn fallback movement failed: {error}"));
    }
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(death)
    );
    for _ in 0..20 {
        let stopped = state.logistics().player().is_some_and(|player| {
            let velocity = player.body().horizontal_velocity();
            velocity.x_nanometers_per_second() == 0 && velocity.z_nanometers_per_second() == 0
        });
        if stopped {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("respawn fallback braking failed: {error}"));
    }
    set_generated_block_for_test(registries.world(), &mut state, spawn, BLOCK_STATE_STONE);
    mark_player_dead(&mut state);

    let outcome = validate_respawn_player(&registries, &state)
        .unwrap_or_else(|error| panic!("respawn fallback validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn fallback commit failed: {error}"));
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), death);
    assert_eq!(outcome.death_position(), death);
}

#[test]
fn ordinary_player_admission_commits_all_owner_state_together() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(4, -2, 7);

    let admitted = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player admission commit failed: {error}"));

    assert!(state.survival().player().is_some());
    assert_eq!(state.logistics().player().copied(), Some(admitted));
    assert_eq!(admitted.position(), position);
    let carried = state
        .inventory()
        .get_stockpile(admitted.carried_stockpile())
        .unwrap_or_else(|| panic!("admitted carried stockpile disappeared"));
    assert_eq!(
        carried.capacity(),
        registries.survival().physiology().carried_mass_capacity()
    );
    assert_eq!(carried.stored_mass(), Mass::ZERO);
}

#[test]
fn dead_player_respawns_with_recoverable_exact_local_custody() {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = VoxelCoord::new(0, 0, 0);
    let death = VoxelCoord::new(1, 0, 0);
    let admitted = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("respawn fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn fixture admission commit failed: {error}"));
    assert_eq!(admitted.respawn_position(), spawn);
    let old_carried = admitted.carried_stockpile();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        old_carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("respawn carried material failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            old_carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("respawn pick assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, old_carried)
        .unwrap_or_else(|error| panic!("respawn pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn pick assembly commit failed: {error}"));
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("respawn pick pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn pick pickup commit failed: {error}"));
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::equipment(pick)),
    )
    .unwrap_or_else(|error| panic!("respawn pick selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("respawn pick selection commit failed: {error}"));
    relocate_player_for_test(&mut state, death);
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(death)
    );
    let matter_before_death = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("respawn matter baseline failed: {error}"))
        .total();
    make_next_tick_fatal(&registries, &mut state);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("respawn canonical fatal tick failed: {error}"));
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );

    let dead_encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dead respawn state serialization failed: {error}"));
    let dead_loaded: LoadedSaveEnvelope = serde_json::from_slice(&dead_encoded)
        .unwrap_or_else(|error| panic!("dead respawn state decode failed: {error}"));
    let dead_loaded = dead_loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("dead respawn state trusted load failed: {error}"));
    assert_eq!(dead_loaded, state);

    let outcome = validate_respawn_player(&registries, &state)
        .unwrap_or_else(|error| panic!("respawn validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn commit failed: {error}"));
    assert_eq!(outcome.position(), spawn);
    assert!(outcome.used_spawn_anchor());
    assert_eq!(outcome.death_position(), death);
    assert_eq!(outcome.death_cache(), old_carried);
    assert_ne!(outcome.carried_stockpile(), old_carried);
    assert_eq!(outcome.dropped_equipment(), &[pick]);
    assert_eq!(
        state.logistics().stationary_stockpile_position(old_carried),
        Some(death)
    );
    assert_eq!(
        state.logistics().stationary_equipment_position(pick),
        Some(death)
    );
    assert!(state.logistics().has_valid_equipment_position_index());
    assert_eq!(
        state.logistics().equipment_locations_near(death, 0),
        vec![(pick, death)]
    );
    assert!(!state.logistics().is_equipment_carried(pick));
    assert_eq!(state.logistics().active_selection(), None);
    assert_eq!(
        state
            .inventory()
            .get_lot(lot)
            .map(|record| (record.stockpile(), record.mass())),
        Some((old_carried, Mass::from_milligrams(500_000)))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(outcome.carried_stockpile())
            .map(|record| (record.capacity(), record.stored_mass())),
        Some((
            registries.survival().physiology().carried_mass_capacity(),
            Mass::ZERO,
        ))
    );
    let survival = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("respawned survival assessment disappeared"));
    assert_eq!(survival.vitality(), Vitality::MAXIMUM);
    assert_eq!(
        survival.metabolic_energy(),
        registries
            .survival()
            .physiology()
            .maximum_metabolic_energy()
    );
    assert_eq!(
        survival.hydration(),
        registries.survival().physiology().maximum_hydration()
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("respawn matter audit failed: {error}"))
            .total(),
        matter_before_death
    );

    relocate_player_for_test(&mut state, death);
    let view = project_player_inventory(&registries, &state)
        .unwrap_or_else(|error| panic!("death-cache inventory projection failed: {error}"));
    assert_eq!(view.local_containers().len(), 1);
    assert_eq!(view.local_containers()[0].stockpile(), old_carried);
    assert_eq!(view.local_containers()[0].stacks().len(), 1);
    assert_eq!(view.local_equipment().len(), 1);
    assert_eq!(view.local_equipment()[0].equipment(), pick);
    let stack = view.local_containers()[0].stacks()[0].clone();
    let recovered =
        validate_quick_move_player_stack(&registries, &state, &stack, outcome.carried_stockpile())
            .unwrap_or_else(|error| panic!("death-cache quick move failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("death-cache quick move commit failed: {error}"));
    assert_eq!(recovered.mass(), Mass::from_milligrams(500_000));
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("death-cache equipment pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("death-cache equipment pickup commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(old_carried)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert!(state.logistics().is_equipment_carried(pick));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("recovered respawn matter audit failed: {error}"))
            .total(),
        matter_before_death
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("recovered respawn state serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("recovered respawn state decode failed: {error}"));
    assert_eq!(
        loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("recovered respawn trusted load failed: {error}")),
        state
    );
}

#[test]
fn respawn_rejects_living_player_and_stale_inventory_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("respawn rejection admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn rejection admission commit failed: {error}"));
    let living = state.clone();
    assert_eq!(
        validate_respawn_player(&registries, &state).err(),
        Some(PlayerRespawnError::PlayerAlive)
    );
    assert_eq!(state, living);

    mark_player_dead(&mut state);
    let validated = validate_respawn_player(&registries, &state)
        .unwrap_or_else(|error| panic!("stale respawn validation failed: {error}"));
    let _ = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("stale respawn competing inventory mutation failed: {error}"));
    let before_commit = state.clone();
    assert!(matches!(
        validated.commit(&mut state),
        Err(PlayerRespawnCommitError::StaleInventoryRevision { .. })
    ));
    assert_eq!(state, before_commit);
}

#[test]
fn current_save_requires_persisted_respawn_anchor() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(2, 0, -3))
        .unwrap_or_else(|error| panic!("respawn-anchor admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("respawn-anchor admission commit failed: {error}"));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("respawn-anchor serialization failed: {error}"));
    let mut encoded = base.clone();
    let removed = encoded["state"]["systems"]["logistics"]["player"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized logistics player is not an object"))
        .remove("respawn_position");
    assert!(removed.is_some());
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(encoded).is_err(),
        "current schema must not invent a missing respawn anchor"
    );

    let invalid_position = VoxelCoord::new(9_223_372_037, 0, 0);
    let mut invalid = base;
    invalid["state"]["systems"]["logistics"]["player"]["respawn_position"] =
        serde_json::json!({"x": invalid_position.x(), "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(invalid)
        .unwrap_or_else(|error| panic!("invalid respawn-anchor decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerRespawnPositionOutsideContinuousWorld {
                position: invalid_position,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_horizontal_body_velocity_above_authored_maximum_speed() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("velocity-bound admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("velocity-bound admission commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("velocity-bound serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["horizontal_velocity"]["x_nanometers_per_second"] =
        serde_json::json!(5_600_000_001_i64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("velocity-bound decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerHorizontalVelocityExceedsMaximumSpeed {
                x_nanometers_per_second: 5_600_000_001,
                z_nanometers_per_second: 0,
                maximum_nanometers_per_second: 5_600_000_000,
            },
        )))
    );
}

#[test]
fn ordinary_player_admission_rejects_body_extent_beyond_continuous_world() {
    let registries = build_registries();
    let state = AppState::new();
    let position = VoxelCoord::new(0, 9_223_372_036, 0);

    assert_eq!(
        validate_admit_player(&registries, &state, position).err(),
        Some(PlayerAdmissionError::PositionOutsideContinuousWorld { position })
    );
}

#[test]
fn trusted_load_rejects_player_body_extent_beyond_continuous_world() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("body-bound load admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("body-bound load admission commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("body-bound load serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["foot_position"]["y_nanometers"] =
        serde_json::json!(9_223_372_036_000_000_000_i64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("body-bound load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerBodyOutsideContinuousWorld,
        )))
    );
}

#[test]
fn ordinary_player_admission_rejects_partial_survival_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival-only fixture failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: true,
            logistics_initialized: false,
        })
    );
}

#[test]
fn ordinary_player_admission_rejects_partial_logistics_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_player_logistics_initialization(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("logistics-only fixture failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("logistics-only fixture commit failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: false,
            logistics_initialized: true,
        })
    );
}

#[test]
fn stale_inventory_rejects_player_admission_without_partial_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let validated = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"));
    let _competitor = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("competing inventory mutation failed: {error}"));
    let before_commit = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(PlayerAdmissionCommitError::StaleInventoryRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn stockpile_allocation_preserves_inventory_revision_reserved_for_running_production() {
    let process = ProcessId::new(988_001);
    let registries = make_test_registries_with_standard_sensible_heating(process);
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("allocation-headroom source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("allocation-headroom destination failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom source material failed: {error}"));
    let equipment = add_equipment(
        &registries,
        &mut state,
        STANDARD_TEST_HEATER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("allocation-headroom equipment failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom energy failed: {error}"));
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            process,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy,
            Temperature::from_millikelvin(900_000),
        ),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom process resolution failed: {error}"));
    validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("allocation-headroom production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("allocation-headroom production commit failed: {error}"));

    let required = state
        .checked_future_inventory_revision_demand()
        .unwrap_or_else(|| panic!("allocation-headroom future inventory demand overflowed"));
    assert!(
        required > 0,
        "running production must retain future inventory work"
    );
    let revision = u64::MAX - required;
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("allocation-headroom serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(revision);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("allocation-headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exactly reserved inventory revision capacity should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_allocate_ground_stockpile(
            &loaded,
            VoxelCoord::new(0, 0, 0),
            Mass::from_milligrams(1),
        )
        .err(),
        Some(GroundStockpileAllocationError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(
        validate_admit_player(&registries, &loaded, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}
