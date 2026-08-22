//! Tests for the sibling execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, MINING_METHOD_HAND_PICK,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use crate::core::quantity::{Temperature, Volume};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::crafting::{
    ManualCraftStartRequest, StartManualCraftError, validate_start_manual_craft,
};
use crate::energy::calculate_explicit_energy_accounting;
use crate::equipment::{
    apply_equipment_condition_plan, decide_equipment_wear, validate_assemble_equipment,
    validate_upgrade_equipment,
};
use crate::geology::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalEvidenceKind, MaterialAbundanceEstimate,
    ProspectingResolution, validate_record_prospecting,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::{
    PlayerWork, PlayerWorkStartError, PlayerWorkValidationError,
    calculate_player_work_resource_budget,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::mining::{
    MiningJobValidationError, MiningTargetRequest, MiningValidationError, resolve_mining_target,
};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::survival::{assess_survival, initialize_player_survival};

fn deposit_spec() -> GeneratedDepositSpec {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4))
        .unwrap_or_else(|error| panic!("mining test bounds failed: {error}"));
    GeneratedDepositSpec::new(
        bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("mining test deposit failed: {error}"))
}

fn insert_known_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, crate::geology::InsertGeneratedDepositError> {
    let region = spec.bounds();
    let material = spec.commodity().material();
    let deposit = crate::geology::insert_generated_deposit(registries, state, spec)?;
    let estimate = MaterialAbundanceEstimate::new(material, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("mining known-deposit estimate failed: {error}"));
    let evidence = ProspectingResolution::new_for_fixture(
        region,
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate],
    );
    validate_record_prospecting(registries, state, evidence)
        .unwrap_or_else(|error| panic!("mining known-deposit evidence failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("mining known-deposit evidence commit failed: {error}"));
    Ok(deposit)
}

fn validate_known_mining(
    registries: &Registries,
    state: &AppState,
    method: MiningMethodId,
    deposit: GeologicalDepositId,
    destination: StockpileId,
    equipment: EquipmentId,
    mass: Mass,
) -> Result<ValidatedMiningStart, MiningStartError> {
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("known mining fixture deposit disappeared"));
    let target = resolve_mining_target(
        state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("known mining target resolution failed: {error}"));
    super::validate_start_mining(
        registries,
        state,
        method,
        target,
        destination,
        equipment,
        mass,
    )
}

#[test]
fn resolved_mining_target_is_invalidated_by_new_geological_knowledge() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0030));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-target knowledge survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-target knowledge destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-target knowledge deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-target knowledge deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target knowledge resolution failed: {error}"));
    let expected = state.geological_knowledge().revision();
    let remote = VoxelBounds::new(VoxelCoord::new(100, -8, 0), VoxelCoord::new(101, -7, 1))
        .unwrap_or_else(|error| panic!("stale-target knowledge evidence bounds failed: {error}"));
    let estimate = MaterialAbundanceEstimate::new(MATERIAL_STONE, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("stale-target knowledge estimate failed: {error}"));
    validate_record_prospecting(
        &registries,
        &state,
        ProspectingResolution::new_for_fixture(
            remote,
            GeologicalEvidenceKind::SurfaceExposure,
            vec![estimate],
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target knowledge evidence validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale-target knowledge evidence commit failed: {error}"));
    let actual = state.geological_knowledge().revision();
    let before = state.clone();

    assert!(matches!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        ),
        Err(MiningStartError::StaleTargetKnowledge {
            expected: found_expected,
            actual: found_actual,
        }) if found_expected == expected && found_actual == actual
    ));
    assert_eq!(state, before);
}

#[test]
fn resolved_mining_target_is_invalidated_by_geology_change() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0031));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-target geology survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-target geology destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-target geology deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-target geology deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target geology resolution failed: {error}"));
    let expected = state.geology().revision();
    let remote_bounds = VoxelBounds::new(VoxelCoord::new(100, -8, 0), VoxelCoord::new(101, -7, 1))
        .unwrap_or_else(|error| panic!("stale-target geology deposit bounds failed: {error}"));
    let remote = GeneratedDepositSpec::new(
        remote_bounds,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(100_000_000),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("stale-target geology deposit spec failed: {error}"));
    crate::geology::insert_generated_deposit(&registries, &mut state, remote)
        .unwrap_or_else(|error| panic!("stale-target geology mutation failed: {error}"));
    let actual = state.geology().revision();
    let before = state.clone();

    assert!(matches!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        ),
        Err(MiningStartError::StaleTargetGeology {
            expected: found_expected,
            actual: found_actual,
        }) if found_expected == expected && found_actual == actual
    ));
    assert_eq!(state, before);
}

