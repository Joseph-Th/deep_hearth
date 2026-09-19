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
use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::energy::{EnergyStoreId, add_energy_store_with_initial_for_fixture};
use crate::equipment::{EquipmentId, add_equipment};
use crate::inventory::{
    MaterialFixtureError, MaterialIngressError, MaterialLotSelection, MaterialTransferError,
    StockpileId, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_bulk_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
    validate_material_transfer_for_test, validate_mount_stockpile,
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
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolution, ProcessResolutionError, ProductionJobId,
    ProductionJobRecord, ProductionValidationError, validate_process_inputs,
};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::survival::{FoodFreshness, assess_food_freshness};
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

fn berry_food() -> CommodityKey {
    CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
}

fn bind_source_mass(
    registries: &Registries,
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
        registries,
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
    let mut state = AppState::new(WorldSeed::new(0x9000_E001));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    (registries, state, source, destination)
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

#[test]
fn process_start_rejects_exhausted_job_id_without_consuming_material() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production job-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["next_job_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production job-id exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production job-id exhaustion fixture should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::JobIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_future_material_lot_identity_capacity() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production lot-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-id exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle exhausted lot-id fixture should load before production admission: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::MaterialLotIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_running_production_without_future_material_lot_identity_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production lot-budget validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production lot-budget serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMaterialLotIdCapacityExhausted {
                next_lot_id: u64::MAX,
                required: 1,
            }
        ))
    );
}

#[test]
fn later_inventory_ingress_cannot_consume_identity_reserved_for_running_production() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_E006));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    let unrelated_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production lot-reservation serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-reservation decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("near-exhausted lot-id fixture should load before production admission: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let token = validate_start_process(&registries, &loaded, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("near-exhausted production admission failed: {error}"));
    let _ = commit_process_for_test(token, &mut loaded);
    let before = loaded.clone();

    assert_eq!(
        deposit_lot_for_test(
            &registries,
            &mut loaded,
            unrelated_destination,
            wood_log(),
            Mass::from_milligrams(1),
            Temperature::from_millikelvin(293_150),
        ),
        Err(MaterialFixtureError::Ingress(
            MaterialIngressError::LotIdExhausted
        ))
    );
    assert_eq!(loaded, before);
}

#[test]
fn production_completion_can_consume_the_last_reserved_material_lot_identity() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("last lot-id serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("last lot-id decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("last lot-id fixture should load: {error}"));
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let token = validate_start_process(&registries, &loaded, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("last lot-id production admission failed: {error}"));
    let job = commit_process_for_test(token, &mut loaded);

    while loaded.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("last lot-id production completion failed: {error}"));
    }

    assert_eq!(loaded.inventory().next_lot_id(), u64::MAX);
    assert_eq!(loaded.checked_future_material_lot_id_demand(), Some(0));
    assert_eq!(loaded.inventory().lot_ids(destination).count(), 1);
    validate_loaded_state(&registries, &loaded)
        .unwrap_or_else(|error| panic!("last lot-id completed state failed validation: {error}"));
}

#[test]
fn process_start_rejects_exhausted_production_revision_without_consuming_material() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production revision exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production revision exhaustion fixture should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::ProductionRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_revision_capacity_for_admission_and_completion() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production revision-budget serialization failed: {error}"));

    for (owner, expected) in [
        ("production", StartProcessError::ProductionRevisionExhausted),
        ("inventory", StartProcessError::InventoryRevisionExhausted),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX - 1);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("production revision-budget decode failed: {error}"));
        let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
            panic!("idle near-exhausted production owner should load: {error}")
        });
        let resolution = make_test_resolution(&registries, &mut loaded, source);
        let before = loaded.clone();

        assert_eq!(
            validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
            Some(expected)
        );
        assert_eq!(loaded, before);
    }
}

