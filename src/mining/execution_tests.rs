//! Contract tests for mining admission, lifecycle, and claim.

use super::*;
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_QUARRY_PICK, FORM_CHEST_BODY, FORM_FLYWHEEL, FORM_HANDLE, FORM_LOG, FORM_LUMP,
    FORM_ORE, FORM_REINFORCEMENT, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    MINING_METHOD_HAND_PICK, PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_TIMBER_PROVISIONS_CHEST, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Pressure, Temperature, Volume};
use crate::core::state::{AppState, StateValidationError, validate_loaded_state};
use crate::core::time::{SimulationTick, WorldSeed};
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
    let mut state = AppState::new(WorldSeed::new(0xA11E_0112));
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
fn fatal_tick_allows_mining_due_that_tick_to_finish_work() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0113));
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
    let setup = |seed: u64, deposit_mass: Mass| {
        let mut state = AppState::new(WorldSeed::new(seed));
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
    let (mut scarce, scarce_deposit, scarce_destination, scarce_pick) =
        setup(0xA11E_0110, recoverable);
    let (mut ample, ample_deposit, ample_destination, ample_pick) =
        setup(0xA11E_0111, Mass::from_milligrams(1_000_000));

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
    let mut state = AppState::new(WorldSeed::new(2));
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

    let duration_for = |seed: u64, quarry: bool| {
        let mut state = AppState::new(WorldSeed::new(seed));
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

    let (stone_ticks, stone_budget) = duration_for(0xA11E_0100, false);
    let (quarry_ticks, quarry_budget) = duration_for(0xA11E_0101, true);
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
    let deposit = crate::geology::insert_generated_deposit(registries, state, spec)?;
    let estimate = MaterialAbundanceEstimate::new(material, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("mining known-deposit estimate failed: {error}"));
    let evidence = ProspectingResolution::new_for_fixture(
        localized,
        GeologicalEvidenceKind::ExcavationSample,
        vec![estimate],
    )
    .with_excavation_hardness_for_fixture(
        ExcavationHardnessEstimate::new(excavation_hardness, excavation_hardness).unwrap_or_else(
            |error| panic!("mining known-deposit hardness evidence failed: {error}"),
        ),
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
    let deposit = crate::geology::insert_generated_deposit(registries, state, spec)?;
    let estimate = MaterialAbundanceEstimate::new(material, abundance, abundance)
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
    let mut state = AppState::new(WorldSeed::new(0xA11E_E001));
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
fn mining_cannot_reserve_output_into_an_active_dismantling_target() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let construction = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
        .unwrap_or_else(|error| panic!("mining dismantle construction stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        Mass::from_milligrams(2_400_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining dismantle enclosure body failed: {error}"));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        destination,
        construction,
    )
    .unwrap_or_else(|error| panic!("mining dismantle enclosure build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining dismantle enclosure build commit failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
        .unwrap_or_else(|error| panic!("mining dismantle recovery stockpile failed: {error}"));
    let _ =
        validate_start_storage_enclosure_dismantling(&registries, &state, destination, recovery)
            .unwrap_or_else(|error| panic!("mining dismantle start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("mining dismantle start commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(1_000),
        )
        .err(),
        Some(MiningStartError::DestinationBusyStorageDismantling {
            stockpile: destination,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn public_mining_debug_does_not_expose_hidden_source_or_reserved_output() {
    let (registries, mut state, deposit, destination, pick) = unstarted_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(1_000),
    )
    .unwrap_or_else(|error| panic!("mining debug start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining debug commit failed: {error}"));

    let job_debug = format!(
        "{:?}",
        state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("mining debug job disappeared"))
    );
    let owner_debug = format!("{:?}", state.mining());

    for debug in [&job_debug, &owner_debug] {
        assert!(!debug.contains("deposit"));
        assert!(!debug.contains("deposit_mass_before"));
        assert!(!debug.contains("output"));
    }
}

fn ready_mining_claim_fixture() -> (Registries, AppState, MiningJobId) {
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
    .unwrap_or_else(|error| panic!("mining claim exhaustion start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining claim exhaustion start commit failed: {error}"));
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("mining claim exhaustion job disappeared after start"));
    let duration = record.completes_at().value() - record.started_at().value();
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining claim exhaustion completion failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
    (registries, state, job)
}

#[test]
fn mining_start_rejects_exhausted_job_id_without_claiming_work() {
    let (registries, state, deposit, destination, pick) = unstarted_mining_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining job-id exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["next_job_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining job-id exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining job-id exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &loaded,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MiningIdExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(loaded.player_work().active(), None);
}

#[test]
fn mining_start_rejects_exhausted_mining_revision_without_claiming_work() {
    let (registries, state, deposit, destination, pick) = unstarted_mining_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining revision exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining revision exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining revision exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &loaded,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MiningRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(loaded.player_work().active(), None);
}

#[test]
fn mining_start_reserves_scheduled_completion_owner_revisions() {
    let (registries, state, deposit, destination, pick) = unstarted_mining_fixture();
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining revision-budget serialization failed: {error}"));

    for (owner, revision, expected) in [
        (
            "mining",
            u64::MAX - 1,
            MiningStartError::MiningRevisionExhausted,
        ),
        (
            "geology",
            u64::MAX,
            MiningStartError::GeologyRevisionExhausted,
        ),
        (
            "equipment",
            u64::MAX,
            MiningStartError::EquipmentRevisionExhausted,
        ),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(revision);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("mining revision-budget decode failed: {error}"));
        let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
            panic!("idle near-exhausted mining owner should load: {error}")
        });
        let before = loaded.clone();

        assert_eq!(
            validate_known_mining(
                &registries,
                &loaded,
                MINING_METHOD_HAND_PICK,
                deposit,
                destination,
                pick,
                Mass::from_milligrams(100_000),
            )
            .err(),
            Some(expected)
        );
        assert_eq!(loaded, before);
    }
}

#[test]
fn trusted_load_rejects_working_mining_without_scheduled_completion_revisions() {
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
    .unwrap_or_else(|error| panic!("mining load revision-budget start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining load revision-budget commit failed: {error}"));
    let encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining load revision-budget serialization failed: {error}")
        });

    for (owner, expected) in [
        (
            "mining",
            MiningJobValidationError::WorkingMiningRevisionExhausted { job },
        ),
        (
            "geology",
            MiningJobValidationError::WorkingGeologyRevisionExhausted { job },
        ),
        (
            "equipment",
            MiningJobValidationError::WorkingEquipmentRevisionExhausted { job },
        ),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("mining load revision-budget decode failed: {error}"));
        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::MiningJob(
                expected
            )))
        );
    }
}