#[test]
fn mining_rejects_work_that_would_continue_after_tool_failure() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0023));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("condition-lifetime survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("condition-lifetime destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("condition-lifetime deposit failed: {error}"));
    let wear = decide_equipment_wear(&state, pick, 999_500)
        .unwrap_or_else(|error| panic!("condition-lifetime wear decision failed: {error}"));
    apply_equipment_condition_plan(&mut state, wear)
        .unwrap_or_else(|error| panic!("condition-lifetime wear commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("condition-lifetime pick disappeared"))
            .condition(),
        Condition::new(500)
            .unwrap_or_else(|error| panic!("condition-lifetime fixture failed: {error}"))
    );
    let before = state.clone();

    assert!(matches!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100),
        ),
        Err(MiningStartError::ConditionDuration(_))
    ));
    assert_eq!(state, before);
}

#[test]
fn loaded_mining_job_reconstructs_authored_condition_outcome() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0021));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining wear-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining wear-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining wear-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining wear-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining wear-audit commit failed: {error}"));
    let required = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining wear-audit job disappeared"))
        .equipment_condition_after();
    let forged = Condition::new(required.parts_per_million().saturating_add(1))
        .unwrap_or_else(|error| panic!("mining forged condition failed: {error}"));
    assert_ne!(forged, required);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining wear-audit serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["equipment_condition_after"] =
        serde_json::json!(forged.parts_per_million());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining wear-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::ConditionOutcomeMismatch {
                job,
                stored: forged,
                required,
            }
        )))
    );
}

#[test]
fn loaded_mining_state_rejects_job_map_key_identity_mismatch() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0024));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining key-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining key-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining key-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining key-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining key-audit commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining key-audit serialization failed: {error}"));
    let jobs = encoded["state"]["systems"]["mining"]["jobs"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized mining jobs were not an object"));
    let record = jobs
        .remove(&job.value().to_string())
        .unwrap_or_else(|| panic!("serialized mining job disappeared"));
    let forged_key = job.value() + 1;
    assert!(jobs.insert(forged_key.to_string(), record).is_none());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining key-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Mining(
            MiningValidationError::JobIdMismatch {
                key: MiningJobId::new(forged_key),
                record: job,
            }
        )))
    );
}

#[test]
fn deposit_excavation_hardness_is_independent_of_assay_composition() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_000A));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mixed-hardness survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mixed-hardness destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(12, -8, 0), VoxelCoord::new(13, -7, 1))
        .unwrap_or_else(|error| panic!("mixed-hardness bounds failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 999_000),
        CompositionComponent::new(MATERIAL_STONE, 1_000),
    ])
    .unwrap_or_else(|error| panic!("mixed-hardness composition failed: {error}"));
    let deposit = insert_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(600_000_000),
            composition,
        )
        .unwrap_or_else(|error| panic!("mixed-hardness deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("mixed-hardness deposit insertion failed: {error}"));

    let error = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .err()
    .unwrap_or_else(|| panic!("stone pick unexpectedly ignored deposit excavation hardness"));
    assert_eq!(
        error,
        MiningStartError::TargetTooHard {
            maximum: Pressure::from_pascals(500_000_000),
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mixed-hardness deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
}

#[test]
fn ready_mining_job_keeps_historical_tool_physics_after_tool_upgrade() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0023));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining trace survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining trace destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining trace deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining trace start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining trace start commit failed: {error}"));
    let duration = state
        .mining()
        .get_job(job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("mining trace job disappeared"));
    for _ in 0..duration {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining trace completion failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(|record| record.ready_at().is_some())
    );

    let reinforcement_source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
            .unwrap_or_else(|error| panic!("mining trace reinforcement source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        reinforcement_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining trace reinforcement failed: {error}"));
    validate_upgrade_equipment(
        &registries,
        &state,
        pick,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        reinforcement_source,
    )
    .unwrap_or_else(|error| panic!("mining trace upgrade failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining trace upgrade commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        state
            .mining()
            .get_job(job)
            .map(MiningJobRecord::equipment_definition),
        Some(EQUIPMENT_STONE_PICK)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("mining trace post-upgrade audit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining trace serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("mining trace decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining trace load failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn loaded_ready_mining_job_reconstructs_authored_duration() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0022));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining duration-audit survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining duration-audit destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining duration-audit deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining duration-audit start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining duration-audit commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining duration-audit job disappeared"));
    let required = crate::core::time::TickSpan::new(
        record.completes_at().value() - record.started_at().value(),
    );
    for _ in 0..required.value() {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining duration-audit completion failed: {error}"));
    }
    let ready = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("ready mining duration-audit job disappeared"));
    assert!(ready.ready_at().is_some());
    let forged_started_at = ready.started_at().value() + 1;

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining duration-audit serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["schedule"]["started_at"] =
        serde_json::json!(forged_started_at);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining duration-audit tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::DurationMismatch {
                job,
                stored: crate::core::time::TickSpan::new(required.value() - 1),
                required,
            }
        )))
    );
}