#[test]
fn trusted_load_rejects_running_production_without_scheduled_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production load-budget validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production load-budget serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::ScheduledRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_running_production_without_inventory_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| {
            panic!("production inventory load-budget validation failed: {error}")
        });
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production inventory load-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production inventory load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionInventoryRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_inventory_revisions_owed_to_existing_due_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_E002));
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, first_source);
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production inventory headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 2);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production inventory headroom decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one scheduled completion must fit at inventory revision MAX-2: {error}")
    });
    let second_resolution = make_test_resolution(&registries, &mut loaded, second_source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn unrelated_inventory_transfer_cannot_spend_revision_owed_to_running_production() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let unrelated_source = add_test_stockpile(&mut state, 100);
    let unrelated_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, unrelated_source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("revision-theft production validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("revision-theft serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("revision-theft decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one production completion must fit at inventory revision MAX-1: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_material_transfer_for_test(
            &registries,
            &loaded,
            unrelated_source,
            unrelated_destination,
            wood_log(),
            Mass::from_milligrams(1),
        )
        .err(),
        Some(MaterialTransferError::RevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_running_production_without_equipment_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| {
            panic!("production equipment load-budget validation failed: {error}")
        });
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production equipment load-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production equipment load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionEquipmentRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_equipment_revisions_owed_to_existing_wear_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_E003));
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);
    let first_resources = add_test_heating_resources(&registries, &mut state);
    let second_resources = add_test_heating_resources(&registries, &mut state);

    let first_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        first_source,
        first_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing wear production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing wear production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing wear production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first wear-bearing production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production equipment headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production equipment headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one scheduled wear completion must fit at equipment revision MAX-1: {error}")
    });
    let second_resolution = resolve_test_heating(
        &registries,
        &loaded,
        TEST_PROCESS,
        second_source,
        second_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::EquipmentRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_completion_structure_revision_for_supported_output() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("production destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production destination mount commit failed: {error}"));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production structure-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production structure-budget decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle exhausted production structure owner should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::StructureRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_supported_output_without_structure_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("production supported-load mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production supported-load mount commit failed: {error}"));
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production supported-load validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production supported-load serialization failed: {error}"));
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production supported-load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionStructureRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_structure_revisions_owed_to_existing_supported_output_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_E004));
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);
    let first_support = add_active_stockpile_support(&registries, &mut state, 0);
    let second_support = add_active_stockpile_support(&registries, &mut state, 2);
    let _ = validate_mount_stockpile(&registries, &state, first_destination, first_support)
        .unwrap_or_else(|error| panic!("first supported destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first supported destination mount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, second_destination, second_support)
        .unwrap_or_else(|error| panic!("second supported destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("second supported destination mount commit failed: {error}")
        });
    let first_resources = add_test_heating_resources(&registries, &mut state);
    let second_resources = add_test_heating_resources(&registries, &mut state);

    let first_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        first_source,
        first_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing supported production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing supported production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing supported production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first supported production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production structure headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production structure headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one supported completion must fit at structural revision MAX-1: {error}")
    });
    let second_resolution = resolve_test_heating(
        &registries,
        &loaded,
        TEST_PROCESS,
        second_source,
        second_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::StructureRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_consumes_inputs_reserves_capacity_and_completes_on_due_tick() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(10));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let duration = resolution.duration();

    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };
    let job = commit_process_for_test(token, &mut state);

    let source_record = match state.inventory().get_stockpile(source) {
        Some(record) => record,
        None => panic!("source disappeared"),
    };
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(
        source_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    assert_eq!(
        destination_record.reserved_inbound(),
        Mass::from_milligrams(10)
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(ProductionJobRecord::completes_at),
        Some(SimulationTick::new(duration.value()))
    );

    for expected_tick in 1..duration.value() {
        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("tick failed: {error}"),
        };
        assert_eq!(outcome.tick(), SimulationTick::new(expected_tick));
        assert!(outcome.production_completions().is_empty());
    }

    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("completion tick failed: {error}"),
    };
    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert!(state.production().get_job(job).is_none());
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    let output_lots: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(output_lots.len(), 1);
    let output_lot = match state.inventory().get_lot(output_lots[0]) {
        Some(lot) => lot,
        None => panic!("completed output lot disappeared"),
    };
    assert_eq!(output_lot.temperature(), TEST_TARGET_TEMPERATURE);
    assert_eq!(
        output_lot.created_at(),
        SimulationTick::new(duration.value())
    );
}

