//! Contract tests for mining admission, lifecycle, and claim.

use super::*;
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_QUARRY_PICK, FORM_CHEST_BODY, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP,
    FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    MINING_METHOD_HAND_PICK, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_HANDLE,
    PROSPECTING_DETAILED_FIELD_SURVEY, PROSPECTING_FIELD_INSPECTION,
    STORAGE_TIMBER_PROVISIONS_CHEST, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Pressure, Temperature, Volume};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::crafting::{
    ManualCraftStartRequest, StartManualCraftError, validate_start_manual_craft,
};
use crate::energy::calculate_explicit_energy_accounting;
use crate::equipment::{
    EquipmentId, degrade_equipment_condition_for_test, validate_assemble_equipment,
    validate_upgrade_equipment,
};
#[cfg(feature = "test-soak")]
use crate::geology::GeologicalDepositLifecycle;
use crate::geology::{
    ExcavationHardnessEstimate, GeneratedDepositSpec, GeologicalDepositId, GeologicalEvidenceKind,
    MaterialAbundanceEstimate, ProspectingResolution, record_prospecting_for_test,
};
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, MaterialLotSelection, STORAGE_AGE_PARTS_PER_TICK,
    StockpileId, StockpileStructuralLoadError, add_solid_stockpile_for_test, deposit_lot_for_test,
    validate_build_storage_enclosure, validate_mount_stockpile,
    validate_start_storage_enclosure_dismantling, validate_unmount_stockpile,
};
use crate::labor::{
    PlayerWork, PlayerWorkStartError, PlayerWorkValidationError,
    calculate_player_work_resource_budget,
};
use crate::logistics::{
    PlayerEquipmentAccessError, PlayerStockpileAccessError, validate_allocate_ground_stockpile,
    validate_initialize_player_logistics, validate_place_ground_stockpile,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition, MaterialId};
use crate::matter::calculate_matter_accounting;
use crate::mining::{
    MiningJobId, MiningJobRecord, MiningJobValidationError, MiningMethodId, MiningTargetRequest,
    MiningValidationError, resolve_mining_target,
};
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::survival::{Vitality, assess_survival, initialize_player_survival, player_record};

fn deposit_spec() -> GeneratedDepositSpec {
    deposit_spec_with_mass(Mass::from_milligrams(1_000_000))
}

fn deposit_spec_with_mass(mass: Mass) -> GeneratedDepositSpec {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4))
        .unwrap_or_else(|error| panic!("mining test bounds failed: {error}"));
    GeneratedDepositSpec::new(
        bounds,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        mass,
        Temperature::from_millikelvin(300_000),
        Pressure::from_pascals(350_000_000),
        MaterialComposition::pure(MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("mining test deposit failed: {error}"))
}

fn make_next_tick_fatal(registries: &Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal mining fixture player disappeared"));
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
fn fatal_tick_cancels_unfinished_mining_without_extracting_or_wearing_tool() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("fatal mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("fatal mining deposit failed: {error}"));
    let remaining_before = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.remaining_mass())
        .unwrap_or_else(|| panic!("fatal mining deposit disappeared before start"));
    let condition_before = state
        .equipment()
        .get_equipment(pick)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("fatal mining pick disappeared before start"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("fatal mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal mining start commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("fatal mining job disappeared after start"));
    assert!(
        record.completes_at().value() > state.tick().value() + 1,
        "fatal mining proof requires unfinished work after the next tick"
    );
    let reserved_output = record.output().mass();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_output)
    );
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal mining tick failed: {error}"));

    assert!(outcome.ready_mining_jobs().is_empty());
    assert!(state.mining().get_job(job).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO),
        "canceled mining must return its unused output reservation"
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(remaining_before),
        "unfinished canceled mining must not extract geological matter"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(condition_before),
        "unfinished canceled mining must not apply completion wear"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("fatal mining state failed trusted-load audit: {error}"));
}