fn assemble_pick_for_test(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("pick assembly source failed: {error}"));
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
            registries,
            state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("pick assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("pick assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("pick assembly commit failed: {error}"))
}

fn assemble_hand_crank_for_test(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("hand-crank assembly source failed: {error}"));
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
        .unwrap_or_else(|error| panic!("hand-crank assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_HAND_CRANK, source)
        .unwrap_or_else(|error| panic!("hand-crank assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("hand-crank assembly commit failed: {error}"))
}

fn assemble_reinforced_pick_for_test(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_020_000))
        .unwrap_or_else(|error| panic!("reinforced pick assembly source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
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
        .unwrap_or_else(|error| panic!("reinforced pick assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_COPPER_REINFORCED_PICK, source)
        .unwrap_or_else(|error| panic!("reinforced pick assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("reinforced pick assembly commit failed: {error}"))
}

#[test]
fn stone_pick_refuses_deposit_above_authored_excavation_hardness() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0002));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
        .unwrap_or_else(|error| panic!("hardness bounds failed: {error}"));
    let deposit = insert_known_deposit(
        &registries,
        &mut state,
        GeneratedDepositSpec::new(
            bounds,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(300_000),
            Pressure::from_pascals(700_000_000),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap_or_else(|error| panic!("hardness deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("hardness deposit insertion failed: {error}"));

    let error = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .err()
    .unwrap_or_else(|| panic!("stone pick unexpectedly mined deposit above its hardness"));
    assert_eq!(
        error,
        MiningStartError::TargetTooHard {
            maximum: Pressure::from_pascals(500_000_000),
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("hardness deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(100_000)
    );
}

#[test]
fn mining_requires_enough_hydration_reserve_to_finish() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0005));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining reserve survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining reserve destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining reserve deposit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining low-hydration decode failed: {error}"));
    let low_reserve = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining low-hydration load failed: {error}"));
    let before = low_reserve.clone();

    assert!(matches!(
        validate_known_mining(
            &registries,
            &low_reserve,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        ),
        Err(MiningStartError::Work(
            PlayerWorkStartError::InsufficientHydration { .. }
        ))
    ));
    assert_eq!(low_reserve, before);
}

#[test]
fn active_mining_save_requires_enough_hydration_to_finish_remaining_work() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0006));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining save reserve survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining save reserve destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining save reserve deposit failed: {error}"));
    let token = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining save reserve start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining save reserve commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining save reserve job disappeared"));
    let remaining =
        crate::core::time::TickSpan::new(record.completes_at().value() - state.tick().value());
    let exertion = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining save reserve method disappeared"))
        .exertion();
    let required = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        remaining,
    )
    .unwrap_or_else(|error| panic!("mining save reserve budget failed: {error:?}"))
    .hydration();
    assert!(required > Volume::from_microliters(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining save reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining save reserve decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientHydration {
                available: Volume::from_microliters(1),
                required,
            }
        )))
    );
}

#[test]
fn copper_reinforcement_turns_cold_worked_native_metal_into_more_capable_extraction() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0004));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("reinforced mining survival setup failed: {error}"));
    let stone_pick = assemble_pick_for_test(&registries, &mut state);
    let reinforced_pick = assemble_reinforced_pick_for_test(&registries, &mut state);
    let reinforced_record = state
        .equipment()
        .get_equipment(reinforced_pick)
        .unwrap_or_else(|| panic!("reinforced pick disappeared after assembly"));
    assert_eq!(
        reinforced_record.embodied_mass(),
        Mass::from_milligrams(1_020_000)
    );
    assert!(reinforced_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && trace.mass() == Mass::from_milligrams(20_000)
    }));

    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(300_000))
        .unwrap_or_else(|error| panic!("reinforced mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("reinforced mining deposit failed: {error}"));
    let requested = Mass::from_milligrams(250_000);

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            stone_pick,
            requested,
        )
        .err(),
        Some(MiningStartError::BatchTooLarge {
            maximum: Mass::from_milligrams(200_000),
            requested,
        })
    );

    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        reinforced_pick,
        requested,
    )
    .unwrap_or_else(|error| panic!("reinforced pick mining validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reinforced pick mining commit failed: {error}"));
    let job_record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("reinforced mining job disappeared"));
    assert_eq!(
        job_record.completes_at().value() - job_record.started_at().value(),
        3
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("reinforced mining deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(750_000)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("reinforced mining state audit failed: {error}"));
}

#[test]
fn missing_mining_capability_reports_the_exact_authored_requirement() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("missing-capability survival setup failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("missing-capability destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("missing-capability deposit failed: {error}"));
    let hand_crank = assemble_hand_crank_for_test(&registries, &mut state);
    let expected_capability = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand-pick mining method disappeared"))
        .mass_flow_capability();

    let error = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        hand_crank,
        Mass::from_milligrams(1),
    )
    .err()
    .unwrap_or_else(|| panic!("hand crank unexpectedly satisfied hand-mining capabilities"));

    assert_eq!(
        error,
        MiningStartError::MissingCapability {
            capability: expected_capability,
        }
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("missing-capability deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(1_000_000)
    );
}