#[test]
fn production_preserves_input_storage_exposure_and_ages_work_in_process() {
    let registries = make_test_registries_with_standard_sensible_heating(TEST_PERISHABLE_PROCESS);
    let mut state = AppState::new(WorldSeed::new(0x9000_0003));
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("perishable source profile failed: {error}"));
    let source = add_stockpile(&mut state, Mass::from_milligrams(20), preserved_profile)
        .unwrap_or_else(|error| panic!("perishable source stockpile failed: {error}"));
    let destination = add_test_stockpile(&mut state, 20);
    let input_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        berry_food(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("perishable input deposit failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(6));
    assert_eq!(
        assess_food_freshness(&registries, &state, input_lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(2),
            remaining: TickSpan::new(287_994),
        })
    );

    let resolution = make_resolution_for_process(
        &registries,
        &mut state,
        source,
        TEST_PERISHABLE_PROCESS,
        Temperature::from_millikelvin(500_000),
    );
    let duration = resolution.duration();
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("perishable process start failed: {error}"));
    commit_process_for_test(token, &mut state);

    for _ in 0..duration.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("perishable process tick failed: {error}"));
    }

    let output_lot = state
        .inventory()
        .lot_ids(destination)
        .next()
        .unwrap_or_else(|| panic!("perishable process output lot disappeared"));
    let output = state
        .inventory()
        .get_lot(output_lot)
        .unwrap_or_else(|| panic!("perishable process output record disappeared"));
    assert_eq!(
        output.created_at(),
        SimulationTick::new(6 + duration.value())
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, output_lot),
        Ok(FoodFreshness::Fresh { age, .. })
            if age == TickSpan::new(2 + duration.value())
    ));
}

#[test]
fn persisted_production_storage_history_must_be_rebased_to_job_start() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0004));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("storage-history validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("storage-history validation tick failed: {error}"));

    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage-history fixture serialization failed: {error}"));
    let mut transition_tampered = encoded.clone();
    transition_tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["material_storage_history"]["last_transition_at"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(transition_tampered)
        .unwrap_or_else(|error| panic!("storage-history tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::StorageHistoryTransitionMismatch {
                job,
                transition: SimulationTick::new(1),
                started_at: SimulationTick::new(0),
            }
        )))
    );

    let mut age_tampered = encoded;
    age_tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["material_storage_history"]["ambient_age_parts"] = serde_json::json!(u64::MAX);
    let serialized = serde_json::to_string(&age_tampered)
        .unwrap_or_else(|error| panic!("storage-age tamper serialization failed: {error}"));
    let sentinel = format!("\"ambient_age_parts\":{}", u64::MAX);
    let overflow = format!("\"ambient_age_parts\":{}", u128::MAX);
    assert_eq!(serialized.matches(&sentinel).count(), 1);
    let overflowed = serialized.replacen(&sentinel, &overflow, 1);
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&overflowed).is_err());
}

#[test]
fn production_started_after_time_elapsed_rebases_storage_history_to_ownership_boundary() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0017));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    apply_clock_advance(&mut state, SimulationTick::new(5));
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("later production start failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("later-start production job disappeared"));

    assert_eq!(record.started_at(), SimulationTick::new(5));
    assert_eq!(
        record.material_storage_history().last_transition_at(),
        record.started_at(),
        "production custody must checkpoint storage exposure exactly at its ownership boundary"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn persisted_production_job_cannot_start_in_the_future() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0005));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("future-start validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("future-start fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["started_at"] =
        serde_json::json!(1_u64);
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["material_storage_history"]
        ["last_transition_at"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("future-start tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::JobStartedInFuture {
                job,
                started_at: SimulationTick::new(1),
                current: SimulationTick::ZERO,
            }
        )))
    );
}

#[test]
fn persisted_running_production_job_cannot_already_be_due() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0006));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("already-due validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    apply_clock_advance(&mut state, SimulationTick::new(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("already-due fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("already-due tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::RunningJobAlreadyDue {
                job,
                due: SimulationTick::new(1),
                current: SimulationTick::new(1),
            }
        )))
    );
}

#[test]
fn persisted_running_production_job_cannot_complete_before_active_duration() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0016));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let duration = resolution.duration();
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("early-due validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    apply_clock_advance(&mut state, SimulationTick::new(1));

    let expected_due = SimulationTick::new(duration.value());
    let forged_due = SimulationTick::new(
        duration
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("heating fixture duration must exceed zero")),
    );
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("early-due fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(forged_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("early-due tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::CompletionScheduleMismatch {
                job,
                expected_due,
                actual_due: forged_due,
            }
        )))
    );
}

