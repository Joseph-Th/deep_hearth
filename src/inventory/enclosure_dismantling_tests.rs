//! Recovery, preservation-history, and atomicity contracts for storage enclosure dismantling.

use super::*;

use crate::content::{
    FORM_BOARD, FORM_CHEST_BODY, FORM_CHIP, FORM_DOUBLE_WALL_CHEST_BODY, FORM_FOOD, FORM_LOG,
    MATERIAL_BERRIES, MATERIAL_WOOD, PROCESS_ASSEMBLE_TIMBER_CHEST,
    PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY, PROCESS_SHAPE_WOOD_BOARDS,
    STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST, STORAGE_TIMBER_PROVISIONS_CHEST,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{AggregateMass, Area, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::core::time::{TickSpan, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::energy::calculate_explicit_energy_accounting;
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileStorageProfile, StockpileSupportError,
    add_solid_stockpile_for_test, deposit_lot_for_test, validate_build_storage_enclosure,
    validate_mount_stockpile, validate_unmount_stockpile,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLoadKind, add_structural_element,
    calculate_aggregate_weight_force_ceiling, materialize_structural_element_for_test,
    validate_activate_structural_element,
};
use crate::survival::{FoodFreshness, assess_food_freshness, initialize_player_survival};

const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);
const CHEST_MASS: Mass = Mass::from_milligrams(2_400_000);

fn fixture() -> (
    Registries,
    AppState,
    StockpileId,
    StockpileId,
    StockpileId,
    MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5702_2001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dismantle survival fixture failed: {error}"));
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("dismantle target fixture failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("dismantle food fixture failed: {error}"));
    let construction = add_solid_stockpile_for_test(&mut state, CHEST_MASS)
        .unwrap_or_else(|error| panic!("dismantle construction fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        CHEST_MASS,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("dismantle chest-body fixture failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, CHEST_MASS)
        .unwrap_or_else(|error| panic!("dismantle recovery fixture failed: {error}"));
    (registries, state, target, construction, recovery, food)
}

fn complete_storage_dismantling(
    registries: &Registries,
    state: &mut AppState,
    target: StockpileId,
    recovery: StockpileId,
) -> StorageEnclosureDismantlingOutcome {
    let start = validate_start_storage_enclosure_dismantling(registries, state, target, recovery)
        .unwrap_or_else(|error| panic!("dismantling start validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("dismantling start commit failed: {error}"));
    let completes_at = start.completes_at();
    let mut completion = None;
    while state.tick() < completes_at {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("dismantling completion tick failed: {error}"));
        if let Some(dismantling) = outcome.storage_enclosure_dismantling() {
            assert!(completion.is_none(), "dismantling completed more than once");
            completion = Some(dismantling.clone());
        }
    }
    completion
        .unwrap_or_else(|| panic!("dismantling reached due tick without a completion outcome"))
}

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("dismantle fixture tick failed: {error}"));
    }
}

fn active_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1))
        .unwrap_or_else(|error| panic!("dismantle support bounds failed: {error}"));
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
    .unwrap_or_else(|error| panic!("dismantle support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("dismantle support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("dismantle support activation commit failed: {error}"));
    support
}