#[test]
fn knap_assemble_mine_claim_loop_is_conserved_exclusive_and_persistent() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining survival initialization failed: {error}"));

    let stone_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("mining primitive-material source failed: {error}"));
    let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("mining shaped stockpile failed: {error}"));
    let ore_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
            .unwrap_or_else(|error| panic!("mining ore destination failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        stone_source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining stone ingress failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        stone_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining handle wood ingress failed: {error}"));

    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, stone_source, shaped),
    )
    .unwrap_or_else(|error| panic!("mining knapping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining knapping commit failed: {error}"));
    for _ in 0..40 {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining knapping tick failed: {error}"));
    }
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_SHAPE_WOOD_HANDLE, stone_source, shaped),
    )
    .unwrap_or_else(|error| panic!("mining handle shaping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining handle shaping commit failed: {error}"));
    for _ in 0..40 {
        advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining handle shaping tick failed: {error}"));
    }

    let energy_before_assembly = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("pre-assembly energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("pre-assembly energy total overflowed"));
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, shaped)
        .unwrap_or_else(|error| panic!("stone pick assembly validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone pick assembly commit failed: {error}"));
    let energy_after_assembly = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("post-assembly energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("post-assembly energy total overflowed"));
    assert_eq!(energy_after_assembly, energy_before_assembly);
    let pick_record = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("assembled stone pick disappeared"));
    assert_eq!(
        pick_record.embodied_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(pick_record.embodied_material().len(), 2);
    assert!(pick_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
            && trace.mass() == Mass::from_milligrams(800_000)
    }));
    assert!(pick_record.embodied_material().iter().any(|trace| {
        trace.profile().commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)
            && trace.mass() == Mass::from_milligrams(200_000)
    }));

    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining copper deposit insertion failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("mining initial matter accounting failed: {error}"))
        .total();
    let energy_before_mining = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("mining initial energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("mining initial energy total overflowed"));
    let survival_before_mining = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("mining survival state disappeared before work"));
    let pick_condition_before = state
        .equipment()
        .get_equipment(pick)
        .unwrap_or_else(|| panic!("mining pick disappeared before work"))
        .condition();

    let mining = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        ore_destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining start validation failed: {error}"));
    let job = mining
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining start commit failed: {error}"));
    let job_record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining job disappeared after start"));
    let pick_condition_after = job_record.equipment_condition_after();
    let mining_duration = job_record.completes_at().value() - job_record.started_at().value();
    assert!(pick_condition_after < pick_condition_before);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("mining pick disappeared after start"))
            .condition(),
        pick_condition_before
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining deposit disappeared"))
            .remaining_mass(),
        Mass::from_milligrams(900_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ore_destination)
            .unwrap_or_else(|| panic!("mining destination disappeared"))
            .reserved_inbound(),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining WIP accounting failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining WIP energy accounting failed: {error}"))
            .total(),
        Some(energy_before_mining)
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::Mining { job })
    );

    let craft_error = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, stone_source, shaped),
    )
    .err()
    .unwrap_or_else(|| panic!("manual crafting unexpectedly started during mining"));
    assert_eq!(
        craft_error,
        StartManualCraftError::Work(PlayerWorkStartError::Busy {
            active: PlayerWork::Mining { job },
        })
    );

    let mut final_tick = None;
    for _ in 0..mining_duration {
        final_tick = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining work tick failed: {error}")),
        );
    }
    assert_eq!(
        final_tick
            .as_ref()
            .unwrap_or_else(|| panic!("mining work produced no tick outcome"))
            .ready_mining_jobs(),
        &[job]
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .unwrap_or_else(|| panic!("mining pick disappeared after work"))
            .condition(),
        pick_condition_after
    );
    let survival_after_mining = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("mining survival state disappeared after work"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("hand mining method disappeared"))
        .exertion();
    assert_eq!(
        survival_before_mining.metabolic_energy().nanojoules()
            - survival_after_mining.metabolic_energy().nanojoules(),
        (physiology.basal_energy_cost_per_tick().nanojoules()
            + exertion.energy_cost_per_tick().nanojoules())
            * u128::from(mining_duration)
    );
    assert_eq!(
        survival_before_mining.hydration().microliters()
            - survival_after_mining.hydration().microliters(),
        (physiology.hydration_loss_per_tick().microliters()
            + exertion.hydration_loss_per_tick().microliters())
            * mining_duration
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ore_destination)
            .unwrap_or_else(|| panic!("mining destination disappeared before claim"))
            .stored_mass(),
        Mass::ZERO
    );

    validate_claim_mining_output(&registries, &state, job)
        .unwrap_or_else(|error| panic!("mining claim validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining claim commit failed: {error}"));
    let destination = state
        .inventory()
        .get_stockpile(ore_destination)
        .unwrap_or_else(|| panic!("mining destination disappeared after claim"));
    assert_eq!(
        destination.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
        Mass::from_milligrams(100_000)
    );
    assert_eq!(destination.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining final matter accounting failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining final energy accounting failed: {error}"))
            .total(),
        Some(energy_before_mining)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("mining final state audit failed: {error}"));

    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining save serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining save decode failed: {error}"));
    let restored = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining save validation failed: {error}"));
    assert_eq!(restored, state);
}

