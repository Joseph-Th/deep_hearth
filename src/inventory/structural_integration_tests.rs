//! Tests for the sibling structural integration module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_LOG, FORM_LUMP, MATERIAL_CHARCOAL, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    build_registries, make_test_registries_with_process,
};
use crate::core::quantity::{Area, Length, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::inventory::{
    MaterialTransferCommitError, MaterialTransferError, add_solid_stockpile_for_test,
    deposit_lot_for_test, validate_material_transfer_for_test,
};
use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
use crate::production::{
    ProcessDefinition, ProcessId, StartProcessError, make_test_process_resolution,
    validate_process_inputs, validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralCommitError, StructuralLifecycle, StructuralMutationError, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_remove_structural_element, validate_set_structural_load,
};

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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0001));
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

    mount(&registries, &mut state, first, support);
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::from_millinewtons(1))
    );
    mount(&registries, &mut state, second, support);

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
fn new_production_rejects_failed_destination_support() {
    let process = ProcessDefinition::new(
        ProcessId::new(971_002),
        "failed destination production fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(0x1A71_0009));
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
    mount(&registries, &mut state, destination, support);
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
    let inputs = match validate_process_inputs(&registries, &state, ProcessId::new(971_002), source)
    {
        Ok(inputs) => inputs,
        Err(error) => panic!("failed destination inputs failed: {error}"),
    };
    let resolution = make_test_process_resolution(
        inputs,
        1,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        )],
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
    let process = ProcessDefinition::new(
        ProcessId::new(971_004),
        "stale destination support fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(0x1A71_0011));
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
    mount(&registries, &mut state, destination, support);
    let inputs = match validate_process_inputs(&registries, &state, ProcessId::new(971_004), source)
    {
        Ok(inputs) => inputs,
        Err(error) => panic!("stale destination support inputs failed: {error}"),
    };
    let resolution = make_test_process_resolution(
        inputs,
        1,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        )],
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
fn committed_production_completes_after_destination_support_fails() {
    let process = ProcessDefinition::new(
        ProcessId::new(971_003),
        "committed failed destination fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(0x1A71_0010));
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
    mount(&registries, &mut state, destination, support);
    let inputs = match validate_process_inputs(&registries, &state, ProcessId::new(971_003), source)
    {
        Ok(inputs) => inputs,
        Err(error) => panic!("committed destination inputs failed: {error}"),
    };
    let resolution = make_test_process_resolution(
        inputs,
        1,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        )],
    );
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

    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("committed output did not complete onto failed destination support: {error}");
    }
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
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(expected_weight(&registries, Mass::from_milligrams(10)))
    );
    assert_eq!(state.production().jobs().count(), 0);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn production_moves_supported_weight_with_authoritative_matter_ownership() {
    let process = ProcessDefinition::new(
        ProcessId::new(971_001),
        "supported stockpile production fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(10),
        )],
        Vec::new(),
    );
    let registries = make_test_registries_with_process(process);
    let mut state = AppState::new(WorldSeed::new(0x1A71_0006));
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
    mount(&registries, &mut state, source, source_support);
    mount(&registries, &mut state, destination, destination_support);
    let inputs = match validate_process_inputs(&registries, &state, ProcessId::new(971_001), source)
    {
        Ok(inputs) => inputs,
        Err(error) => panic!("supported production inputs failed: {error}"),
    };
    let resolution = make_test_process_resolution(
        inputs,
        1,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Mass::from_milligrams(10),
            Temperature::from_millikelvin(500_000),
        )],
    );
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

    if let Err(error) = advance_tick(&registries, &mut state) {
        panic!("supported production completion failed: {error}");
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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0002));
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
    mount(&registries, &mut state, source, source_support);
    mount(&registries, &mut state, destination, destination_support);

    let transfer = match validate_material_transfer_for_test(
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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0003));
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
    mount(&registries, &mut state, source, source_support);
    mount(&registries, &mut state, destination, destination_support);
    let transfer = match validate_material_transfer_for_test(
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
        Err(MaterialTransferCommitError::Structure(
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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0007));
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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0008));
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
    mount(&registries, &mut state, source, support);
    mount(&registries, &mut state, destination, support);
    let transfer = match validate_material_transfer_for_test(
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
        Err(MaterialTransferCommitError::Structure(
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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0004));
    let support = active_support(&registries, &mut state, 0);
    let stockpile = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(100),
        Mass::from_milligrams(10),
    );
    mount(&registries, &mut state, stockpile, support);

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
    let mut state = AppState::new(WorldSeed::new(0x1A71_0005));
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
        validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            stockpile,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ),
        Err(MaterialTransferError::StructuralLoad(
            StockpileStructuralLoadError::SupportNotActiveForIncrease {
                stockpile: rejected_stockpile,
                element,
                lifecycle: StructuralLifecycle::Failed,
            }
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
fn run_supported_transfer_soak(seed: WorldSeed) -> AppState {
    let registries = build_registries();
    let mut state = AppState::new(seed);
    let left_support = active_support(&registries, &mut state, 0);
    let right_support = active_support(&registries, &mut state, 2);
    let left = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::from_milligrams(1),
    );
    let right = seeded_stockpile(
        &registries,
        &mut state,
        Mass::from_milligrams(10),
        Mass::ZERO,
    );
    mount(&registries, &mut state, left, left_support);
    mount(&registries, &mut state, right, right_support);

    for step in 0..1_000_u64 {
        let (source, destination) = if step.is_multiple_of(2) {
            (left, right)
        } else {
            (right, left)
        };
        let transfer = match validate_material_transfer_for_test(
            &registries,
            &state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ) {
            Ok(transfer) => transfer,
            Err(error) => {
                panic!("supported transfer soak validation failed at {step}: {error}")
            }
        };
        if let Err(error) = transfer.commit(&mut state) {
            panic!("supported transfer soak commit failed at {step}: {error}");
        }
        if step.is_multiple_of(113) {
            assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        }
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn supported_transfer_soak_preserves_invariants_and_deterministic_replay() {
    let seed = WorldSeed::new(0x1A71_5000);
    let first = run_supported_transfer_soak(seed);
    let second = run_supported_transfer_soak(seed);
    assert_eq!(first, second);
}