#[test]
fn fatal_mining_cancellation_consumes_reserved_headroom_at_owner_limits() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("fatal headroom mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal headroom mining commit failed: {error}"));
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(|record| record.completes_at().value() > state.tick().value() + 1),
        "fatal headroom proof requires unfinished mining after the next tick"
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fatal headroom mining serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX - 2);
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("fatal headroom mining decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("exact mining cancellation headroom must load: {error}"));
    make_next_tick_fatal(&registries, &mut loaded);

    let _ = advance_tick(&registries, &mut loaded)
        .unwrap_or_else(|error| panic!("fatal headroom cancellation tick failed: {error}"));

    assert!(loaded.mining().get_job(job).is_none());
    assert_eq!(loaded.inventory().revision(), u64::MAX);
    assert_eq!(loaded.inventory().next_lot_id(), u64::MAX - 1);
    assert_eq!(loaded.mining().revision(), u64::MAX - 1);
    assert_eq!(loaded.structures().revision(), u64::MAX - 1);
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn fatal_tick_allows_mining_due_that_tick_to_finish_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("fatal due mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("fatal due mining deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("fatal due mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal due mining start commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("fatal due mining job disappeared after start"));
    let completes_at = record.completes_at();
    let output_mass = record.output().mass();
    let condition_after = record.equipment_condition_after();
    let remaining_before = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.remaining_mass())
        .unwrap_or_else(|| panic!("fatal due mining deposit disappeared before work"));
    while state.tick().value() + 1 < completes_at.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal due mining setup tick failed: {error}"));
        assert!(outcome.ready_mining_jobs().is_empty());
    }
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due mining completion tick failed: {error}"));

    assert_eq!(outcome.ready_mining_jobs(), &[job]);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(|record| record.is_ready_to_claim()),
        "mining due on the fatal tick must enter claim custody"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(output_mass),
        "ready mining output must retain its reserved claim destination"
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        remaining_before.checked_sub(output_mass),
        "mining due on the fatal tick must extract its bound output"
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(condition_after),
        "mining due on the fatal tick must apply its completion wear"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("fatal due mining state invalid: {error}"));
}