#[cfg(feature = "test-soak")]
fn run_mining_soak(seed: WorldSeed) -> AppState {
    let registries = build_registries();
    let mut state = AppState::new(seed);
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining soak survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("mining soak destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining soak deposit failed: {error}"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("mining soak matter accounting failed: {error}"))
        .total();
    let initial_energy = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("mining soak energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("mining soak energy total overflowed"));

    for step in 0_u64..1_000 {
        let job = validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(1_000),
        )
        .unwrap_or_else(|error| panic!("mining soak start failed at step {step}: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining soak start commit failed at step {step}: {error}"));

        if step == 500 {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| panic!("mining soak save failed: {error}"));
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("mining soak decode failed: {error}"));
            state = decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("mining soak active-job load failed: {error}"));
        }

        let job_record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("mining soak job disappeared at step {step}"));
        let duration = job_record
            .completes_at()
            .value()
            .checked_sub(job_record.started_at().value())
            .unwrap_or_else(|| panic!("mining soak duration underflowed at step {step}"));
        assert!(duration > 0);
        for _ in 0..duration {
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining soak tick failed at step {step}: {error}"));
        }
        validate_claim_mining_output(&registries, &state, job)
            .unwrap_or_else(|error| panic!("mining soak claim failed at step {step}: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("mining soak claim commit failed at step {step}: {error}")
            });

        if step.is_multiple_of(97) {
            validate_loaded_state(&registries, &state).unwrap_or_else(|error| {
                panic!("mining soak exhaustive audit failed at step {step}: {error}")
            });
            assert_eq!(
                calculate_matter_accounting(&state)
                    .unwrap_or_else(|error| panic!("mining soak matter audit failed: {error}"))
                    .total(),
                initial_matter
            );
            assert_eq!(
                calculate_explicit_energy_accounting(&registries, &state)
                    .unwrap_or_else(|error| panic!("mining soak energy audit failed: {error}"))
                    .total(),
                Some(initial_energy)
            );
        }
    }

    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining soak deposit disappeared"))
            .lifecycle(),
        GeologicalDepositLifecycle::Depleted
    );
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("mining soak destination disappeared"));
    assert_eq!(
        destination_record.stored_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(state.mining().jobs().count(), 0);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining soak final matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining soak final energy audit failed: {error}"))
            .total(),
        Some(initial_energy)
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn mining_soak_preserves_depletion_conservation_persistence_and_replay() {
    let seed = WorldSeed::new(0xA11E_5000);
    let first = run_mining_soak(seed);
    let second = run_mining_soak(seed);

    assert_eq!(first, second);
}
