//! Focused tests for production admission, completion, routing, and conservation semantics.

use std::ops::Deref;

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_FOOD, FORM_INGOT, FORM_LOG, MATERIAL_BERRIES, MATERIAL_COPPER,
    MATERIAL_WOOD, STANDARD_TEST_HEATER, STANDARD_TEST_HEATING_ENERGY, STANDARD_TEST_SCREEN,
    STANDARD_TEST_SCREENING_ENERGY, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    make_test_registries_with_standard_screening,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{EnergyStoreId, add_energy_store_with_initial_for_fixture};
use crate::equipment::{EquipmentId, add_equipment};
use crate::inventory::{
    MaterialFixtureError, MaterialIngressError, MaterialLotSelection, MaterialRelocationError,
    MaterialRelocationTestError, StockpileId, StockpileStorageProfile,
    add_solid_stockpile_for_test, add_stockpile, deposit_bulk_for_test, deposit_lot_for_test,
    deposit_lot_spec_for_test, validate_material_relocation_for_test, validate_mount_stockpile,
};
use crate::logistics::{
    PlayerStepCommitError, PlayerStepError, PlayerStockpileAccessError,
    add_traversable_edge_for_test, validate_place_ground_stockpile, validate_start_player_step,
};
use crate::maintenance::Condition;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialLotSpec, ParticleSizeClass,
    ParticleSizeDistribution, ParticleSizeRange,
};
use crate::ore_processing::{
    ResolvedScreening, ScreeningProcessDefinition, ScreeningRequest, resolve_screening_process,
};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, ProductionJobId,
    ProductionJobRecord, ProductionSiteEndpoint, ProductionValidationError,
    validate_process_inputs,
};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::survival::{FoodFreshness, Vitality, assess_food_freshness, player_record};
use crate::thermal::{
    ResolvedSensibleHeating, SensibleHeatingRequest, ThermalJobValidationError,
    resolve_sensible_heating_process,
};

const TEST_PROCESS: ProcessId = ProcessId::new(900_001);
const TEST_COMPOSITION_PROCESS: ProcessId = ProcessId::new(900_002);
const TEST_PERISHABLE_PROCESS: ProcessId = ProcessId::new(900_003);
const TEST_TARGET_TEMPERATURE: Temperature = Temperature::from_millikelvin(900_000);

fn wood_log() -> CommodityKey {
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
}

#[test]
fn player_process_start_requires_complete_player_admission() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolved = make_test_resolution(&registries, &mut state, source);

    assert!(
        validate_start_process(&registries, &state, &resolved, source, destination).is_ok(),
        "capability-level production admission must remain available without a player"
    );
    let before = state.clone();
    assert_eq!(
        validate_start_player_process(&registries, &state, &resolved, source, destination).err(),
        Some(StartPlayerProcessError::PlayerNotAdmitted)
    );
    assert_eq!(state, before);
}

#[test]
fn player_process_start_rejects_dead_admitted_player_without_mutation() {
    let (registries, state, source, carried, resolved, _step_destination) =
        carried_output_process_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dead player production serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("dead player production decode failed: {error}"));
    let dead = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle dead admitted player should remain a valid persisted state: {error}")
    });
    let before = dead.clone();

    assert_eq!(
        validate_start_player_process(&registries, &dead, &resolved, source, carried).err(),
        Some(StartPlayerProcessError::PlayerDead)
    );
    assert_eq!(dead, before);
}

