//! Focused tests for production admission, completion, routing, and conservation semantics.

use super::*;
use crate::content::{
    FORM_FOOD, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_ORE, MATERIAL_BERRIES,
    MATERIAL_CHARCOAL, MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_WOOD,
    make_test_registries_with_process,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::inventory::{
    StockpileId, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_bulk_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::material::{
    CommodityKey, CompositionComponent, CompositionConstraint, MaterialComposition,
    MaterialInputSpec, MaterialLotSpec, MaterialPhaseStateError,
};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessDefinition, ProcessId, ProcessInputError, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ProductionJobId, ProductionJobRecord, ProductionValidationError,
    make_test_process_resolution, make_test_process_resolution_with_streams,
    validate_process_inputs,
};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::survival::{FoodFreshness, assess_food_freshness};

const TEST_PROCESS: ProcessId = ProcessId::new(900_001);
const TEST_COMPOSITION_PROCESS: ProcessId = ProcessId::new(900_002);
const TEST_PERISHABLE_PROCESS: ProcessId = ProcessId::new(900_003);

fn wood_log() -> CommodityKey {
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
}

fn make_test_perishable_process() -> ProcessDefinition {
    ProcessDefinition::new(
        TEST_PERISHABLE_PROCESS,
        "test perishable handling",
        vec![MaterialInputSpec::new(
            berry_food(),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    )
}

fn charcoal_lump() -> CommodityKey {
    CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP)
}

fn slag_lump() -> CommodityKey {
    CommodityKey::new(MATERIAL_SLAG, FORM_LUMP)
}

fn berry_food() -> CommodityKey {
    CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)
}

fn copper_ore() -> CommodityKey {
    CommodityKey::new(MATERIAL_COPPER, FORM_ORE)
}

fn native_copper() -> CommodityKey {
    CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
}

fn make_copper_slag_composition(copper_parts_per_million: u32) -> MaterialComposition {
    let slag_parts_per_million = 1_000_000_u32 - copper_parts_per_million;
    match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, copper_parts_per_million),
        CompositionComponent::new(MATERIAL_SLAG, slag_parts_per_million),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    }
}

fn minimum_copper_constraint(minimum: u32) -> CompositionConstraint {
    match CompositionConstraint::new(MATERIAL_COPPER, minimum, 1_000_000) {
        Ok(constraint) => constraint,
        Err(error) => panic!("constraint fixture failed: {error}"),
    }
}

fn make_test_process() -> ProcessDefinition {
    ProcessDefinition::new(
        TEST_PROCESS,
        "test mass conversion",
        vec![MaterialInputSpec::new(
            wood_log(),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    )
}

fn make_test_multi_stream_resolution(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    duration_ticks: u64,
) -> ProcessResolution {
    let inputs = match validate_process_inputs(registries, state, TEST_PROCESS, source) {
        Ok(inputs) => inputs,
        Err(error) => panic!("multi-stream input binding failed: {error}"),
    };
    make_test_process_resolution_with_streams(
        inputs,
        duration_ticks,
        vec![
            (
                ProcessOutputStreamId::new(20),
                vec![MaterialLotSpec::new(
                    slag_lump(),
                    Mass::from_milligrams(4),
                    Temperature::from_millikelvin(600_000),
                )],
            ),
            (
                ProcessOutputStreamId::new(10),
                vec![MaterialLotSpec::new(
                    charcoal_lump(),
                    Mass::from_milligrams(6),
                    Temperature::from_millikelvin(600_000),
                )],
            ),
        ],
    )
}

fn make_test_registries() -> Registries {
    make_test_registries_with_process(make_test_process())
}

fn make_test_resolution(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    duration_ticks: u64,
) -> ProcessResolution {
    let inputs = match validate_process_inputs(registries, state, TEST_PROCESS, source) {
        Ok(inputs) => inputs,
        Err(error) => panic!("test process input binding failed: {error}"),
    };
    make_test_process_resolution(
        inputs,
        duration_ticks,
        vec![MaterialLotSpec::new(
            charcoal_lump(),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(600_000),
        )],
    )
}

fn make_resolution_for_process(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    process: ProcessId,
    duration_ticks: u64,
    outputs: Vec<MaterialLotSpec>,
) -> ProcessResolution {
    let inputs = match validate_process_inputs(registries, state, process, source) {
        Ok(inputs) => inputs,
        Err(error) => panic!("test process input binding failed: {error}"),
    };
    make_test_process_resolution(inputs, duration_ticks, outputs)
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
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production job-id exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();
    let resolution = make_test_resolution(&registries, &loaded, source, 3);

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::JobIdExhausted)
    );
    assert_eq!(loaded, before);
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
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production revision exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();
    let resolution = make_test_resolution(&registries, &loaded, source, 3);

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::ProductionRevisionExhausted)
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
    let resolution = make_test_resolution(&registries, &state, source, 3);

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
        Some(SimulationTick::new(3))
    );

    for expected_tick in 1..=2 {
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
        destination_record.get_mass(charcoal_lump()),
        Mass::from_milligrams(10)
    );
    let output_lots: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(output_lots.len(), 1);
    let output_lot = match state.inventory().get_lot(output_lots[0]) {
        Some(lot) => lot,
        None => panic!("completed output lot disappeared"),
    };
    assert_eq!(
        output_lot.temperature(),
        Temperature::from_millikelvin(600_000)
    );
    assert_eq!(output_lot.created_at(), SimulationTick::new(3));
}