#[test]
fn mining_claim_rejects_exhausted_lot_id_without_releasing_pending_output() {
    let (registries, state, job) = ready_mining_claim_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining claim lot-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining claim lot-id exhaustion decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("mining claim lot-id exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_claim_mining_output(&registries, &loaded, job).err(),
        Some(MiningClaimError::LotIdExhausted)
    );
    assert_eq!(loaded, before);
    assert!(
        loaded
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
}

#[test]
fn mining_claim_rejects_exhausted_inventory_revision_without_releasing_pending_output() {
    let (registries, state, job) = ready_mining_claim_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining claim inventory revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("mining claim inventory revision exhaustion decode failed: {error}")
    });
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("mining claim inventory revision exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_claim_mining_output(&registries, &loaded, job).err(),
        Some(MiningClaimError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert!(
        loaded
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
}

#[test]
fn mining_claim_rejects_exhausted_mining_revision_without_releasing_pending_output() {
    let (registries, state, job) = ready_mining_claim_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining claim mining revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("mining claim mining revision exhaustion decode failed: {error}")
    });
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("mining claim mining revision exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_claim_mining_output(&registries, &loaded, job).err(),
        Some(MiningClaimError::MiningRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert!(
        loaded
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
}

#[test]
fn resolved_mining_target_survives_unrelated_remote_geological_knowledge() {
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
    let remote = VoxelBounds::new(VoxelCoord::new(100, -8, 0), VoxelCoord::new(101, -7, 1))
        .unwrap_or_else(|error| panic!("stale-target knowledge evidence bounds failed: {error}"));
    let estimate = MaterialAbundanceEstimate::new(MATERIAL_STONE, 1, 1_000_000)
        .unwrap_or_else(|error| panic!("stale-target knowledge estimate failed: {error}"));
    record_prospecting_for_test(
        &registries,
        &mut state,
        ProspectingResolution::new_for_fixture(
            remote,
            GeologicalEvidenceKind::SurfaceExposure,
            vec![estimate],
        ),
    )
    .unwrap_or_else(|error| panic!("stale-target knowledge evidence failed: {error}"));
    let before = state.clone();

    let _validated = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("remote knowledge should not stale local target: {error}"));
    assert_eq!(state, before);
}

#[test]
fn resolved_mining_target_is_invalidated_by_new_local_ambiguity() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0033));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("ambiguous-target survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("ambiguous-target destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("ambiguous-target deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("ambiguous-target deposit disappeared"));
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(
            deposit_record.bounds(),
            deposit_record.commodity().material(),
        ),
    )
    .unwrap_or_else(|error| panic!("ambiguous-target initial resolution failed: {error}"));

    crate::geology::insert_generated_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("ambiguous-target second deposit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_mining_target_is_invalidated_by_better_local_hardness_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0034));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness-stale target survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness-stale target destination failed: {error}"));
    let deposit = insert_surface_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("hardness-stale target deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("hardness-stale target deposit disappeared"));
    let region = deposit_record.bounds();
    let material = deposit_record.commodity().material();
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(300_000_000),
        Pressure::from_pascals(500_000_000),
    );
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale target resolution failed: {error}"));
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(340_000_000),
        Pressure::from_pascals(360_000_000),
    );
    let current = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale target re-resolution failed: {error}"));
    assert_eq!(current.deposit, target.deposit);
    assert_ne!(current, target);
    let before = state.clone();

    assert_eq!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
}