#[test]
fn persisted_heating_job_rejects_consumed_state_hotter_than_committed_target() {
    let input = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let registries = make_test_registries_with_standard_sensible_heating(TEST_COMPOSITION_PROCESS);
    let mut state = AppState::new(WorldSeed::new(0x9000_0007));
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 20);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        input,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("phase-validation input fixture failed: {error}"));
    let resolution = make_resolution_for_process(
        &registries,
        &mut state,
        source,
        TEST_COMPOSITION_PROCESS,
        TEST_TARGET_TEMPERATURE,
    );
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("phase-validation process start failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|definition| definition.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("copper fixture lost its authored melting point"));
    let invalid_temperature = Temperature::from_millikelvin(
        melting_point
            .millikelvin()
            .checked_add(1)
            .unwrap_or_else(|| panic!("copper melting point cannot exhaust temperature range")),
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("phase-validation fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_inputs"]
        [0]["profile"]["temperature"] = serde_json::json!(invalid_temperature.millikelvin());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("phase-validation tamper failed structural decode: {error}")
    });

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::TargetBelowInputTemperature {
                job,
                current: invalid_temperature,
                target: TEST_TARGET_TEMPERATURE,
            }
        )))
    );
}

#[test]
fn routed_output_streams_reserve_and_complete_by_identity_not_route_order() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_001));
    let source = add_test_stockpile(&mut state, 20);
    let undersize_destination = add_test_stockpile(&mut state, 10);
    let oversize_destination = add_test_stockpile(&mut state, 10);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let duration = resolution.duration();
    assert_eq!(
        resolution
            .output_streams()
            .iter()
            .map(|stream| stream.id())
            .collect::<Vec<_>>(),
        vec![
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            ScreeningProcessDefinition::OVERSIZE_STREAM,
        ]
    );

    let token = match validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
        ],
    ) {
        Ok(token) => token,
        Err(error) => panic!("multi-stream process validation failed: {error}"),
    };
    let job = commit_process_for_test(token, &mut state);

    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize_destination)
            .map(|record| record.available_capacity()),
        Some(Mass::from_milligrams(4)),
        "available capacity must include committed production output"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(4))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize_destination)
            .map(|record| record.available_capacity()),
        Some(Mass::from_milligrams(6)),
        "available capacity must distinguish each destination's reservation"
    );
    let stored_routes = match state.production().get_job(job) {
        Some(record) => record
            .output_streams()
            .iter()
            .map(|stream| (stream.id(), stream.destination()))
            .collect::<Vec<_>>(),
        None => panic!("multi-stream job disappeared after start"),
    };
    assert_eq!(
        stored_routes,
        vec![
            (
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
            (
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
        ]
    );
    if let Err(error) = validate_loaded_state(&registries, &state) {
        panic!("multi-stream running state failed validation: {error}");
    }
    let encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("multi-stream save serialization failed: {error}"),
    };
    let mut noncanonical = encoded.clone();
    let streams = match noncanonical["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"]
        .as_array_mut()
    {
        Some(streams) => streams,
        None => panic!("multi-stream save omitted production output streams"),
    };
    streams.reverse();
    let noncanonical: LoadedSaveEnvelope = match serde_json::from_value(noncanonical) {
        Ok(loaded) => loaded,
        Err(error) => {
            panic!("noncanonical multi-stream save failed structural decode: {error}")
        }
    };
    assert_eq!(
        noncanonical.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::NonCanonicalOutputStreamOrder { job }
        )))
    );

    let mut duplicate_stream = encoded.clone();
    let streams = duplicate_stream["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("multi-stream duplicate test lost output streams"));
    streams.push(streams[0].clone());
    let duplicate_stream: LoadedSaveEnvelope = serde_json::from_value(duplicate_stream)
        .unwrap_or_else(|error| {
            panic!("duplicate output-stream save failed structural decode: {error}")
        });
    assert_eq!(
        duplicate_stream.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::DuplicateOutputStreamId {
                job,
                stream: ScreeningProcessDefinition::UNDERSIZE_STREAM,
            }
        )))
    );

    let mut duplicate_output = encoded.clone();
    let outputs = duplicate_output["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"][0]["outputs"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("multi-stream duplicate test lost stream outputs"));
    outputs.push(outputs[0].clone());
    let duplicate_output: LoadedSaveEnvelope = serde_json::from_value(duplicate_output)
        .unwrap_or_else(|error| {
            panic!("duplicate output-spec save failed structural decode: {error}")
        });
    assert_eq!(
        duplicate_output.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::DuplicateOutputSpecification { job }
        )))
    );

    let loaded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(loaded) => loaded,
        Err(error) => panic!("multi-stream save deserialization failed: {error}"),
    };
    let restored = match loaded.into_state(&registries) {
        Ok(restored) => restored,
        Err(error) => panic!("multi-stream save validation failed: {error}"),
    };
    assert_eq!(restored, state);

    for _ in 1..duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("multi-stream pre-completion tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("multi-stream completion tick failed: {error}"),
    };
    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert_eq!(
        outcome.production_completions()[0].routes(),
        [
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
        ]
    );
    let completion = &outcome.production_completions()[0];
    assert_eq!(completion.landings().len(), 2);
    assert_eq!(
        completion.landings()[0].stream(),
        ScreeningProcessDefinition::UNDERSIZE_STREAM
    );
    assert_eq!(
        completion.landings()[0].destination(),
        undersize_destination
    );
    assert_eq!(completion.landings()[0].parcels().len(), 1);
    assert_eq!(
        completion.landings()[1].stream(),
        ScreeningProcessDefinition::OVERSIZE_STREAM
    );
    assert_eq!(completion.landings()[1].destination(), oversize_destination);
    assert_eq!(completion.landings()[1].parcels().len(), 1);
    let undersize_parcel = &completion.landings()[0].parcels()[0];
    assert_eq!(
        undersize_parcel.output().commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(undersize_parcel.output().mass(), Mass::from_milligrams(6));
    let undersize_landing = state
        .inventory()
        .get_lot(undersize_parcel.lot())
        .unwrap_or_else(|| panic!("undersize completion landing disappeared"));
    assert_eq!(undersize_landing.stockpile(), undersize_destination);
    let oversize_parcel = &completion.landings()[1].parcels()[0];
    assert_eq!(
        oversize_parcel.output().commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(oversize_parcel.output().mass(), Mass::from_milligrams(4));
    let oversize_landing = state
        .inventory()
        .get_lot(oversize_parcel.lot())
        .unwrap_or_else(|| panic!("oversize completion landing disappeared"));
    assert_eq!(oversize_landing.stockpile(), oversize_destination);
    let undersize_record = match state.inventory().get_stockpile(undersize_destination) {
        Some(record) => record,
        None => panic!("undersize destination disappeared"),
    };
    assert_eq!(undersize_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        undersize_record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(6)
    );
    assert_eq!(
        undersize_record.available_capacity(),
        Mass::from_milligrams(4),
        "completion must replace reserved capacity with stored matter without changing free capacity"
    );
    let oversize_record = match state.inventory().get_stockpile(oversize_destination) {
        Some(record) => record,
        None => panic!("oversize destination disappeared"),
    };
    assert_eq!(oversize_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        oversize_record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(4)
    );
    assert_eq!(
        oversize_record.available_capacity(),
        Mass::from_milligrams(6)
    );
}

#[test]
fn duplicate_output_route_is_rejected_atomically() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_002));
    let source = add_test_stockpile(&mut state, 20);
    let first_destination = add_test_stockpile(&mut state, 10);
    let second_destination = add_test_stockpile(&mut state, 10);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                first_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                second_destination,
            ),
        ],
    );

    assert_eq!(
        result,
        Err(StartProcessError::DuplicateOutputRoute {
            stream: ScreeningProcessDefinition::UNDERSIZE_STREAM,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn shared_destination_capacity_is_checked_against_aggregate_stream_mass() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_003));
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 9);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, destination),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, destination),
        ],
    );

    assert_eq!(
        result,
        Err(StartProcessError::CapacityExceeded {
            stockpile: destination,
            capacity: Mass::from_milligrams(9),
            committed_after_consumption: Mass::ZERO,
            requested_inbound: Mass::from_milligrams(10),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn failed_process_start_is_atomic() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(11));
    let source = add_test_stockpile(&mut state, 100);
    add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 5);
    let before = state.clone();

    let lot = state
        .inventory()
        .lot_ids(source)
        .next()
        .unwrap_or_else(|| panic!("atomicity fixture lost its material lot"));
    let result = validate_process_inputs(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    );

    assert!(matches!(
        result,
        Err(ProcessInputError::InsufficientSelectedLotMass {
            lot: _lot,
            available: _available,
            requested: _requested,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn process_start_rejects_resolution_that_bypasses_registered_resource_topology() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0014));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let inputs = bind_source_mass(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        Mass::from_milligrams(10),
    );
    let resolution = inputs
        .resolve_without_resources(
            TickSpan::new(1),
            vec![MaterialLotSpec::new(
                wood_log(),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        )
        .unwrap_or_else(|error| panic!("topology-bypass fixture resolution failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::ResolutionEnergyTopologyMismatch {
            process: TEST_PROCESS,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_process_cannot_create_or_destroy_unaccounted_matter() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(111));
    let source = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let inputs = bind_source_mass(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        Mass::from_milligrams(10),
    );
    let before = state.clone();

    let result = inputs.resolve_without_resources(
        TickSpan::new(3),
        vec![MaterialLotSpec::new(
            wood_log(),
            Mass::from_milligrams(9),
            Temperature::from_millikelvin(600_000),
        )],
    );

    assert!(matches!(
        result,
        Err(ProcessResolutionError::MatterBalanceMismatch {
            input_mass,
            output_mass,
        }) if input_mass == Mass::from_milligrams(10)
            && output_mass == Mass::from_milligrams(9)
    ));
    assert_eq!(state, before);
}

#[test]
fn reserved_output_capacity_cannot_be_taken_by_later_deposits() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(12));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 12);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    let result = deposit_bulk_for_test(
        &registries,
        &mut state,
        destination,
        wood_log(),
        Mass::from_milligrams(3),
    );

    assert!(matches!(
        result,
        Err(crate::inventory::MaterialFixtureError::Ingress(
            crate::inventory::MaterialIngressError::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            }
        ))
    ));
}

#[test]
fn same_stockpile_process_accounts_for_consumed_space_before_reserving_output() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(13));
    let stockpile = add_test_stockpile(&mut state, 10);
    deposit_test_wood(&registries, &mut state, stockpile, 10);
    let resolution = make_test_resolution(&registries, &mut state, stockpile);

    let token = match validate_start_process(&registries, &state, &resolution, stockpile, stockpile)
    {
        Ok(token) => token,
        Err(error) => panic!("same-stockpile process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    let record = match state.inventory().get_stockpile(stockpile) {
        Some(record) => record,
        None => panic!("stockpile disappeared"),
    };
    assert_eq!(record.stored_mass(), Mass::ZERO);
    assert_eq!(record.reserved_inbound(), Mass::from_milligrams(10));
}