#[test]
fn validated_player_process_rejects_survival_change_before_commit() {
    let (registries, mut state, source, carried, resolved, _step_destination) =
        carried_output_process_fixture();
    let validated = validate_start_player_process(&registries, &state, &resolved, source, carried)
        .unwrap_or_else(|error| panic!("player production validation failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("player production survival state disappeared"));
    let expected = state.survival().revision();
    let actual = expected + 1;
    state.survival_state_mut().apply_player(
        expected,
        actual,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(StartProcessCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(state, before);
}

fn berry_food() -> CommodityKey {
    CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
}

fn bind_source_mass(
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    mass: Mass,
) -> super::super::resolution::ValidatedProcessInputs {
    let lot =
        state.inventory().lot_ids(source).next().unwrap_or_else(|| {
            panic!("test process source {} has no material lot", source.value())
        });
    validate_process_inputs(
        state,
        process,
        source,
        &[MaterialLotSelection::new(lot, mass)],
    )
    .unwrap_or_else(|error| panic!("test process input binding failed: {error}"))
}

#[derive(Clone, Copy)]
struct TestHeatingResources {
    equipment: EquipmentId,
    energy: EnergyStoreId,
}

struct TestHeatingResolution(ResolvedSensibleHeating);

impl Deref for TestHeatingResolution {
    type Target = ProcessResolution;

    fn deref(&self) -> &Self::Target {
        self.0.process_resolution()
    }
}

fn add_test_heating_resources(
    registries: &Registries,
    state: &mut AppState,
) -> TestHeatingResources {
    let equipment = add_equipment(registries, state, STANDARD_TEST_HEATER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("test heating equipment fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        registries,
        state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("test heating energy fixture failed: {error}"));
    TestHeatingResources { equipment, energy }
}

fn resolve_test_heating(
    registries: &Registries,
    state: &AppState,
    process: ProcessId,
    source: StockpileId,
    resources: TestHeatingResources,
    target: Temperature,
) -> TestHeatingResolution {
    let lot =
        state.inventory().lot_ids(source).next().unwrap_or_else(|| {
            panic!("test heating source {} has no material lot", source.value())
        });
    let resolved = resolve_sensible_heating_process(
        registries,
        state,
        SensibleHeatingRequest::new(
            process,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            resources.equipment,
            resources.energy,
            target,
        ),
    )
    .unwrap_or_else(|error| panic!("test sensible-heating resolution failed: {error}"));
    TestHeatingResolution(resolved)
}

fn screening_distribution() -> ParticleSizeDistribution {
    let class = |minimum, maximum, weight| {
        let range = ParticleSizeRange::new(
            Length::from_micrometers(minimum),
            Length::from_micrometers(maximum),
        )
        .unwrap_or_else(|error| panic!("routed screening range fixture failed: {error}"));
        ParticleSizeClass::new(range, weight)
            .unwrap_or_else(|error| panic!("routed screening class fixture failed: {error}"))
    };
    ParticleSizeDistribution::new(vec![class(500, 2_000, 6), class(6_000, 10_000, 4)])
        .unwrap_or_else(|error| panic!("routed screening distribution fixture failed: {error}"))
}

fn make_test_multi_stream_resolution(
    registries: &Registries,
    state: &mut AppState,
    source: StockpileId,
) -> ResolvedScreening {
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
        MaterialComposition::pure(MATERIAL_COPPER),
        screening_distribution(),
    )
    .unwrap_or_else(|error| panic!("routed screening input fixture failed: {error}"));
    let lot = deposit_lot_spec_for_test(registries, state, source, input)
        .unwrap_or_else(|error| panic!("routed screening deposit failed: {error}"));
    let equipment = add_equipment(registries, state, STANDARD_TEST_SCREEN, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("routed screening equipment fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        registries,
        state,
        STANDARD_TEST_SCREENING_ENERGY,
        Energy::from_nanojoules(10_000_000_000),
    )
    .unwrap_or_else(|error| panic!("routed screening energy fixture failed: {error}"));
    resolve_screening_process(
        registries,
        state,
        ScreeningRequest::new(
            TEST_PROCESS,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy,
        ),
    )
    .unwrap_or_else(|error| panic!("routed screening resolution failed: {error}"))
}

fn make_test_registries() -> Registries {
    make_test_registries_with_standard_sensible_heating(TEST_PROCESS)
}

fn make_test_resolution(
    registries: &Registries,
    state: &mut AppState,
    source: StockpileId,
) -> TestHeatingResolution {
    let resources = add_test_heating_resources(registries, state);
    resolve_test_heating(
        registries,
        state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    )
}

fn make_resolution_for_process(
    registries: &Registries,
    state: &mut AppState,
    source: StockpileId,
    process: ProcessId,
    target: Temperature,
) -> TestHeatingResolution {
    let resources = add_test_heating_resources(registries, state);
    resolve_test_heating(registries, state, process, source, resources, target)
}

fn commit_process_for_test(token: ValidatedStartProcess, state: &mut AppState) -> ProductionJobId {
    match token.commit(state) {
        Ok(job) => job,
        Err(error) => panic!("validated process commit failed: {error}"),
    }
}

fn add_test_stockpile(state: &mut AppState, capacity: u64) -> StockpileId {
    match add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    }
}

fn deposit_test_wood(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    mass: u64,
) {
    if let Err(error) = deposit_bulk_for_test(
        registries,
        state,
        stockpile,
        wood_log(),
        Mass::from_milligrams(mass),
    ) {
        panic!("fixture deposit failed: {error}");
    }
}

fn unstarted_process_fixture() -> (Registries, AppState, StockpileId, StockpileId) {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    (registries, state, source, destination)
}

fn carried_output_process_fixture() -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    TestHeatingResolution,
    VoxelCoord,
) {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    let step_destination = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, origin, step_destination);
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| {
            panic!("carried-output production player admission failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("carried-output production player admission commit failed: {error}")
        });
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("carried-output production player disappeared"));
    let source = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    validate_place_ground_stockpile(&state, source, origin)
        .unwrap_or_else(|error| {
            panic!("carried-output production source placement failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("carried-output production source placement commit failed: {error}")
        });
    let resources = add_test_heating_resources(&registries, &mut state);
    let equipment_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        equipment_revision,
        equipment_revision + 1,
        resources.equipment,
        origin,
    );
    let energy_revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        energy_revision,
        energy_revision + 1,
        resources.energy,
        origin,
    );
    let resolved = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    (
        registries,
        state,
        source,
        carried,
        resolved,
        step_destination,
    )
}