#[test]
fn production_preserves_input_storage_exposure_and_ages_work_in_process() {
    let registries = make_test_registries_with_process(make_test_perishable_process());
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
        &state,
        source,
        TEST_PERISHABLE_PROCESS,
        3,
        vec![MaterialLotSpec::new(
            berry_food(),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(293_150),
        )],
    );
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("perishable process start failed: {error}"));
    commit_process_for_test(token, &mut state);

    for _ in 0..3 {
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
    assert_eq!(output.created_at(), SimulationTick::new(9));
    assert_eq!(
        assess_food_freshness(&registries, &state, output_lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(5),
            remaining: TickSpan::new(95_995),
        })
    );
}

#[test]
fn production_rejects_storage_exposure_that_would_overflow_before_completion() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0006));
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 20);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("storage-overflow input deposit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage-overflow fixture serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["ambient_age_parts"] = serde_json::json!(u64::MAX);
    let serialized = serde_json::to_string(&encoded)
        .unwrap_or_else(|error| panic!("storage-overflow fixture encoding failed: {error}"));
    let sentinel = format!("\"ambient_age_parts\":{}", u64::MAX);
    let near_limit = u128::MAX - 2_000_000;
    let replacement = format!("\"ambient_age_parts\":{near_limit}");
    assert_eq!(serialized.matches(&sentinel).count(), 1);
    let encoded = serialized.replacen(&sentinel, &replacement, 1);
    let loaded: LoadedSaveEnvelope = serde_json::from_str(&encoded)
        .unwrap_or_else(|error| panic!("storage-overflow fixture decode failed: {error}"));
    let loaded = loaded.into_state(&registries).unwrap_or_else(|error| {
        panic!("near-limit storage age should be valid at tick zero: {error}")
    });
    let resolution = make_test_resolution(&registries, &loaded, source, 3);

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination),
        Err(StartProcessError::InputStorageAgeOverflow { stockpile: source })
    );
}

#[test]
fn persisted_production_storage_history_must_be_rebased_to_job_start() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0004));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &state, source, 3);
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
    let tampered: LoadedSaveEnvelope = serde_json::from_str(&overflowed)
        .unwrap_or_else(|error| panic!("storage-age overflow failed structural decode: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::StorageHistoryOverflow {
                job,
                at: SimulationTick::new(1),
            }
        )))
    );
}

