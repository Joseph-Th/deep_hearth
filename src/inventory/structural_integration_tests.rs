//! Contract tests for inventory-owned structural loads.

use std::ops::Deref;

use super::*;
use crate::content::{
    FORM_FOOD, FORM_LOG, MATERIAL_BERRIES, MATERIAL_WOOD, STANDARD_TEST_HEATER,
    STANDARD_TEST_HEATING_ENERGY, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::TickSpan;
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::add_equipment;
use crate::inventory::{
    MaterialLotSelection, MaterialRelocationCommitError, MaterialRelocationError,
    MaterialRelocationTestError, add_solid_stockpile_for_test, deposit_lot_for_test,
    validate_material_relocation_for_test,
};
use crate::logistics::{validate_initialize_player_logistics, validate_place_ground_stockpile};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessId, ProcessResolution, ProductionAvailabilityChange, ProductionSuspensionReason,
    StartProcessError, validate_start_process,
};
use crate::simulation::{TickError, advance_tick};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralCommitError, StructuralLifecycle, StructuralLoadKind, StructuralMutationError,
    add_structural_element, calculate_aggregate_weight_force_ceiling,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_remove_structural_element, validate_set_structural_load,
};
use crate::survival::{FoodFreshness, assess_food_freshness};
use crate::thermal::{
    ResolvedSensibleHeating, SensibleHeatingRequest, resolve_sensible_heating_process,
};

const STRUCTURAL_TEST_TARGET: Temperature = Temperature::from_millikelvin(500_000);

struct StructuralHeatingResolution(ResolvedSensibleHeating);

impl Deref for StructuralHeatingResolution {
    type Target = ProcessResolution;

    fn deref(&self) -> &Self::Target {
        self.0.process_resolution()
    }
}