#[test]
fn mining_shortage_is_revealed_only_after_committing_requested_work() {
    let registries = build_registries();
    let setup = |deposit_mass: Mass| {
        let mut state = AppState::new();
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("shortage mining survival setup failed: {error}"));
        let pick = assemble_pick_for_test(&registries, &mut state);
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
            .unwrap_or_else(|error| panic!("shortage mining destination failed: {error}"));
        let deposit = insert_known_deposit(
            &registries,
            &mut state,
            deposit_spec_with_mass(deposit_mass),
        )
        .unwrap_or_else(|error| panic!("shortage mining deposit failed: {error}"));
        (state, deposit, destination, pick)
    };
    let requested = Mass::from_milligrams(100_000);
    let recoverable = Mass::from_milligrams(50_000);
    let (mut scarce, scarce_deposit, scarce_destination, scarce_pick) = setup(recoverable);
    let (mut ample, ample_deposit, ample_destination, ample_pick) =
        setup(Mass::from_milligrams(1_000_000));

    let scarce_before = scarce.clone();
    let scarce_start = validate_known_mining(
        &registries,
        &scarce,
        MINING_METHOD_HAND_PICK,
        scarce_deposit,
        scarce_destination,
        scarce_pick,
        requested,
    )
    .unwrap_or_else(|error| panic!("scarce target leaked shortage during validation: {error}"));
    let ample_start = validate_known_mining(
        &registries,
        &ample,
        MINING_METHOD_HAND_PICK,
        ample_deposit,
        ample_destination,
        ample_pick,
        requested,
    )
    .unwrap_or_else(|error| panic!("ample target mining validation failed: {error}"));
    assert_eq!(
        scarce, scarce_before,
        "mining validation must stay read-only"
    );
    assert_eq!(
        scarce_start.player_work().resource_budget(),
        ample_start.player_work().resource_budget(),
        "hidden reserve size must not change pre-commit labor feasibility for the same request"
    );

    let scarce_job = scarce_start
        .commit(&mut scarce)
        .unwrap_or_else(|error| panic!("scarce mining commit failed: {error}"));
    let ample_job = ample_start
        .commit(&mut ample)
        .unwrap_or_else(|error| panic!("ample mining commit failed: {error}"));
    let scarce_record = scarce
        .mining()
        .get_job(scarce_job)
        .unwrap_or_else(|| panic!("scarce mining job disappeared"));
    let ample_record = ample
        .mining()
        .get_job(ample_job)
        .unwrap_or_else(|| panic!("ample mining job disappeared"));
    assert_eq!(
        scarce_record.completes_at().value() - scarce_record.started_at().value(),
        ample_record.completes_at().value() - ample_record.started_at().value(),
        "requested effort, not hidden recoverable mass, must determine mining duration"
    );
    assert_eq!(
        scarce_record.equipment_condition_after(),
        ample_record.equipment_condition_after(),
        "requested effort, not hidden recoverable mass, must determine tool wear"
    );
    assert_eq!(
        scarce
            .inventory()
            .get_stockpile(scarce_destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(recoverable),
        "the committed job may reveal its now-irreversible actual output reservation"
    );

    let duration = scarce_record.completes_at().value() - scarce_record.started_at().value();
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut scarce)
            .unwrap_or_else(|error| panic!("scarce mining completion failed: {error}"));
    }
    let receipt = validate_claim_mining_output(&registries, &scarce, scarce_job)
        .unwrap_or_else(|error| panic!("scarce mining claim validation failed: {error}"))
        .commit(&mut scarce)
        .unwrap_or_else(|error| panic!("scarce mining claim failed: {error}"));
    assert_eq!(receipt.output().mass(), recoverable);
    assert_eq!(
        scarce
            .geology()
            .get_deposit(scarce_deposit)
            .map(|deposit| deposit.remaining_mass()),
        Some(Mass::ZERO)
    );
    validate_loaded_state(&registries, &scarce)
        .unwrap_or_else(|error| panic!("scarce mining post-claim state invalid: {error}"));
}

