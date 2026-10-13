//! Shared trusted-load schedule projection contracts.

use super::*;

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STANDARD_TEST_HEATER, STANDARD_TEST_HEATING_ENERGY,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::add_equipment;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::PlayerStepWork;
use crate::logistics::{add_traversable_edge_for_test, validate_place_ground_stockpile};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::production::{ProcessId, validate_start_player_process};
use crate::spatial::VoxelCoord;
use crate::thermal::{SensibleHeatingRequest, resolve_sensible_heating_process};

const RETAINED_SITE_PROCESS: ProcessId = ProcessId::new(989_001);

#[test]
fn active_work_schedule_projects_total_and_remaining_ticks_once() {
    assert_eq!(
        project_active_work_schedule(
            SimulationTick::new(15),
            SimulationTick::new(10),
            SimulationTick::new(25),
        ),
        Some(ActiveWorkSchedule {
            duration: TickSpan::new(15),
            remaining: TickSpan::new(10),
        })
    );
}

#[test]
fn active_work_schedule_rejects_non_active_intervals() {
    for (current, started_at, completes_at) in [(10, 11, 20), (10, 5, 10), (10, 12, 11)] {
        assert_eq!(
            project_active_work_schedule(
                SimulationTick::new(current),
                SimulationTick::new(started_at),
                SimulationTick::new(completes_at),
            ),
            None
        );
    }
}

#[test]
fn trusted_step_rejects_retained_production_output_in_carried_custody() {
    let registries = make_test_registries_with_standard_sensible_heating(RETAINED_SITE_PROCESS);
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    let destination = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, origin, destination);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("retained-site player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("retained-site player admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("retained-site player disappeared"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("retained-site source allocation failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("retained-site source material failed: {error}"));
    validate_place_ground_stockpile(&state, source, origin)
        .unwrap_or_else(|error| panic!("retained-site source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("retained-site source placement commit failed: {error}"));
    let equipment = add_equipment(
        &registries,
        &mut state,
        STANDARD_TEST_HEATER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("retained-site equipment fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("retained-site energy fixture failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        equipment,
        origin,
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        energy,
        origin,
    );
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            RETAINED_SITE_PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy,
            Temperature::from_millikelvin(900_000),
        ),
    )
    .unwrap_or_else(|error| panic!("retained-site process resolution failed: {error}"));
    let job = validate_start_player_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        carried,
    )
    .unwrap_or_else(|error| panic!("retained-site production validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("retained-site production commit failed: {error}"));

    let duration = registries
        .survival()
        .physiology()
        .locomotion()
        .step_duration();
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .unwrap_or_else(|| panic!("retained-site step completion overflowed"));
    let work = PlayerStepWork::new(origin, destination, state.tick(), completes_at);
    let revision = state.player_work().revision();
    state
        .player_work_state_mut()
        .apply_start(revision, revision + 1, PlayerWork::Step { work });

    assert_eq!(
        validate_loaded_player_work(&registries, &state, state.player_work()),
        Err(PlayerWorkValidationError::StepProductionSiteConflict { job })
    );
}