#[test]
fn ground_stockpile_requires_explicit_location_transition_before_mounting() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000))
        .unwrap_or_else(|error| panic!("ground stockpile fixture failed: {error}"));
    let position = VoxelCoord::new(4, 0, -2);
    validate_place_ground_stockpile(&state, stockpile, position)
        .unwrap_or_else(|error| panic!("ground stockpile placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("ground stockpile placement commit failed: {error}"));
    let support = active_support(&registries, &mut state, 0);

    assert_eq!(
        validate_mount_stockpile(&registries, &state, stockpile, support).err(),
        Some(StockpileSupportError::GroundLocated {
            stockpile,
            position,
        })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn validated_mount_rejects_ground_location_added_before_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10_000))
        .unwrap_or_else(|error| panic!("stale ground-mount stockpile failed: {error}"));
    let support = active_support(&registries, &mut state, 0);
    let validated = validate_mount_stockpile(&registries, &state, stockpile, support)
        .unwrap_or_else(|error| panic!("stale ground-mount validation failed: {error}"));
    let position = VoxelCoord::new(8, 0, 1);
    validate_place_ground_stockpile(&state, stockpile, position)
        .unwrap_or_else(|error| panic!("stale ground-mount placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale ground-mount placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(StockpileSupportCommitError::GroundLocated {
            stockpile,
            position,
        })
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn player_carried_stockpile_cannot_also_be_structurally_mounted() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(0, 0, 0),
        Mass::from_milligrams(10_000),
    )
    .unwrap_or_else(|error| panic!("carried-stockpile logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("carried-stockpile logistics commit failed: {error}"))
    .carried_stockpile();
    let support = active_support(&registries, &mut state, 0);

    assert_eq!(
        validate_mount_stockpile(&registries, &state, carried, support).err(),
        Some(StockpileSupportError::PlayerCarried { stockpile: carried })
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn suspended_production_rechecks_inventory_revision_capacity_before_resume() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_013));
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let recovery_support = active_support(&registries, &mut state, 2);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::from_milligrams(10),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, destination, support);
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_013),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );
    let _ = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("resume-budget production start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("resume-budget production commit failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("resume-budget support overload failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("resume-budget support overload commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("resume-budget suspension tick failed: {error}"));
    assert!(
        state
            .production()
            .jobs()
            .next()
            .is_some_and(|job| job.is_suspended())
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("resume-budget destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("resume-budget destination unmount commit failed: {error}"));
    let _ = mount(&registries, &mut state, destination, recovery_support);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("resume-budget serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("resume-budget decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("suspended work should not reserve an unscheduled completion revision: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        advance_tick(&registries, &mut loaded),
        Err(TickError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

fn active_support(registries: &Registries, state: &mut AppState, x: i64) -> StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("stockpile support bounds fixture failed: {error}"),
    };
    let element = match add_structural_element(
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
    ) {
        Ok(element) => element,
        Err(error) => panic!("stockpile support element fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(activation) => activation,
        Err(error) => panic!("stockpile support activation fixture failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("stockpile support activation commit failed: {error}");
    }
    element
}

fn resolve_structural_heating(
    registries: &Registries,
    state: &mut AppState,
    process: ProcessId,
    source: StockpileId,
    mass: Mass,
    target: Temperature,
) -> StructuralHeatingResolution {
    let lot =
        state.inventory().lot_ids(source).next().unwrap_or_else(|| {
            panic!("structural production source {} has no lot", source.value())
        });
    let equipment = add_equipment(registries, state, STANDARD_TEST_HEATER, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("structural production heater fixture failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        registries,
        state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("structural production energy fixture failed: {error}"));
    let resolved = resolve_sensible_heating_process(
        registries,
        state,
        SensibleHeatingRequest::new(
            process,
            source,
            &[MaterialLotSelection::new(lot, mass)],
            equipment,
            energy,
            target,
        ),
    )
    .unwrap_or_else(|error| panic!("structural production heating resolution failed: {error}"));
    StructuralHeatingResolution(resolved)
}

fn seeded_stockpile(
    registries: &Registries,
    state: &mut AppState,
    capacity: Mass,
    mass: Mass,
) -> StockpileId {
    let stockpile = match add_solid_stockpile_for_test(state, capacity) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("stockpile support storage fixture failed: {error}"),
    };
    if !mass.is_zero()
        && let Err(error) = deposit_lot_for_test(
            registries,
            state,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            mass,
            Temperature::from_millikelvin(293_150),
        )
    {
        panic!("stockpile support material fixture failed: {error}");
    }
    stockpile
}

fn mount(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    element: StructuralElementId,
) -> StockpileSupportOutcome {
    let token = match validate_mount_stockpile(registries, state, stockpile, element) {
        Ok(token) => token,
        Err(error) => panic!("stockpile mount validation failed: {error}"),
    };
    match token.commit(state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("stockpile mount commit failed: {error}"),
    }
}

fn expected_weight(registries: &Registries, mass: Mass) -> Force {
    match calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(mass),
        registries.core().gravity(),
    ) {
        Some(force) => force,
        None => panic!("stockpile support expected weight overflowed"),
    }
}

#[test]
fn multiple_stockpiles_aggregate_mass_before_rounding_weight() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let first = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::from_milligrams(1),
    );
    let second = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::from_milligrams(1),
    );

    let _ = mount(&registries, &mut state, first, support);
    let structural_revision_after_first = state.structures().revision();
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::from_millinewtons(1))
    );
    let _ = mount(&registries, &mut state, second, support);
    assert_eq!(
        state.structures().revision(),
        structural_revision_after_first,
        "aggregate mass may change without inventing a structural revision when rounded force is unchanged"
    );

    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::from_millinewtons(1))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn same_tick_production_completions_apply_one_aggregate_destination_load() {
    let process_id = ProcessId::new(971_006);
    let registries = make_test_registries_with_standard_sensible_heating(process_id);
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(40),
        Mass::from_milligrams(20),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(40),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, destination, support);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );

    let mut duration = None;
    for _ in 0..2 {
        let resolution = resolve_structural_heating(
            &registries,
            &mut state,
            process_id,
            source,
            Mass::from_milligrams(10),
            STRUCTURAL_TEST_TARGET,
        );
        match duration {
            Some(expected) => assert_eq!(resolution.duration(), expected),
            None => duration = Some(resolution.duration()),
        }
        validate_start_process(&registries, &state, &resolution, source, destination)
            .unwrap_or_else(|error| panic!("same-tick supported production start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("same-tick supported production commit failed: {error}")
            });
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(20))
    );

    let duration = duration.unwrap_or_else(|| panic!("same-tick production fixture had no jobs"));
    for _ in 1..duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("same-tick pre-completion failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("same-tick supported completion failed: {error}"));
    assert_eq!(outcome.production_completions().len(), 2);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(20))
    );
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(20))),
        "same-tick production must validate and commit the aggregate final destination load"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn new_production_rejects_failed_destination_support() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_002));
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::from_milligrams(10),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, destination, support);
    let overload = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    ) {
        Ok(overload) => overload,
        Err(error) => panic!("failed destination overload validation failed: {error}"),
    };
    if let Err(error) = overload.commit(&mut state) {
        panic!("failed destination overload commit failed: {error}");
    }
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_002),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );

    assert!(matches!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::StructuralLoad(
            StockpileStructuralLoadError::SupportNotActiveForIncrease {
                stockpile,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
        )) if stockpile == destination && element == support
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(state.production().jobs().count(), 0);
}