fn assemble_quarry_pick_for_test(registries: &Registries, state: &mut AppState) -> EquipmentId {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("quarry-pick assembly source failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
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
        .unwrap_or_else(|error| panic!("quarry-pick assembly material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, EQUIPMENT_STONE_QUARRY_PICK, source)
        .unwrap_or_else(|error| panic!("quarry-pick assembly validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("quarry-pick assembly commit failed: {error}"))
}

#[test]
fn mining_order_projection_matches_executed_quarry_batches() {
    use crate::mining::{MiningOrderRequest, resolve_mining_order};

    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("order survival setup failed: {error}"));
    let pick = assemble_quarry_pick_for_test(&registries, &mut state);
    let method = registries
        .mining()
        .get_method(MINING_METHOD_HAND_PICK)
        .unwrap_or_else(|| panic!("mining method missing"));
    let equipment = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_QUARRY_PICK)
        .unwrap_or_else(|| panic!("quarry definition missing"));
    let crate::capability::CapabilityValue::Mass(batch) = equipment
        .capabilities()
        .get_capability(method.max_batch_mass_capability())
        .unwrap_or_else(|| panic!("batch capability missing"))
    else {
        panic!("batch kind changed")
    };
    let order = Mass::from_milligrams(batch.milligrams() * 40 + batch.milligrams() / 2);
    let destination = add_solid_stockpile_for_test(&mut state, order)
        .unwrap_or_else(|error| panic!("order destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec_with_mass(order))
        .unwrap_or_else(|error| panic!("order deposit failed: {error}"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(deposit_spec().bounds(), MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("order target failed: {error}"));
    let hardness = target
        .excavation_hardness()
        .unwrap_or_else(|| panic!("acquired hardness missing"))
        .upper();
    let projection = resolve_mining_order(
        registries.core().physical_tick_duration(),
        method,
        equipment,
        MiningOrderRequest::new(Condition::PRISTINE, hardness, order, batch, 41),
    )
    .unwrap_or_else(|error| panic!("order projection failed: {error}"));
    let started = state.tick();
    let mut remaining = order;
    let mut batches = 0;
    while !remaining.is_zero() {
        let requested = remaining.min(batch);
        let job = validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            requested,
        )
        .unwrap_or_else(|error| panic!("order admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("order commit failed: {error}"));
        let completes = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("order job missing"))
            .completes_at();
        while state.tick() < completes {
            let outcome = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("order tick failed: {error}"));
            assert_eq!(
                outcome.ready_mining_jobs().contains(&job),
                state.tick() == completes
            );
        }
        let receipt = validate_claim_mining_output(&registries, &state, job)
            .unwrap_or_else(|error| panic!("order claim failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("order landing failed: {error}"));
        assert_eq!(receipt.output().mass(), requested);
        remaining = remaining
            .checked_sub(requested)
            .unwrap_or_else(|| panic!("order remainder underflow"));
        batches += 1;
    }
    assert_eq!(
        projection.duration(),
        state
            .tick()
            .checked_duration_since(started)
            .unwrap_or_else(|| panic!("order duration underflow"))
    );
    assert_eq!(projection.batches(), batches);
    assert_eq!(
        Some(projection.condition_after()),
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition())
    );
    assert!(projection.condition_after() < Condition::PRISTINE);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("executed order invalid: {error}"));
}

#[test]
fn heavy_quarry_pick_reduces_bulk_soft_rock_attention_through_canonical_mining() {
    let registries = build_registries();
    let mass = Mass::from_milligrams(200_000);

    let duration_for = |quarry: bool| {
        let mut state = AppState::new();
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("bulk-mining survival setup failed: {error}"));
        let equipment = if quarry {
            assemble_quarry_pick_for_test(&registries, &mut state)
        } else {
            assemble_pick_for_test(&registries, &mut state)
        };
        let destination = add_solid_stockpile_for_test(&mut state, mass)
            .unwrap_or_else(|error| panic!("bulk-mining destination failed: {error}"));
        let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
            .unwrap_or_else(|error| panic!("bulk-mining deposit failed: {error}"));
        let start_tick = state.tick();
        let token = validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            equipment,
            mass,
        )
        .unwrap_or_else(|error| panic!("bulk-mining validation failed: {error}"));
        let budget = token.player_work().resource_budget();
        let job = token
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("bulk-mining commit failed: {error}"));
        let record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("bulk-mining job disappeared"));
        (record.completes_at().value() - start_tick.value(), budget)
    };

    let (stone_ticks, stone_budget) = duration_for(false);
    let (quarry_ticks, quarry_budget) = duration_for(true);
    assert_eq!(stone_ticks, 3);
    assert_eq!(quarry_ticks, 2);
    assert!(quarry_budget.metabolic_energy() < stone_budget.metabolic_energy());
    assert!(quarry_budget.hydration() < stone_budget.hydration());
}

fn insert_known_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, crate::geology::InsertGeneratedDepositError> {
    let region = spec.bounds();
    let excavation_hardness = spec.excavation_hardness();
    let min = region.min();
    let localized = VoxelBounds::new(min, VoxelCoord::new(min.x() + 1, min.y() + 1, min.z() + 1))
        .unwrap_or_else(|error| panic!("mining known-deposit localized bounds failed: {error}"));
    let material = spec.commodity().material();
    let abundance = spec.composition().parts_per_million(material);
    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .copied()
        .unwrap_or_else(|| panic!("authored detailed prospecting definition disappeared"));
    let uncertainty = detailed.abundance_uncertainty_ppm();
    let estimate = MaterialAbundanceEstimate::new(
        material,
        abundance.saturating_sub(uncertainty),
        abundance.saturating_add(uncertainty).min(1_000_000),
    )
    .unwrap_or_else(|error| panic!("mining known-deposit estimate failed: {error}"));
    let hardness_resolution = detailed
        .excavation_hardness_resolution()
        .unwrap_or_else(|| panic!("authored detailed prospecting hardness resolution disappeared"));
    let resolution_pa = hardness_resolution.pascals();
    let hardness_pa = excavation_hardness.pascals();
    let lower_pa = hardness_pa
        .saturating_sub(1)
        .checked_div(resolution_pa)
        .and_then(|bucket| bucket.checked_mul(resolution_pa))
        .unwrap_or_else(|| panic!("mining known-deposit hardness lower bucket overflowed"));
    let upper_pa = if hardness_pa.is_multiple_of(resolution_pa) {
        hardness_pa
    } else {
        hardness_pa
            .checked_div(resolution_pa)
            .and_then(|bucket| bucket.checked_add(1))
            .and_then(|bucket| bucket.checked_mul(resolution_pa))
            .unwrap_or(u64::MAX)
    };
    let deposit = crate::geology::insert_generated_deposit(registries, state, spec)?;
    let evidence = ProspectingResolution::new_for_fixture(
        localized,
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate],
    )
    .with_excavation_hardness_for_fixture(
        ExcavationHardnessEstimate::new(
            Pressure::from_pascals(lower_pa),
            Pressure::from_pascals(upper_pa),
        )
        .unwrap_or_else(|error| panic!("mining known-deposit hardness evidence failed: {error}")),
    );
    record_prospecting_for_test(registries, state, evidence)
        .unwrap_or_else(|error| panic!("mining known-deposit evidence failed: {error}"));
    Ok(deposit)
}

fn insert_surface_known_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> Result<GeologicalDepositId, crate::geology::InsertGeneratedDepositError> {
    let region = spec.bounds();
    let min = region.min();
    let localized = VoxelBounds::new(min, VoxelCoord::new(min.x() + 1, min.y() + 1, min.z() + 1))
        .unwrap_or_else(|error| panic!("surface-known mining bounds failed: {error}"));
    let material = spec.commodity().material();
    let abundance = spec.composition().parts_per_million(material);
    let inspection = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .copied()
        .unwrap_or_else(|| panic!("authored field-inspection definition disappeared"));
    let uncertainty = inspection.abundance_uncertainty_ppm();
    let deposit = crate::geology::insert_generated_deposit(registries, state, spec)?;
    let estimate = MaterialAbundanceEstimate::new(
        material,
        abundance.saturating_sub(uncertainty),
        abundance.saturating_add(uncertainty).min(1_000_000),
    )
    .unwrap_or_else(|error| panic!("surface-known mining estimate failed: {error}"));
    let evidence = ProspectingResolution::new_for_fixture(
        localized,
        GeologicalEvidenceKind::SurfaceExposure,
        vec![estimate],
    );
    record_prospecting_for_test(registries, state, evidence)
        .unwrap_or_else(|error| panic!("surface-known mining evidence failed: {error}"));
    Ok(deposit)
}

fn record_local_hardness_evidence(
    registries: &Registries,
    state: &mut AppState,
    region: VoxelBounds,
    material: MaterialId,
    lower: Pressure,
    upper: Pressure,
) {
    let estimate = MaterialAbundanceEstimate::new(material, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("local hardness abundance fixture failed: {error}"));
    let hardness = ExcavationHardnessEstimate::new(lower, upper)
        .unwrap_or_else(|error| panic!("local hardness band fixture failed: {error}"));
    let evidence = ProspectingResolution::new_for_fixture(
        region,
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate],
    )
    .with_excavation_hardness_for_fixture(hardness);
    record_prospecting_for_test(registries, state, evidence)
        .unwrap_or_else(|error| panic!("local hardness evidence failed: {error}"));
}