#[test]
fn persisted_production_job_cannot_start_in_the_future() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0005));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &state, source, 3);
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
    let resolution = make_test_resolution(&registries, &state, source, 3);
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
    let resolution = make_test_resolution(&registries, &state, source, 3);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("early-due validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    apply_clock_advance(&mut state, SimulationTick::new(1));

    let forged_due = SimulationTick::new(2);
    let expected_due = SimulationTick::new(3);
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
fn persisted_production_job_rejects_impossible_consumed_material_phase_state() {
    let input = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let process = ProcessDefinition::new(
        TEST_COMPOSITION_PROCESS,
        "test persisted phase validation",
        vec![MaterialInputSpec::new(input, Mass::from_milligrams(10))],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
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
        &state,
        source,
        TEST_COMPOSITION_PROCESS,
        3,
        vec![MaterialLotSpec::new(
            input,
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(300_000),
        )],
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
        Err(LoadError::InvalidState(
            StateValidationError::InvalidJobConsumedPhaseState {
                job,
                error: MaterialPhaseStateError::SolidAboveMeltingPoint {
                    material: MATERIAL_COPPER,
                    temperature: invalid_temperature,
                    melting_point,
                },
            }
        ))
    );
}

#[test]
fn routed_output_streams_reserve_and_complete_by_identity_not_route_order() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(10_001));
    let source = add_test_stockpile(&mut state, 20);
    let charcoal_destination = add_test_stockpile(&mut state, 10);
    let slag_destination = add_test_stockpile(&mut state, 10);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
    assert_eq!(
        resolution
            .output_streams()
            .iter()
            .map(|stream| stream.id())
            .collect::<Vec<_>>(),
        vec![
            ProcessOutputStreamId::new(10),
            ProcessOutputStreamId::new(20)
        ]
    );

    let token = match validate_start_process_routed(
        &registries,
        &state,
        &resolution,
        source,
        &[
            ProcessOutputRoute::new(ProcessOutputStreamId::new(20), slag_destination),
            ProcessOutputRoute::new(ProcessOutputStreamId::new(10), charcoal_destination),
        ],
    ) {
        Ok(token) => token,
        Err(error) => panic!("multi-stream process validation failed: {error}"),
    };
    let job = commit_process_for_test(token, &mut state);

    assert_eq!(
        state
            .inventory()
            .get_stockpile(charcoal_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(slag_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(4))
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
            (ProcessOutputStreamId::new(10), charcoal_destination),
            (ProcessOutputStreamId::new(20), slag_destination),
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
                stream: ProcessOutputStreamId::new(10),
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

    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("multi-stream completion tick failed: {error}"),
    };
    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert_eq!(
        outcome.production_completions()[0].routes(),
        [
            ProcessOutputRoute::new(ProcessOutputStreamId::new(10), charcoal_destination,),
            ProcessOutputRoute::new(ProcessOutputStreamId::new(20), slag_destination),
        ]
    );
    let charcoal_record = match state.inventory().get_stockpile(charcoal_destination) {
        Some(record) => record,
        None => panic!("charcoal destination disappeared"),
    };
    assert_eq!(charcoal_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        charcoal_record.get_mass(charcoal_lump()),
        Mass::from_milligrams(6)
    );
    let slag_record = match state.inventory().get_stockpile(slag_destination) {
        Some(record) => record,
        None => panic!("slag destination disappeared"),
    };
    assert_eq!(slag_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(slag_record.get_mass(slag_lump()), Mass::from_milligrams(4));
}

#[test]
fn duplicate_output_route_is_rejected_atomically() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(10_002));
    let source = add_test_stockpile(&mut state, 20);
    let first_destination = add_test_stockpile(&mut state, 10);
    let second_destination = add_test_stockpile(&mut state, 10);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        &resolution,
        source,
        &[
            ProcessOutputRoute::new(ProcessOutputStreamId::new(10), first_destination),
            ProcessOutputRoute::new(ProcessOutputStreamId::new(10), second_destination),
        ],
    );

    assert_eq!(
        result,
        Err(StartProcessError::DuplicateOutputRoute {
            stream: ProcessOutputStreamId::new(10),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn shared_destination_capacity_is_checked_against_aggregate_stream_mass() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(10_003));
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 9);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_multi_stream_resolution(&registries, &state, source, 1);
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        &resolution,
        source,
        &[
            ProcessOutputRoute::new(ProcessOutputStreamId::new(10), destination),
            ProcessOutputRoute::new(ProcessOutputStreamId::new(20), destination),
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

    let result = validate_process_inputs(&registries, &state, TEST_PROCESS, source);

    assert!(matches!(
        result,
        Err(ProcessInputError::InsufficientMass {
            stockpile: _stockpile,
            commodity: _commodity,
            available: _available,
            requested: _requested,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn resolved_process_cannot_create_or_destroy_unaccounted_matter() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(111));
    let source = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let inputs = validate_process_inputs(&registries, &state, TEST_PROCESS, source)
        .unwrap_or_else(|error| panic!("matter-balance input binding failed: {error}"));
    let before = state.clone();

    let result = inputs.resolve_without_resources(
        TickSpan::new(3),
        vec![MaterialLotSpec::new(
            charcoal_lump(),
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
    let resolution = make_test_resolution(&registries, &state, source, 20);
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
    let resolution = make_test_resolution(&registries, &state, stockpile, 2);

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

    let first_resolution = make_test_resolution(&registries, &state, source, 1);
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => commit_process_for_test(token, &mut state),
            Err(error) => panic!("first process validation failed: {error}"),
        };
    let second_resolution = make_test_resolution(&registries, &state, source, 1);
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

    let first_resolution = make_test_resolution(&registries, &state, source, 1);
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => token,
            Err(error) => panic!("first process validation failed: {error}"),
        };
    commit_process_for_test(first, &mut state);
    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("first completion failed: {error}");
    }

    let second_resolution = make_test_resolution(&registries, &state, source, 1);
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
    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("second completion failed: {error}");
    }

    let lot_ids: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(lot_ids.len(), 1);
    let lot = state
        .inventory()
        .get_lot(lot_ids[0])
        .unwrap_or_else(|| panic!("coalesced production output disappeared"));
    assert_eq!(lot.mass(), Mass::from_milligrams(20));
    assert_eq!(lot.created_at(), SimulationTick::new(1));
    assert_eq!(lot.latest_created_at(), SimulationTick::new(2));
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
    let resolution = make_test_resolution(&registries, &state, source, 10);
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
    let resolution = make_test_resolution(&registries, &state, source, 10);
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
    let resolution = make_test_resolution(&registries, &state, source, 10);
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
    let resolution = make_test_resolution(&registries, &state, source, 10);
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
    let resolution = make_test_resolution(&registries, &state, source, 1);
    let later_resolution = make_resolution_for_process(
        &registries,
        &state,
        source,
        TEST_PROCESS,
        1,
        vec![
            MaterialLotSpec::new(
                charcoal_lump(),
                Mass::from_milligrams(1),
                Temperature::from_millikelvin(900_000),
            ),
            MaterialLotSpec::new(
                slag_lump(),
                Mass::from_milligrams(9),
                Temperature::from_millikelvin(900_000),
            ),
        ],
    );
    assert_ne!(resolution.outputs(), later_resolution.outputs());
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("original process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("completion after later resolution change failed: {error}");
    }

    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(
        destination_record.get_mass(charcoal_lump()),
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
    assert_eq!(lot.temperature(), Temperature::from_millikelvin(600_000));
}

#[test]
fn composition_constrained_process_consumes_only_eligible_lots() {
    let input = match MaterialInputSpec::with_constraints(
        copper_ore(),
        Mass::from_milligrams(10),
        vec![minimum_copper_constraint(800_000)],
    ) {
        Ok(input) => input,
        Err(error) => panic!("input fixture failed: {error}"),
    };
    let process = ProcessDefinition::new(
        TEST_COMPOSITION_PROCESS,
        "test concentration",
        vec![input],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(18));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    let poor = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        copper_ore(),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(300_000),
        make_copper_slag_composition(600_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("poor ore fixture failed: {error}"),
    };

    let poor_only = validate_process_inputs(&registries, &state, TEST_COMPOSITION_PROCESS, source);
    match poor_only {
        Err(ProcessInputError::InsufficientMass { available, .. }) => {
            assert_eq!(available, Mass::ZERO);
        }
        Err(error) => panic!("unexpected composition validation error: {error}"),
        Ok(_) => panic!("poor ore incorrectly satisfied rich-ore input constraint"),
    }

    let rich = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        copper_ore(),
        Mass::from_milligrams(11),
        Temperature::from_millikelvin(300_000),
        make_copper_slag_composition(900_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("rich ore fixture failed: {error}"),
    };
    let resolution = make_resolution_for_process(
        &registries,
        &state,
        source,
        TEST_COMPOSITION_PROCESS,
        5,
        vec![
            MaterialLotSpec::new(
                native_copper(),
                Mass::from_milligrams(8),
                Temperature::from_millikelvin(350_000),
            ),
            MaterialLotSpec::new(
                slag_lump(),
                Mass::from_milligrams(2),
                Temperature::from_millikelvin(350_000),
            ),
        ],
    );
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("rich ore should satisfy process: {error}"),
    };
    commit_process_for_test(token, &mut state);

    let poor_lot = match state.inventory().get_lot(poor) {
        Some(lot) => lot,
        None => panic!("poor ore lot disappeared"),
    };
    let rich_lot = match state.inventory().get_lot(rich) {
        Some(lot) => lot,
        None => panic!("rich ore lot disappeared"),
    };
    assert_eq!(poor_lot.mass(), Mass::from_milligrams(20));
    assert_eq!(rich_lot.mass(), Mass::from_milligrams(1));
}

#[test]
fn overlapping_composition_inputs_cannot_double_count_one_lot() {
    let first = match MaterialInputSpec::with_constraints(
        copper_ore(),
        Mass::from_milligrams(6),
        vec![minimum_copper_constraint(800_000)],
    ) {
        Ok(input) => input,
        Err(error) => panic!("first input fixture failed: {error}"),
    };
    let second = match MaterialInputSpec::with_constraints(
        copper_ore(),
        Mass::from_milligrams(6),
        vec![minimum_copper_constraint(850_000)],
    ) {
        Ok(input) => input,
        Err(error) => panic!("second input fixture failed: {error}"),
    };
    let process = ProcessDefinition::new(
        TEST_COMPOSITION_PROCESS,
        "overlapping composition selection",
        vec![first, second],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(19));
    let source = add_test_stockpile(&mut state, 100);
    add_test_stockpile(&mut state, 100);
    if let Err(error) = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        copper_ore(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
        make_copper_slag_composition(900_000),
    ) {
        panic!("overlap lot fixture failed: {error}");
    }

    let result = validate_process_inputs(&registries, &state, TEST_COMPOSITION_PROCESS, source);

    match result {
        Err(ProcessInputError::InsufficientMass {
            available,
            requested,
            ..
        }) => {
            assert_eq!(available, Mass::from_milligrams(4));
            assert_eq!(requested, Mass::from_milligrams(6));
        }
        Err(error) => panic!("unexpected overlap validation error: {error}"),
        Ok(_) => panic!("overlapping inputs double-counted one material lot"),
    }
}