#[test]
fn validated_mining_start_is_invalidated_by_new_geological_knowledge() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0032));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-start knowledge survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("stale-start knowledge destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("stale-start knowledge deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("stale-start knowledge deposit disappeared"));
    let deposit_bounds = deposit_record.bounds();
    let deposit_material = deposit_record.commodity().material();
    let target = resolve_mining_target(
        &state,
        MiningTargetRequest::new(deposit_bounds, deposit_material),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge target resolution failed: {error}"));
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge mining validation failed: {error}"));
    let contradiction = MaterialAbundanceEstimate::new(MATERIAL_COPPER, 0, 0)
        .unwrap_or_else(|error| panic!("stale-start knowledge estimate failed: {error}"));
    record_prospecting_for_test(
        &registries,
        &mut state,
        ProspectingResolution::new_for_fixture(
            deposit_bounds,
            GeologicalEvidenceKind::SurfaceExposure,
            vec![contradiction],
        ),
    )
    .unwrap_or_else(|error| panic!("stale-start knowledge evidence failed: {error}"));
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(MiningStartCommitError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(1_000_000))
    );
}

#[test]
fn validated_mining_start_is_invalidated_by_better_local_hardness_evidence() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0035));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness-stale start survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness-stale start destination failed: {error}"));
    let deposit = insert_surface_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("hardness-stale start deposit failed: {error}"));
    let deposit_record = state
        .geology()
        .get_deposit(deposit)
        .unwrap_or_else(|| panic!("hardness-stale start deposit disappeared"));
    let region = deposit_record.bounds();
    let material = deposit_record.commodity().material();
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(300_000_000),
        Pressure::from_pascals(500_000_000),
    );
    let target = resolve_mining_target(&state, MiningTargetRequest::new(region, material))
        .unwrap_or_else(|error| panic!("hardness-stale start target failed: {error}"));
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("hardness-stale start validation failed: {error}"));
    record_local_hardness_evidence(
        &registries,
        &mut state,
        region,
        material,
        Pressure::from_pascals(340_000_000),
        Pressure::from_pascals(360_000_000),
    );
    let before = state.clone();

    assert_eq!(
        start.commit(&mut state),
        Err(MiningStartCommitError::TargetNoLongerResolved)
    );
    assert_eq!(state, before);
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn validated_mining_start_survives_unrelated_remote_geology_change() {
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
    let start = super::validate_start_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("remote-geology mining validation failed: {error}"));
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

    start.commit(&mut state).unwrap_or_else(|error| {
        panic!("remote geology should not stale validated mining: {error}")
    });
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Mining { .. })
    ));
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
    degrade_equipment_condition_for_test(&mut state, pick, 999_500);
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
fn loaded_mining_state_rejects_equipment_double_booking_after_index_rebuild() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0025));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining double-book survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("mining double-book destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining double-book deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining double-book start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining double-book commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining double-book serialization failed: {error}"));
    let second_job = job.value() + 1;
    let mut duplicated =
        encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()].clone();
    duplicated["identity"]["id"] = serde_json::json!(second_job);
    encoded["state"]["systems"]["mining"]["jobs"][second_job.to_string()] = duplicated;
    encoded["state"]["systems"]["mining"]["next_job_id"] = serde_json::json!(second_job + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining double-book tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Mining(
            MiningValidationError::EquipmentDoubleBooked { equipment: pick }
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
        MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
            observed_upper: Pressure::from_pascals(600_000_000),
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
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining trace completion failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
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
fn loaded_working_mining_job_rejects_forged_source_mass_trace() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0031));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining source-trace survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining source-trace destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining source-trace deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining source-trace start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining source-trace commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining source-trace serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["deposit_mass_before"] =
        serde_json::json!(900_000_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining source-trace tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::WorkingDepositMassMismatch {
                job,
                expected: Mass::from_milligrams(900_000),
                actual: Mass::from_milligrams(1_000_000),
            }
        )))
    );
}