#[test]
fn same_tick_completions_are_emitted_in_stable_job_id_order() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(14));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, source);
    let duration = first_resolution.duration();
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => commit_process_for_test(token, &mut state),
            Err(error) => panic!("first process validation failed: {error}"),
        };
    let second_resolution = make_test_resolution(&registries, &mut state, source);
    let second = match validate_start_process(
        &registries,
        &state,
        &second_resolution,
        source,
        destination,
    ) {
        Ok(token) => commit_process_for_test(token, &mut state),
        Err(error) => panic!("second process validation failed: {error}"),
    };
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(20)),
        "two same-tick jobs must reserve their shared destination cumulatively"
    );
    let inventory_revision_before_completion = state.inventory().revision();
    for _ in 1..duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("pre-completion tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("completion tick failed: {error}"),
    };
    let completed: Vec<_> = outcome
        .production_completions()
        .iter()
        .map(|completion| completion.job())
        .collect();
    assert_eq!(completed, vec![first, second]);
    assert_eq!(
        state.inventory().revision(),
        inventory_revision_before_completion + 1,
        "one completion batch must apply all shared-destination deposits under one inventory revision"
    );
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("shared completion destination disappeared"));
    assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(20));
    assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn compatible_nonperishable_production_outputs_coalesce_and_preserve_provenance_range() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(141));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, source);
    let duration = first_resolution.duration();
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => token,
            Err(error) => panic!("first process validation failed: {error}"),
        };
    commit_process_for_test(first, &mut state);
    let mut first_outcome = None;
    for _ in 0..duration.value() {
        first_outcome = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("first completion failed: {error}")),
        );
    }
    let first_outcome =
        first_outcome.unwrap_or_else(|| panic!("first production resolution had zero duration"));
    let first_parcel = &first_outcome.production_completions()[0].landings()[0].parcels()[0];
    assert_eq!(first_parcel.output().mass(), Mass::from_milligrams(10));
    let surviving_lot = first_parcel.lot();

    let second_resolution = make_test_resolution(&registries, &mut state, source);
    let second = match validate_start_process(
        &registries,
        &state,
        &second_resolution,
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("second process validation failed: {error}"),
    };
    commit_process_for_test(second, &mut state);
    let mut second_outcome = None;
    for _ in 0..duration.value() {
        second_outcome = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("second completion failed: {error}")),
        );
    }
    let second_outcome =
        second_outcome.unwrap_or_else(|| panic!("second production resolution had zero duration"));
    let second_parcel = &second_outcome.production_completions()[0].landings()[0].parcels()[0];
    assert_eq!(second_parcel.output().mass(), Mass::from_milligrams(10));
    assert_eq!(
        second_parcel.lot(),
        surviving_lot,
        "coalesced production receipt must retain the pre-existing surviving lot identity"
    );

    let lot_ids: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(lot_ids.len(), 1);
    assert_eq!(lot_ids[0], surviving_lot);
    let lot = state
        .inventory()
        .get_lot(lot_ids[0])
        .unwrap_or_else(|| panic!("coalesced production output disappeared"));
    assert_eq!(lot.mass(), Mass::from_milligrams(20));
    assert_eq!(lot.created_at(), SimulationTick::new(duration.value()));
    assert_eq!(
        lot.latest_created_at(),
        SimulationTick::new(duration.value() * 2)
    );
}