#[test]
fn validated_production_start_rejects_destination_support_collapse_before_commit() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_004));
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::from_milligrams(10),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, destination, support);
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_004),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );
    let start = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(start) => start,
        Err(error) => panic!("stale destination support start validation failed: {error}"),
    };

    let overload = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    ) {
        Ok(overload) => overload,
        Err(error) => panic!("stale destination support overload validation failed: {error}"),
    };
    if let Err(error) = overload.commit(&mut state) {
        panic!("stale destination support overload commit failed: {error}");
    }
    let source_before_commit = state
        .inventory()
        .get_stockpile(source)
        .map(|record| record.stored_mass());

    assert!(matches!(
        start.commit(&mut state),
        Err(
            crate::production::StartProcessCommitError::StaleStructureRevision {
                expected: _expected,
                actual: _actual,
            }
        )
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        source_before_commit
    );
    assert_eq!(state.production().jobs().count(), 0);
}

#[test]
fn production_suspends_until_failed_destination_support_is_recovered() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_003));
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let recovery_support = active_support(&registries, &mut state, 2);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::from_milligrams(10),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, destination, support);
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_003),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );
    let duration = resolution.duration();
    let start = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(start) => start,
        Err(error) => panic!("committed destination start validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("committed destination start commit failed: {error}");
    }

    let overload = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    ) {
        Ok(overload) => overload,
        Err(error) => panic!("committed destination overload validation failed: {error}"),
    };
    if let Err(error) = overload.commit(&mut state) {
        panic!("committed destination overload commit failed: {error}");
    }
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );

    let suspended = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("failed destination suspension tick failed: {error}"));
    let job = state
        .production()
        .jobs()
        .next()
        .unwrap_or_else(|| panic!("production job disappeared instead of suspending"));
    assert_eq!(
        suspended.production_availability_changes(),
        &[ProductionAvailabilityChange::Suspended {
            job: job.id(),
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
            suspended_at: crate::core::time::SimulationTick::new(0),
            remaining_active_time: duration,
        }]
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );
    assert_eq!(state.production().jobs().count(), 1);
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("suspended destination unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("suspended destination unmount commit failed: {error}"));
    let _ = mount(&registries, &mut state, destination, recovery_support);

    let mut completion_count = 0;
    for _ in 0..=duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("recovered destination completion failed: {error}"));
        completion_count += outcome.production_completions().len();
        if state.production().jobs().next().is_none() {
            break;
        }
    }
    assert_eq!(completion_count, 1);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(
        state
            .structures()
            .get_element(recovery_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(10)))
    );
    assert_eq!(state.production().jobs().count(), 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn suspended_perishable_work_in_process_keeps_aging_in_wall_clock_time() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_005));
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("perishable suspension source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("perishable suspension destination failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("perishable suspension input failed: {error}"));
    let _ = mount(&registries, &mut state, destination, support);
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_005),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );
    let duration = resolution.duration();
    let job = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("perishable suspension start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("perishable suspension start commit failed: {error}"));

    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("perishable suspension support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("perishable suspension support commit failed: {error}"));
    let suspended = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("perishable suspension transition failed: {error}"));
    assert_eq!(
        suspended.production_availability_changes(),
        &[ProductionAvailabilityChange::Suspended {
            job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable {
                stockpile: destination,
            },
            suspended_at: crate::core::time::SimulationTick::ZERO,
            remaining_active_time: duration,
        }]
    );

    for _ in 0..4 {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("perishable suspended wait failed: {error}"));
        assert!(outcome.production_completions().is_empty());
        assert!(
            state
                .production()
                .get_job(job)
                .is_some_and(|record| record.is_suspended())
        );
    }
    assert_eq!(state.tick(), crate::core::time::SimulationTick::new(5));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::ZERO)
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("perishable suspension recovery failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("perishable suspension recovery commit failed: {error}"));
    for _ in 0..=duration.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("perishable suspension completion failed: {error}"));
        if state.production().get_job(job).is_none() {
            break;
        }
    }
    assert!(state.production().get_job(job).is_none());
    let output_lot = state
        .inventory()
        .lot_ids(destination)
        .next()
        .unwrap_or_else(|| panic!("perishable suspended output disappeared"));
    let output = state
        .inventory()
        .get_lot(output_lot)
        .unwrap_or_else(|| panic!("perishable suspended output record disappeared"));
    assert_eq!(output.created_at(), state.tick());
    assert!(matches!(
        assess_food_freshness(&registries, &state, output_lot),
        Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(state.tick().value())
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn production_moves_supported_weight_with_authoritative_matter_ownership() {
    let registries = make_test_registries_with_standard_sensible_heating(ProcessId::new(971_001));
    let mut state = AppState::new();
    let source_support = active_support(&registries, &mut state, 0);
    let destination_support = active_support(&registries, &mut state, 2);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::from_milligrams(10),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(20),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, source, source_support);
    let _ = mount(&registries, &mut state, destination, destination_support);
    let resolution = resolve_structural_heating(
        &registries,
        &mut state,
        ProcessId::new(971_001),
        source,
        Mass::from_milligrams(10),
        STRUCTURAL_TEST_TARGET,
    );
    let duration = resolution.duration();
    let start = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(start) => start,
        Err(error) => panic!("supported production start validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("supported production start commit failed: {error}");
    }

    assert_eq!(
        state
            .structures()
            .get_element(source_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(destination_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("supported production completion failed: {error}");
        }
    }
    assert_eq!(
        state
            .structures()
            .get_element(destination_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(10)))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn transfer_between_supported_stockpiles_updates_both_loads_atomically() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source_support = active_support(&registries, &mut state, 0);
    let destination_support = active_support(&registries, &mut state, 2);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(300_000),
        Mass::from_milligrams(200_000),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(300_000),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, source, source_support);
    let _ = mount(&registries, &mut state, destination, destination_support);

    let transfer = match validate_material_relocation_for_test(
        &registries,
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(50_000),
    ) {
        Ok(transfer) => transfer,
        Err(error) => panic!("supported transfer validation failed: {error}"),
    };
    if let Err(error) = transfer.commit(&mut state) {
        panic!("supported transfer commit failed: {error}");
    }

    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(150_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        Some(Mass::from_milligrams(50_000))
    );
    assert_eq!(
        state
            .structures()
            .get_element(source_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(150_000)))
    );
    assert_eq!(
        state
            .structures()
            .get_element(destination_support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(50_000)))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn supported_transfer_rejects_stale_structure_before_moving_matter() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source_support = active_support(&registries, &mut state, 0);
    let destination_support = active_support(&registries, &mut state, 2);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(300_000),
        Mass::from_milligrams(200_000),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(300_000),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, source, source_support);
    let _ = mount(&registries, &mut state, destination, destination_support);
    let transfer = match validate_material_relocation_for_test(
        &registries,
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(50_000),
    ) {
        Ok(transfer) => transfer,
        Err(error) => panic!("stale supported transfer validation failed: {error}"),
    };
    let source_before = state
        .inventory()
        .get_stockpile(source)
        .map(|record| record.stored_mass());
    let destination_before = state
        .inventory()
        .get_stockpile(destination)
        .map(|record| record.stored_mass());

    let snow = match validate_set_structural_load(
        &registries,
        &state,
        source_support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(1),
    ) {
        Ok(snow) => snow,
        Err(error) => panic!("stale supported transfer mutation failed: {error}"),
    };
    if let Err(error) = snow.commit(&mut state) {
        panic!("stale supported transfer mutation commit failed: {error}");
    }

    assert!(matches!(
        transfer.commit(&mut state),
        Err(MaterialRelocationCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            }
        ))
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        source_before
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        destination_before
    );
}