#[test]
fn loaded_working_mining_job_rejects_forged_requested_mass() {
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
    .unwrap_or_else(|error| panic!("mining requested-mass start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining requested-mass commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining requested-mass serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["resources"]["requested_mass"] =
        serde_json::json!(0_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining requested-mass tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::ZeroRequestedMass { job }
        )))
    );
}

#[test]
fn unclaimed_output_allows_follow_on_extraction_from_the_same_deposit() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0038));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("follow-on mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let first_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
            .unwrap_or_else(|error| panic!("first follow-on mining destination failed: {error}"));
    let second_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
            .unwrap_or_else(|error| panic!("second follow-on mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("follow-on mining deposit failed: {error}"));
    let extraction_mass = Mass::from_milligrams(100_000);

    let first_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        first_destination,
        pick,
        extraction_mass,
    )
    .unwrap_or_else(|error| panic!("first follow-on mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first follow-on mining commit failed: {error}"));
    let first_duration = state
        .mining()
        .get_job(first_job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("first follow-on mining job disappeared"));
    for _ in 0..first_duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first follow-on mining tick failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(first_job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(900_000))
    );

    let second_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        second_destination,
        pick,
        extraction_mass,
    )
    .unwrap_or_else(|error| panic!("second follow-on mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second follow-on mining commit failed: {error}"));
    let second_duration = state
        .mining()
        .get_job(second_job)
        .map(|record| record.completes_at().value() - record.started_at().value())
        .unwrap_or_else(|| panic!("second follow-on mining job disappeared"));
    for _ in 0..second_duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("second follow-on mining tick failed: {error}"));
    }

    for job in [first_job, second_job] {
        assert!(
            state
                .mining()
                .get_job(job)
                .is_some_and(MiningJobRecord::is_ready_to_claim),
            "completed extraction {job:?} must remain claimable after later extraction"
        );
    }
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(800_000))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("follow-on mining state audit failed: {error}"));

    let mut forged_mass_history = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| {
            panic!("follow-on mining mass-history serialization failed: {error}")
        });
    forged_mass_history["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["resources"]
        ["deposit_mass_before"] = serde_json::json!(950_000_u64);
    let forged_mass_history: LoadedSaveEnvelope = serde_json::from_value(forged_mass_history)
        .unwrap_or_else(|error| panic!("follow-on mining mass-history decode failed: {error}"));
    assert_eq!(
        forged_mass_history.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::DepositHistoryMassIncrease {
                earlier: first_job,
                later: second_job,
                maximum_later_mass: Mass::from_milligrams(900_000),
                later_mass: Mass::from_milligrams(950_000),
            }
        )))
    );

    let first_record = state
        .mining()
        .get_job(first_job)
        .unwrap_or_else(|| panic!("first follow-on mining job disappeared before schedule tamper"));
    let second_record = state.mining().get_job(second_job).unwrap_or_else(|| {
        panic!("second follow-on mining job disappeared before schedule tamper")
    });
    let forged_second_start = SimulationTick::new(
        second_record
            .started_at()
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("second follow-on mining job unexpectedly starts at zero")),
    );
    let forged_second_completion = SimulationTick::new(
        second_record
            .completes_at()
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("second follow-on mining completion unexpectedly zero")),
    );
    let mut forged_schedule = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("follow-on mining schedule serialization failed: {error}"));
    forged_schedule["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["schedule"]
        ["started_at"] = serde_json::json!(forged_second_start.value());
    forged_schedule["state"]["systems"]["mining"]["jobs"][second_job.value().to_string()]["schedule"]
        ["completes_at"] = serde_json::json!(forged_second_completion.value());
    let forged_schedule: LoadedSaveEnvelope = serde_json::from_value(forged_schedule)
        .unwrap_or_else(|error| panic!("follow-on mining schedule decode failed: {error}"));
    assert_eq!(
        forged_schedule.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::OverlappingRetainedWork {
                earlier: first_job,
                later: second_job,
                earlier_completes: first_record.completes_at(),
                later_starts: forged_second_start,
            }
        )))
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("follow-on mining save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("follow-on mining decode failed: {error}"));
    let restored = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("follow-on mining load failed: {error}"));
    assert_eq!(restored, state);

    validate_claim_mining_output(&registries, &state, first_job)
        .unwrap_or_else(|error| panic!("older follow-on mining claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("older follow-on mining claim commit failed: {error}"));
    validate_claim_mining_output(&registries, &state, second_job)
        .unwrap_or_else(|error| panic!("newer follow-on mining claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("newer follow-on mining claim commit failed: {error}"));
    for destination in [first_destination, second_destination] {
        assert_eq!(
            state.inventory().get_stockpile(destination).map(|record| {
                (
                    record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_ORE)),
                    record.reserved_inbound(),
                )
            }),
            Some((extraction_mass, Mass::ZERO))
        );
    }
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("claimed follow-on mining audit failed: {error}"));
}