#[test]
fn dismantling_checkpoints_preservation_and_recovers_reusable_enclosure_matter() {
    let (registries, mut state, target, construction, recovery, food) = fixture();
    advance_exact(&registries, &mut state, 100);
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("dismantle build validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("dismantle build commit failed: {error}"));
    advance_exact(&registries, &mut state, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(150)
    ));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("dismantle matter-before audit failed: {error}"));
    let energy_before = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("dismantle energy-before audit failed: {error}"))
        .total();

    let start = validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery)
        .unwrap_or_else(|error| panic!("dismantle start validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dismantle start commit failed: {error}"));
    assert_eq!(start.definition(), STORAGE_TIMBER_PROVISIONS_CHEST);
    assert_eq!(start.recovered_mass(), CHEST_MASS);
    assert_eq!(start.completes_at().value() - state.tick().value(), 24);
    assert!(
        state
            .inventory()
            .get_stockpile(target)
            .and_then(|stockpile| stockpile.enclosure())
            .is_some()
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovery)
            .map(|stockpile| (stockpile.stored_mass(), stockpile.reserved_inbound())),
        Some((Mass::ZERO, CHEST_MASS))
    );

    advance_exact(&registries, &mut state, 7);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active dismantle serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active dismantle decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("active dismantle trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let mut completion = None;
    while state.tick() < start.completes_at() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("active dismantle source continuation failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("active dismantle loaded continuation failed: {error}"));
        assert_eq!(actual, expected);
        if let Some(dismantling) = expected.storage_enclosure_dismantling() {
            completion = Some(dismantling.clone());
        }
    }
    assert_eq!(loaded, state);
    let outcome = completion
        .unwrap_or_else(|| panic!("active dismantle continuation did not emit completion"));
    assert_eq!(outcome.definition(), STORAGE_TIMBER_PROVISIONS_CHEST);
    assert_eq!(outcome.recovered_lots().len(), 1);
    let target_record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("dismantled target disappeared"));
    assert_eq!(target_record.enclosure(), None);
    assert_eq!(
        target_record.storage_profile(),
        StockpileStorageProfile::unbounded_solid_only()
    );
    assert_eq!(target_record.embodied_mass(), Mass::ZERO);
    assert_eq!(
        state.inventory().get_stockpile(recovery).map(|stockpile| {
            stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY))
        }),
        Some(CHEST_MASS)
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(162)
    ));
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("dismantle matter-after audit failed: {error}"));
    assert_eq!(matter_after.total(), matter_before.total());
    assert_eq!(matter_after.storage_infrastructure(), AggregateMass::ZERO);
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("dismantle energy-after audit failed: {error}"))
            .total(),
        energy_before
    );

    advance_exact(&registries, &mut loaded, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &loaded, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(262)
    ));
    validate_build_storage_enclosure(
        &registries,
        &loaded,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        recovery,
    )
    .unwrap_or_else(|error| panic!("recovered enclosure rebuild validation failed: {error}"))
    .commit(&mut loaded)
    .unwrap_or_else(|error| panic!("recovered enclosure rebuild commit failed: {error}"));
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(recovery)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert!(matches!(
        assess_food_freshness(&registries, &loaded, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(262)
    ));
    advance_exact(&registries, &mut loaded, 100);
    assert!(matches!(
        assess_food_freshness(&registries, &loaded, food),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(312)
    ));
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("rebuilt enclosure matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn double_wall_enclosure_checkpoints_three_to_one_preservation_and_reuses_exact_body() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5702_2007));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("double-wall dismantle survival setup failed: {error}"));
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("double-wall dismantle target failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        target,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("double-wall dismantle food failed: {error}"));
    let body_mass = Mass::from_milligrams(4_000_000);
    let construction = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall dismantle construction failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("double-wall dismantle body failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall dismantle recovery failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("double-wall dismantle matter setup failed: {error}"))
        .total();
    let food_age = |state: &AppState| match assess_food_freshness(&registries, state, food)
        .unwrap_or_else(|error| panic!("double-wall freshness assessment failed: {error:?}"))
    {
        FoodFreshness::Fresh { age, .. } | FoodFreshness::Spoiled { age } => age,
    };

    advance_exact(&registries, &mut state, 90);
    assert_eq!(food_age(&state), TickSpan::new(90));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("double-wall enclosure build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("double-wall enclosure build commit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("double-wall enclosure built matter failed: {error}"))
            .storage_infrastructure(),
        AggregateMass::from_mass(body_mass)
    );

    advance_exact(&registries, &mut state, 90);
    assert_eq!(
        food_age(&state),
        TickSpan::new(120),
        "90 enclosed ticks at 3x preservation must add exactly 30 effective age ticks"
    );
    let outcome = complete_storage_dismantling(&registries, &mut state, target, recovery);
    assert_eq!(
        outcome.definition(),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST
    );
    assert_eq!(food_age(&state), TickSpan::new(134));
    assert_eq!(
        state.inventory().get_stockpile(recovery).map(|stockpile| {
            stockpile.get_mass(CommodityKey::new(
                MATERIAL_WOOD,
                FORM_DOUBLE_WALL_CHEST_BODY,
            ))
        }),
        Some(body_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("double-wall dismantle matter recovery failed: {error}"))
            .total(),
        matter_before
    );

    advance_exact(&registries, &mut state, 90);
    assert_eq!(food_age(&state), TickSpan::new(224));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        target,
        recovery,
    )
    .unwrap_or_else(|error| panic!("double-wall recovered rebuild failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("double-wall recovered rebuild commit failed: {error}"));
    assert_eq!(food_age(&state), TickSpan::new(224));
    advance_exact(&registries, &mut state, 90);
    assert_eq!(food_age(&state), TickSpan::new(254));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("double-wall rebuilt matter failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn dismantled_double_wall_body_salvages_into_a_standard_enclosure_with_exact_residue() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5702_2008));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("storage salvage survival setup failed: {error}"));
    let body_mass = Mass::from_milligrams(4_000_000);
    let target = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5_000_000))
        .unwrap_or_else(|error| panic!("storage salvage target failed: {error}"));
    let construction = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("storage salvage construction failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
        body_mass,
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("storage salvage body fixture failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("storage salvage recovery failed: {error}"));
    let salvage_output = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("storage salvage output failed: {error}"));
    let rebuilt_body = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("storage salvage rebuilt-body stockpile failed: {error}"));

    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("storage salvage double-wall build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("storage salvage double-wall build commit failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("storage salvage matter-before failed: {error}"))
        .total();
    let dismantled = complete_storage_dismantling(&registries, &mut state, target, recovery);
    assert_eq!(
        dismantled.definition(),
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST
    );
    let recovered_body = *dismantled
        .recovered_lots()
        .first()
        .unwrap_or_else(|| panic!("storage salvage recovered body disappeared"));

    let salvage_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            recovery,
            MaterialLotSelection::new(recovered_body, body_mass),
            salvage_output,
        ),
    )
    .unwrap_or_else(|error| panic!("storage salvage manual recovery failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("storage salvage manual recovery commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(salvage_job)
            .map(|job| job.active_duration()),
        Some(TickSpan::new(100))
    );
    advance_exact(&registries, &mut state, 25);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage salvage serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("storage salvage decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("storage salvage trusted load failed: {error}"));
    assert_eq!(loaded, state);
    for _ in 25..100 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("storage salvage source continuation failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("storage salvage loaded continuation failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(salvage_output)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)) }),
        Some(Mass::from_milligrams(3_200_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(salvage_output)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)) }),
        Some(Mass::from_milligrams(800_000))
    );
    let boards = state
        .inventory()
        .lot_ids(salvage_output)
        .find(|lot| {
            state.inventory().get_lot(*lot).is_some_and(|record| {
                record.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
                    && record.mass() >= CHEST_MASS
            })
        })
        .unwrap_or_else(|| panic!("storage salvage board lot disappeared"));

    let rebuild_job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            salvage_output,
            MaterialLotSelection::new(boards, CHEST_MASS),
            rebuilt_body,
        ),
    )
    .unwrap_or_else(|error| panic!("storage salvage standard joinery failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("storage salvage standard joinery commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(rebuild_job)
            .map(|job| job.active_duration()),
        Some(TickSpan::new(80))
    );
    advance_exact(&registries, &mut state, 80);
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        rebuilt_body,
    )
    .unwrap_or_else(|error| panic!("storage salvage standard enclosure build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("storage salvage standard enclosure commit failed: {error}"));

    let target_record = state
        .inventory()
        .get_stockpile(target)
        .unwrap_or_else(|| panic!("storage salvage target disappeared"));
    assert_eq!(
        target_record
            .enclosure()
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_TIMBER_PROVISIONS_CHEST)
    );
    assert_eq!(
        target_record.embodied_mass(),
        Mass::from_milligrams(2_400_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(salvage_output)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(salvage_output)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("storage salvage matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn dismantling_rejects_capacity_same_target_and_mounted_target_without_mutation() {
    let (registries, mut state, target, construction, recovery, _) = fixture();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("dismantle rejection build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("dismantle rejection build commit failed: {error}"));
    let before_same = state.clone();
    assert_eq!(
        validate_start_storage_enclosure_dismantling(&registries, &state, target, target).err(),
        Some(StorageEnclosureDismantlingError::RecoveryDestinationIsTarget { stockpile: target })
    );
    assert_eq!(state, before_same);

    let too_small = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("small dismantle recovery fixture failed: {error}"));
    let before_capacity = state.clone();
    assert!(matches!(
        validate_start_storage_enclosure_dismantling(&registries, &state, target, too_small),
        Err(StorageEnclosureDismantlingError::RecoveryCapacityExceeded { stockpile, .. })
            if stockpile == too_small
    ));
    assert_eq!(state, before_capacity);

    let support = active_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, target, support)
        .unwrap_or_else(|error| panic!("dismantle mounted target setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dismantle mounted target commit failed: {error}"));
    let before_mounted = state.clone();
    assert_eq!(
        validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery).err(),
        Some(StorageEnclosureDismantlingError::TargetMounted {
            stockpile: target,
            element: support,
        })
    );
    assert_eq!(state, before_mounted);
}

#[test]
fn dismantling_rejects_reserved_inbound_output_without_mutation() {
    let (registries, mut state, target, construction, recovery, _) = fixture();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("reserved dismantle build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reserved dismantle build commit failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("reserved dismantle craft source failed: {error}"));
    let craft_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("reserved dismantle craft material failed: {error}"));
    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_BOARDS,
            source,
            MaterialLotSelection::new(craft_lot, Mass::from_milligrams(1_000_000)),
            target,
        ),
    )
    .unwrap_or_else(|error| panic!("reserved dismantle craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("reserved dismantle craft commit failed: {error}"));
    let reserved = state
        .inventory()
        .get_stockpile(target)
        .map(|stockpile| stockpile.reserved_inbound())
        .unwrap_or_else(|| panic!("reserved dismantle target disappeared"));
    assert!(!reserved.is_zero());
    let before = state.clone();

    assert_eq!(
        validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery).err(),
        Some(StorageEnclosureDismantlingError::TargetHasReservedInbound {
            stockpile: target,
            reserved,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn dismantling_requires_unmounted_resources_and_holds_them_until_completion() {
    let (registries, mut state, target, construction, recovery, _) = fixture();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("loaded-recovery dismantle build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("loaded-recovery dismantle build commit failed: {error}"));
    let support = active_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, recovery, support)
        .unwrap_or_else(|error| panic!("loaded-recovery mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("loaded-recovery mount commit failed: {error}"));
    assert_eq!(
        validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery).err(),
        Some(
            StorageEnclosureDismantlingError::RecoveryDestinationMounted {
                stockpile: recovery,
                element: support,
            }
        )
    );
    let _ = validate_unmount_stockpile(&registries, &state, recovery)
        .unwrap_or_else(|error| panic!("loaded-recovery unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("loaded-recovery unmount commit failed: {error}"));

    let start = validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery)
        .unwrap_or_else(|error| panic!("resource-lock dismantle validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("resource-lock dismantle commit failed: {error}"));
    assert_eq!(
        validate_mount_stockpile(&registries, &state, target, support).err(),
        Some(StockpileSupportError::StockpileBusyStorageDismantling { stockpile: target })
    );
    assert_eq!(
        validate_mount_stockpile(&registries, &state, recovery, support).err(),
        Some(StockpileSupportError::StockpileBusyStorageDismantling {
            stockpile: recovery
        })
    );
    while state.tick() < start.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("resource-lock dismantle tick failed: {error}"));
    }

    let expected = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(CHEST_MASS),
        registries.core().gravity(),
    )
    .unwrap_or_else(|| panic!("loaded-recovery expected weight overflowed"));
    let _ = validate_mount_stockpile(&registries, &state, recovery, support)
        .unwrap_or_else(|error| panic!("completed recovery remount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("completed recovery remount commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| { record.load(StructuralLoadKind::StoredMatter) }),
        Some(expected)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn dismantling_prechecks_inventory_revision_and_lot_id_exhaustion_without_mutation() {
    let (registries, mut base, target, construction, recovery, _) = fixture();
    validate_build_storage_enclosure(
        &registries,
        &base,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("exhaustion dismantle build failed: {error}"))
    .commit(&mut base)
    .unwrap_or_else(|error| panic!("exhaustion dismantle build commit failed: {error}"));

    let mut revision_encoded = serde_json::to_value(SaveEnvelope::new(&registries, &base))
        .unwrap_or_else(|error| {
            panic!("dismantle revision exhaustion serialization failed: {error}")
        });
    revision_encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(revision_encoded)
        .unwrap_or_else(|error| panic!("dismantle revision exhaustion decode failed: {error}"));
    let revision_state = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("dismantle near-exhausted inventory revision fixture should load: {error}")
    });
    let revision_before = revision_state.clone();
    assert_eq!(
        validate_start_storage_enclosure_dismantling(
            &registries,
            &revision_state,
            target,
            recovery,
        )
        .err(),
        Some(StorageEnclosureDismantlingError::InventoryRevisionExhausted)
    );
    assert_eq!(revision_state, revision_before);

    let mut lot_encoded = serde_json::to_value(SaveEnvelope::new(&registries, &base))
        .unwrap_or_else(|error| {
            panic!("dismantle lot-id exhaustion serialization failed: {error}")
        });
    lot_encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(lot_encoded)
        .unwrap_or_else(|error| panic!("dismantle lot-id exhaustion decode failed: {error}"));
    let lot_state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("dismantle exhausted lot-id fixture should load: {error}"));
    let lot_before = lot_state.clone();
    assert_eq!(
        validate_start_storage_enclosure_dismantling(&registries, &lot_state, target, recovery)
            .err(),
        Some(StorageEnclosureDismantlingError::RecoveryLotIdExhausted)
    );
    assert_eq!(lot_state, lot_before);
}

#[test]
fn stale_dismantling_token_cannot_overwrite_later_inventory_mutation() {
    let (registries, mut state, target, construction, recovery, _) = fixture();
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        target,
        construction,
    )
    .unwrap_or_else(|error| panic!("stale dismantle build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale dismantle build commit failed: {error}"));
    let expected_revision = state.inventory().revision();
    let stale = validate_start_storage_enclosure_dismantling(&registries, &state, target, recovery)
        .unwrap_or_else(|error| panic!("stale dismantle validation failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        recovery,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("stale dismantle competing inventory mutation failed: {error}"));
    let before_commit = state.clone();

    assert_eq!(
        stale.commit(&mut state),
        Err(
            StorageEnclosureDismantlingCommitError::StaleInventoryRevision {
                expected: expected_revision,
                actual: expected_revision + 1,
            }
        )
    );
    assert_eq!(state, before_commit);
}