#[test]
fn resolution_source_mismatch_is_rejected_before_any_start_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(1415));
    let source = add_test_stockpile(&mut state, 100);
    let other_source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    deposit_test_wood(&registries, &mut state, other_source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, other_source, destination,),
        Err(StartProcessError::ResolutionSourceMismatch {
            bound: source,
            requested: other_source,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_inputs_become_stale_after_inventory_changes_before_start_validation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(1416));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let expected_revision = state.inventory().revision();
    add_test_stockpile(&mut state, 1);
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::StaleResolvedInputs {
            expected_inventory_revision: expected_revision,
            actual_inventory_revision: expected_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn stale_inventory_revision_rejects_validated_process_without_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(15));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };

    add_test_stockpile(&mut state, 1);
    let before_commit = state.clone();
    let result = token.commit(&mut state);

    assert!(matches!(
        result,
        Err(StartProcessCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_commit);
}

#[test]
fn stale_production_revision_rejects_second_validated_token_without_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(16));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 30);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let stale = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("first process validation failed: {error}"),
    };
    let winner = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("second process validation failed: {error}"),
    };
    commit_process_for_test(winner, &mut state);
    let before_stale_commit = state.clone();

    let result = stale.commit(&mut state);

    assert!(matches!(
        result,
        Err(StartProcessCommitError::StaleProductionRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_stale_commit);
}

#[test]
fn in_flight_job_uses_committed_output_snapshot_after_later_resolution_differs() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(17));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resources = add_test_heating_resources(&registries, &mut state);
    let resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    let later_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        Temperature::from_millikelvin(1_000_000),
    );
    assert_ne!(resolution.outputs(), later_resolution.outputs());
    let duration = resolution.duration();
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("original process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion after later resolution change failed: {error}");
        }
    }

    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    let lot_id = match state.inventory().lot_ids(destination).next() {
        Some(id) => id,
        None => panic!("committed output lot is missing"),
    };
    let lot = match state.inventory().get_lot(lot_id) {
        Some(lot) => lot,
        None => panic!("committed output lot record is missing"),
    };
    assert_eq!(lot.temperature(), TEST_TARGET_TEMPERATURE);
}