fn active_stockpile_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(8, 0, 0), VoxelCoord::new(9, 1, 1))
        .unwrap_or_else(|error| panic!("mining support bounds failed: {error}"));
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
    .unwrap_or_else(|error| panic!("mining support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("mining support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("mining support activation commit failed: {error}"));
    support
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

fn unstarted_mining_fixture() -> (
    Registries,
    AppState,
    GeologicalDepositId,
    StockpileId,
    EquipmentId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining exhaustion survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining exhaustion destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining exhaustion deposit failed: {error}"));
    (registries, state, deposit, destination, pick)
}

#[test]
fn mining_rejects_player_outside_resolved_deposit() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let bounds = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.bounds())
        .unwrap_or_else(|| panic!("remote-player mining deposit disappeared"));
    let player_position = VoxelCoord::new(10, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-player mining logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-player mining logistics commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::PlayerOutsideDeposit {
            player_position,
            deposit,
            bounds,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mining_rejects_known_remote_equipment() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let player_position = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.bounds().min())
        .unwrap_or_else(|| panic!("remote-tool mining deposit disappeared"));
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-tool mining logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-tool mining logistics commit failed: {error}"));
    let equipment_position = VoxelCoord::new(
        player_position.x() + 1,
        player_position.y(),
        player_position.z(),
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        pick,
        equipment_position,
    );
    let before = state.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::EquipmentAccess(
            PlayerEquipmentAccessError::RemoteKnownEquipment {
                equipment: pick,
                equipment_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn mining_rejects_known_remote_output_destination() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let player_position = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.bounds().min())
        .unwrap_or_else(|| panic!("remote-output mining deposit disappeared"));
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-output mining logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-output mining logistics commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        pick,
        player_position,
    );
    let destination_position = VoxelCoord::new(
        player_position.x() + 1,
        player_position.y(),
        player_position.z(),
    );
    validate_place_ground_stockpile(&state, destination, destination_position)
        .unwrap_or_else(|error| {
            panic!("remote-output mining destination placement failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-output mining destination placement commit failed: {error}")
        });
    let before = state.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::DestinationAccess(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: destination,
                stockpile_position: destination_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn mining_token_rejects_logistics_change_before_commit() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let player_position = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.bounds().min())
        .unwrap_or_else(|| panic!("stale-logistics mining deposit disappeared"));
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale-logistics mining setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-logistics mining commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        pick,
        player_position,
    );
    validate_place_ground_stockpile(&state, destination, player_position)
        .unwrap_or_else(|error| panic!("stale-logistics destination placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("stale-logistics destination placement commit failed: {error}")
        });
    let validated = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("stale-logistics mining validation failed: {error}"));
    let expected = state.logistics().revision();
    validate_allocate_ground_stockpile(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale-logistics mining allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-logistics mining allocation commit failed: {error}"));
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(MiningStartCommitError::StaleLogistics { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_working_mining_with_player_outside_deposit() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let bounds = state
        .geology()
        .get_deposit(deposit)
        .map(|record| record.bounds())
        .unwrap_or_else(|| panic!("remote-load mining deposit disappeared"));
    let player_position = bounds.min();
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote-load mining logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote-load mining logistics commit failed: {error}"));
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_equipment_placement(
        revision,
        revision + 1,
        pick,
        player_position,
    );
    validate_place_ground_stockpile(&state, destination, player_position)
        .unwrap_or_else(|error| panic!("remote-load mining destination placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("remote-load mining destination placement commit failed: {error}")
        });
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("remote-load mining validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote-load mining commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let remote_position = VoxelCoord::new(10, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("remote-load mining serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["position"] =
        serde_json::json!({"x": 10, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote-load mining decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::WorkingPlayerOutsideDeposit {
                job,
                player_position: remote_position,
                bounds,
            }
        )))
    );
}

#[path = "execution_tests/claim_contracts.rs"]
mod claim_contracts;

#[path = "execution_tests/start_commit.rs"]
mod start_commit;

#[path = "execution_tests/continuation.rs"]
mod continuation;

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

#[path = "execution_tests/gameplay.rs"]
mod gameplay;