#[test]
fn trusted_load_rejects_multiple_working_mining_jobs_before_single_extraction_tick() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0034));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("multiple-mining survival setup failed: {error}"));
    let first_pick = assemble_pick_for_test(&registries, &mut state);
    let second_pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("multiple-mining destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("multiple-mining deposit failed: {error}"));
    let first_job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        first_pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("multiple-mining canonical start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("multiple-mining canonical commit failed: {error}"));
    let first_record = state
        .mining()
        .get_job(first_job)
        .unwrap_or_else(|| panic!("multiple-mining canonical job disappeared"));
    let first_started_at = first_record.started_at();
    let first_completes_at = first_record.completes_at();

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("multiple-mining serialization failed: {error}"));
    let first_key = first_job.value().to_string();
    let second_job = MiningJobId::new(first_job.value() + 1);
    let second_key = second_job.value().to_string();
    let mut duplicate = encoded["state"]["systems"]["mining"]["jobs"][&first_key].clone();
    duplicate["identity"]["id"] = serde_json::json!(second_job.value());
    duplicate["resources"]["equipment_trace"]["equipment"] = serde_json::json!(second_pick.value());
    encoded["state"]["systems"]["mining"]["jobs"][&second_key] = duplicate;
    encoded["state"]["systems"]["mining"]["next_job_id"] =
        serde_json::json!(second_job.value() + 1);
    encoded["state"]["systems"]["inventory"]["stockpiles"][destination.value().to_string()]["reserved_inbound"] =
        serde_json::json!(200_000_u64);
    let forged: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("multiple-mining forged decode failed: {error}"));

    assert_eq!(
        forged.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::MiningJob(
            MiningJobValidationError::OverlappingRetainedWork {
                earlier: first_job,
                later: second_job,
                earlier_completes: first_completes_at,
                later_starts: first_started_at,
            }
        )))
    );
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
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining duration-audit completion failed: {error}"));
    }
    let ready = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("ready mining duration-audit job disappeared"));
    assert!(ready.is_ready_to_claim());
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
                stored: required
                    .checked_sub(crate::core::time::TickSpan::new(1))
                    .unwrap_or_else(|| panic!("mining duration fixture must exceed one tick")),
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
fn stone_pick_refuses_acquired_hardness_above_authored_capability() {
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
        MiningStartError::ExcavationHardnessEvidenceExceedsCapability {
            observed_upper: Pressure::from_pascals(700_000_000),
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
fn mining_requires_acquired_hardness_without_revealing_hidden_resistance() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0012));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hardness-evidence survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hardness-evidence destination failed: {error}"));
    let bounds = VoxelBounds::new(VoxelCoord::new(8, -8, 0), VoxelCoord::new(9, -7, 1))
        .unwrap_or_else(|error| panic!("hardness-evidence bounds failed: {error}"));
    let deposit = insert_surface_known_deposit(
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
        .unwrap_or_else(|error| panic!("hardness-evidence deposit fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("hardness-evidence deposit insertion failed: {error}"));
    let target = resolve_mining_target(&state, MiningTargetRequest::new(bounds, MATERIAL_STONE))
        .unwrap_or_else(|error| panic!("surface-known mining target failed: {error}"));
    assert_eq!(target.excavation_hardness(), None);
    let before = state.clone();

    assert_eq!(
        super::validate_start_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            target,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MissingExcavationHardnessEvidence {
            material: MATERIAL_STONE,
            region: bounds,
        }),
        "read-only mining admission must request physical sampling instead of revealing hidden hardness"
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .map(|record| record.remaining_mass()),
        Some(Mass::from_milligrams(100_000))
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
    let remaining = record
        .completes_at()
        .checked_duration_since(state.tick())
        .unwrap_or_else(|| panic!("mining completion precedes current tick"));
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
        Mass::from_milligrams(1_000_000)
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
    let stone = deposit_lot_for_test(
        &registries,
        &mut state,
        stone_source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining stone ingress failed: {error}"));
    let wood = deposit_lot_for_test(
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
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            stone_source,
            MaterialLotSelection::new(stone, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("mining knapping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining knapping commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining knapping tick failed: {error}"));
    }
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_HANDLE,
            stone_source,
            MaterialLotSelection::new(wood, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
    )
    .unwrap_or_else(|error| panic!("mining handle shaping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining handle shaping commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
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
        Mass::from_milligrams(1_000_000)
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
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            stone_source,
            MaterialLotSelection::new(stone, Mass::from_milligrams(1_000_000)),
            shaped,
        ),
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
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining deposit disappeared after work"))
            .remaining_mass(),
        Mass::from_milligrams(900_000)
    );
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
    let ready_energy = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("ready mining energy accounting failed: {error}"));
    assert_eq!(ready_energy.total(), Some(energy_before_mining));
    assert!(
        !ready_energy.mining_material_thermal().is_zero(),
        "extracted ore must retain explicit thermal ownership while waiting to be claimed"
    );
    let completion_tick = state.tick();
    for _ in 0..3 {
        let delayed = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("delayed mining-claim tick failed: {error}"));
        assert!(delayed.ready_mining_jobs().is_empty());
        assert!(
            state
                .mining()
                .get_job(job)
                .is_some_and(MiningJobRecord::is_ready_to_claim),
            "completed mining output must remain durably mining-owned until claim"
        );
        assert_eq!(state.player_work().active(), None);
        assert_eq!(
            state
                .inventory()
                .get_stockpile(ore_destination)
                .map(|stockpile| stockpile.reserved_inbound()),
            Some(Mass::from_milligrams(100_000)),
            "delayed mining output must retain its destination capacity reservation"
        );
        assert_eq!(
            calculate_matter_accounting(&state)
                .unwrap_or_else(|error| panic!("delayed mining matter audit failed: {error}"))
                .total(),
            matter_before
        );
        validate_loaded_state(&registries, &state)
            .unwrap_or_else(|error| panic!("delayed mining state audit failed: {error}"));
    }

    let delayed_encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("delayed mining save serialization failed: {error}"));
    let delayed_loaded: LoadedSaveEnvelope = serde_json::from_value(delayed_encoded)
        .unwrap_or_else(|error| panic!("delayed mining save decode failed: {error}"));
    let delayed_restored = delayed_loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("delayed mining save validation failed: {error}"));
    assert_eq!(delayed_restored, state);

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
    let claimed_lot = state
        .inventory()
        .lot_ids(ore_destination)
        .next()
        .and_then(|lot| state.inventory().get_lot(lot))
        .unwrap_or_else(|| panic!("claimed mining output lot disappeared"));
    assert_eq!(
        claimed_lot.created_at(),
        completion_tick,
        "delayed claim must preserve physical extraction time as provenance"
    );
    assert_eq!(
        claimed_lot.latest_created_at(),
        completion_tick,
        "delayed claim must not rewrite output provenance to the later claim tick"
    );
    assert_eq!(
        claimed_lot
            .storage_history()
            .project(state.tick(), AMBIENT_PRESERVATION_MULTIPLIER_PPM,),
        Some(3 * STORAGE_AGE_PARTS_PER_TICK),
        "unclaimed output must accumulate ambient storage exposure before inventory admission"
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("claimed mining energy ownership audit failed: {error}"))
            .mining_material_thermal(),
        crate::energy::PreciseEnergy::ZERO,
        "claim must transfer all ready ore thermal ownership out of mining"
    );
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