#[test]
fn empty_stockpile_mount_rejects_stale_structure_without_a_load_delta() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let stockpile = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::ZERO,
    );
    let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
        Ok(mount) => mount,
        Err(error) => panic!("empty stale mount validation failed: {error}"),
    };

    let snow = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(1),
    ) {
        Ok(snow) => snow,
        Err(error) => panic!("empty stale mount structural mutation failed: {error}"),
    };
    if let Err(error) = snow.commit(&mut state) {
        panic!("empty stale mount structural mutation commit failed: {error}");
    }

    assert!(matches!(
        mount.commit(&mut state),
        Err(StockpileSupportCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            }
        ))
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        None
    );
}

#[test]
fn same_support_transfer_binds_structure_even_when_aggregate_weight_is_unchanged() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::from_milligrams(2),
    );
    let destination = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::ZERO,
    );
    let _ = mount(&registries, &mut state, source, support);
    let _ = mount(&registries, &mut state, destination, support);
    let transfer = match validate_material_relocation_for_test(
        &registries,
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
    ) {
        Ok(transfer) => transfer,
        Err(error) => panic!("same-support transfer validation failed: {error}"),
    };
    let before_source = state
        .inventory()
        .get_stockpile(source)
        .map(|record| record.stored_mass());
    let before_destination = state
        .inventory()
        .get_stockpile(destination)
        .map(|record| record.stored_mass());

    let snow = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(1),
    ) {
        Ok(snow) => snow,
        Err(error) => panic!("same-support stale mutation failed: {error}"),
    };
    if let Err(error) = snow.commit(&mut state) {
        panic!("same-support stale mutation commit failed: {error}");
    }

    assert!(matches!(
        transfer.commit(&mut state),
        Err(MaterialRelocationCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            }
        ))
    ));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| record.stored_mass()),
        before_source
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.stored_mass()),
        before_destination
    );
}