#[test]
fn player_step_rejects_moving_a_retained_production_output_destination() {
    let (registries, mut state, source, carried, resolved, step_destination) =
        carried_output_process_fixture();
    let job = validate_start_process(&registries, &state, &resolved, source, carried)
        .unwrap_or_else(|error| panic!("carried-output production start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("carried-output production commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();

    assert_eq!(
        validate_start_player_step(&registries, &state, step_destination).err(),
        Some(PlayerStepError::ProductionSiteConflict {
            stockpile: carried,
            job,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_player_step_rechecks_production_site_before_commit() {
    let (registries, mut state, source, carried, resolved, step_destination) =
        carried_output_process_fixture();
    let step = validate_start_player_step(&registries, &state, step_destination)
        .unwrap_or_else(|error| panic!("production-site race step validation failed: {error}"));
    let job = validate_start_process(&registries, &state, &resolved, source, carried)
        .unwrap_or_else(|error| panic!("production-site race process validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production-site race process commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        step.commit(&mut state),
        Err(PlayerStepCommitError::ProductionSiteConflict {
            stockpile: carried,
            job,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn process_start_rejects_carried_output_destination_during_player_step() {
    let (registries, mut state, source, carried, resolved, step_destination) =
        carried_output_process_fixture();
    validate_start_player_step(&registries, &state, step_destination)
        .unwrap_or_else(|error| panic!("in-transit output step validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("in-transit output step commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolved, source, carried).err(),
        Some(StartProcessError::OutputDestinationInTransit { stockpile: carried })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_process_start_rechecks_carried_output_transit_before_commit() {
    let (registries, mut state, source, carried, resolved, step_destination) =
        carried_output_process_fixture();
    let process = validate_start_process(&registries, &state, &resolved, source, carried)
        .unwrap_or_else(|error| panic!("in-transit commit process validation failed: {error}"));
    validate_start_player_step(&registries, &state, step_destination)
        .unwrap_or_else(|error| panic!("in-transit commit step validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("in-transit commit step commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        process.commit(&mut state),
        Err(StartProcessCommitError::OutputDestinationInTransit { stockpile: carried })
    );
    assert_eq!(state, before);
}

#[test]
fn process_start_rejects_separated_known_endpoints() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resources = add_test_heating_resources(&registries, &mut state);
    let resolved = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    let source_position = VoxelCoord::new(0, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("production-site source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production-site source placement commit failed: {error}"));
    let energy_position = VoxelCoord::new(1, 0, 0);
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        resources.energy,
        energy_position,
    );
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolved, source, destination,).err(),
        Some(StartProcessError::SpatialEndpointMismatch {
            first: ProductionSiteEndpoint::Source(source),
            first_position: source_position,
            second: ProductionSiteEndpoint::EnergyStore(resources.energy),
            second_position: energy_position,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn player_process_start_rejects_remote_co_located_process_site() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resources = add_test_heating_resources(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    let process_position = VoxelCoord::new(1, 0, 0);
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&state, stockpile, process_position)
            .unwrap_or_else(|error| panic!("remote process stockpile placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("remote process stockpile placement commit failed: {error}")
            });
    }
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        resources.equipment,
        process_position,
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        revision,
        revision + 1,
        resources.energy,
        process_position,
    );
    validate_admit_player(&registries, &state, player_position)
        .unwrap_or_else(|error| panic!("remote process player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote process player admission commit failed: {error}"));
    let resolved = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );

    assert!(
        validate_start_process(&registries, &state, &resolved, source, destination).is_ok(),
        "capability-level production admission should remain independent of player locality"
    );
    let before = state.clone();
    assert_eq!(
        validate_start_player_process(&registries, &state, &resolved, source, destination).err(),
        Some(StartPlayerProcessError::StockpileAccess(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: process_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn process_start_token_rejects_logistics_change_before_commit() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolved = make_test_resolution(&registries, &mut state, source);
    let validated = validate_start_process(&registries, &state, &resolved, source, destination)
        .unwrap_or_else(|error| panic!("stale production-site validation failed: {error}"));
    let expected = state.logistics().revision();
    validate_place_ground_stockpile(&state, source, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("stale production-site placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale production-site placement commit failed: {error}"));
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(StartProcessCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_running_process_with_separated_active_endpoints() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resources = add_test_heating_resources(&registries, &mut state);
    let resolved = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    let position = VoxelCoord::new(0, 0, 0);
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&state, stockpile, position)
            .unwrap_or_else(|error| panic!("running production-site placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("running production-site placement commit failed: {error}")
            });
    }
    let equipment_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        equipment_revision,
        equipment_revision + 1,
        resources.equipment,
        position,
    );
    let energy_revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        energy_revision,
        energy_revision + 1,
        resources.energy,
        position,
    );
    let job = validate_start_process(&registries, &state, &resolved, source, destination)
        .unwrap_or_else(|error| panic!("running production-site validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("running production-site commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let remote_position = VoxelCoord::new(1, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("running production-site serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["equipment_locations"]
        [resources.equipment.value().to_string()] = serde_json::json!({"x": 1, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("running production-site decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobSpatialEndpointMismatch {
                job,
                first: ProductionSiteEndpoint::Equipment(resources.equipment),
                first_position: remote_position,
                second: ProductionSiteEndpoint::Destination(destination),
                second_position: position,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_suspended_process_with_separated_retained_endpoints() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resources = add_test_heating_resources(&registries, &mut state);
    let position = VoxelCoord::new(0, 0, 0);
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&state, stockpile, position)
            .unwrap_or_else(|error| panic!("suspended production-site placement failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("suspended production-site placement commit failed: {error}")
            });
    }
    let equipment_revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        equipment_revision,
        equipment_revision + 1,
        resources.equipment,
        position,
    );
    let energy_revision = state.logistics().revision();
    state.logistics_state_mut().apply_energy_store_placement(
        energy_revision,
        energy_revision + 1,
        resources.energy,
        position,
    );
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("suspended production-site mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended production-site mount commit failed: {error}"));
    let resolved = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    let job = validate_start_process(&registries, &state, &resolved, source, destination)
        .unwrap_or_else(|error| panic!("suspended production-site validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended production-site commit failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("suspended production-site overload failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended production-site overload commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended production-site transition failed: {error}"));
    assert!(
        state
            .production()
            .get_job(job)
            .is_some_and(ProductionJobRecord::is_suspended)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let remote_position = VoxelCoord::new(1, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended production-site serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["equipment_locations"]
        [resources.equipment.value().to_string()] = serde_json::json!({"x": 1, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("suspended production-site decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobSpatialEndpointMismatch {
                job,
                first: ProductionSiteEndpoint::Equipment(resources.equipment),
                first_position: remote_position,
                second: ProductionSiteEndpoint::Destination(destination),
                second_position: position,
            }
        ))
    );
}

fn add_active_stockpile_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        .unwrap_or_else(|error| panic!("production support bounds failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("production support fixture failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("production support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("production support activation commit failed: {error}"));
    support
}

#[path = "production_execution_tests/headroom.rs"]
mod headroom;

#[path = "production_execution_tests/lifecycle.rs"]
mod lifecycle;

#[path = "production_execution_tests/routing.rs"]
mod routing;

#[path = "production_execution_tests/completion.rs"]
mod completion;