#[test]
fn ready_mining_output_waits_for_destination_support_recovery() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA11E_0030));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("mining support-recovery destination failed: {error}"));
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("mining support-recovery mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery mount commit failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining support-recovery deposit failed: {error}"));
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining support-recovery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining support-recovery start commit failed: {error}"));
    let duration = state
        .mining()
        .get_job(job)
        .and_then(|record| {
            record
                .completes_at()
                .value()
                .checked_sub(record.started_at().value())
        })
        .unwrap_or_else(|| panic!("mining support-recovery duration was invalid"));
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("mining support-recovery work tick failed: {error}"));
    }
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_ready_to_claim)
    );

    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("mining support-recovery overload failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining support-recovery overload commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    let blocked = state.clone();
    assert!(matches!(
        validate_claim_mining_output(&registries, &state, job),
        Err(MiningClaimError::StructuralLoad(
            StockpileStructuralLoadError::SupportNotActiveForIncrease {
                stockpile,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
        )) if stockpile == destination && element == support
    ));
    assert_eq!(state, blocked);

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery unmount commit failed: {error}"));
    validate_claim_mining_output(&registries, &state, job)
        .unwrap_or_else(|error| panic!("mining support-recovery claim failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining support-recovery claim commit failed: {error}"));
    assert!(state.mining().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| (record.stored_mass(), record.reserved_inbound())),
        Some((Mass::from_milligrams(100_000), Mass::ZERO))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
            let _ = advance_tick(&registries, &mut state)
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