#[test]
fn stored_matter_load_is_inventory_owned_and_blocks_support_removal() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let stockpile = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(100),
        Mass::from_milligrams(10),
    );
    let _ = mount(&registries, &mut state, stockpile, support);

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            support,
            StructuralLoadKind::StoredMatter,
            Force::ZERO,
        ),
        Err(StructuralMutationError::LoadOwnedBySubsystem {
            kind: StructuralLoadKind::StoredMatter,
        })
    );
    assert_eq!(
        validate_remove_structural_element(&registries, &state, support),
        Err(StructuralMutationError::ElementSupportsStockpile {
            element: support,
            stockpile,
        })
    );
}

#[test]
fn overload_from_stored_matter_can_fail_support_and_failed_debris_can_be_unloaded() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_support(&registries, &mut state, 0);
    let mass = Mass::from_milligrams(5_000_000_000);
    let stockpile = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(6_000_000_000),
        mass,
    );

    let outcome = mount(&registries, &mut state, stockpile, support);
    assert!(outcome.structural_analysis().is_some());
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        Some(support)
    );

    let source = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(1),
        Mass::from_milligrams(1),
    );
    let before_rejected_transfer = state.clone();
    assert!(matches!(
        validate_material_relocation_for_test(
            &registries,
            &state,
            source,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ),
        Err(MaterialRelocationTestError::Relocation(
            MaterialRelocationError::StructuralLoad(
                StockpileStructuralLoadError::SupportNotActiveForIncrease {
                    stockpile: rejected_stockpile,
                    element,
                    lifecycle: StructuralLifecycle::Failed,
                }
            )
        )) if rejected_stockpile == stockpile && element == support
    ));
    assert_eq!(state, before_rejected_transfer);

    let unmount = match validate_unmount_stockpile(&registries, &state, stockpile) {
        Ok(unmount) => unmount,
        Err(error) => panic!("failed-support unmount validation failed: {error}"),
    };
    if let Err(error) = unmount.commit(&mut state) {
        panic!("failed-support unmount commit failed: {error}");
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        None
    );
    assert_eq!(
        state.structures().get_element(support).map(|record| (
            record.lifecycle(),
            record.load(StructuralLoadKind::StoredMatter),
        )),
        Some((StructuralLifecycle::Failed, Force::ZERO))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[cfg(feature = "test-soak")]
#[path = "structural_integration_tests/soak.rs"]
mod soak;
