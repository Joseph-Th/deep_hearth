//! Tests for the sibling deconstruction execution module; isolated so test-only edits do not invalidate production builds.

use super::super::construction_execution::bind_structural_construction_selection;
use super::*;
use crate::content::{
    FORM_LOG, FORM_SCRAP, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::energy::{ExplicitEnergyAccountingError, calculate_explicit_energy_accounting};
use crate::inventory::{
    MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test,
    validate_mount_stockpile,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralConstructionError, StructuralLifecycle, StructuralLoadKind, StructuralMutationError,
    add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_remove_structural_element,
    validate_set_structural_load, validate_structural_construction,
};

fn active_storage_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(11, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("deconstruction storage support bounds failed: {error}"),
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
        Err(error) => panic!("deconstruction storage support failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    let activation = match validate_activate_structural_element(registries, state, element) {
        Ok(activation) => activation,
        Err(error) => panic!("deconstruction storage support activation failed: {error}"),
    };
    if let Err(error) = activation.commit(state) {
        panic!("deconstruction storage support activation commit failed: {error}");
    }
    element
}

fn wood_length_for_mass(mass: Mass) -> Length {
    assert!(!mass.is_zero(), "test member mass must be nonzero");
    let numerator = (u128::from(mass.milligrams()) - 1) * 1_000_000;
    let denominator = 1_000_u128 * 650_u128;
    Length::from_micrometers((numerator / denominator + 1) as u64)
}

fn materialized_member(
    registries: &Registries,
    state: &mut AppState,
    mass: Mass,
) -> StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("deconstruction bounds fixture failed: {error}"),
    };
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            wood_length_for_mass(mass),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("deconstruction member fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    element
}

fn explicit_energy(registries: &Registries, state: &AppState) -> Energy {
    match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
        accounting
            .total()
            .ok_or(ExplicitEnergyAccountingError::Overflow)
    }) {
        Ok(total) => total,
        Err(error) => panic!("deconstruction explicit energy accounting failed: {error}"),
    }
}

#[test]
fn direct_removal_cannot_destroy_embodied_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0001));
    let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));

    assert_eq!(
        validate_remove_structural_element(&registries, &state, element),
        Err(StructuralMutationError::ElementOwnsMatter {
            element,
            mass: Mass::from_milligrams(10),
        })
    );
}

#[test]
fn deconstruction_preserves_matter_profile_and_provenance() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0002));
    let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
    let support = active_storage_support(&registries, &mut state);
    let trace = match state.structures().get_element(element) {
        Some(record) => record.embodied_material()[0].clone(),
        None => panic!("deconstruction member disappeared"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(destination) => destination,
        Err(error) => panic!("deconstruction destination failed: {error}"),
    };
    let mount = match validate_mount_stockpile(&registries, &state, destination, support) {
        Ok(mount) => mount,
        Err(error) => panic!("deconstruction destination mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("deconstruction destination mount commit failed: {error}");
    }
    let initial = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("deconstruction initial accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &state);
    let token = match validate_structural_deconstruction(
        &registries,
        &state,
        make_test_deconstruction_resolution(element, destination),
    ) {
        Ok(token) => token,
        Err(error) => panic!("deconstruction validation failed: {error}"),
    };
    let outcome = match token.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("deconstruction commit failed: {error}"),
    };
    assert!(state.structures().get_element(element).is_none());
    assert_eq!(outcome.recovered_lots().len(), 1);
    let lot = match state.inventory().get_lot(outcome.recovered_lots()[0]) {
        Some(lot) => lot,
        None => panic!("recovered material lot disappeared"),
    };
    assert_eq!(lot.mass(), trace.mass());
    assert_eq!(lot.commodity(), trace.profile().commodity());
    assert_eq!(lot.temperature(), trace.profile().temperature());
    assert_eq!(lot.composition(), trace.profile().composition());
    assert_eq!(lot.created_at(), trace.provenance().earliest_created_at());
    assert_eq!(
        lot.latest_created_at(),
        trace.provenance().latest_created_at()
    );
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::from_millinewtons(1))
    );
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(initial)
    );
    assert_eq!(explicit_energy(&registries, &state), initial_energy);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn failed_member_recovers_as_scrap_that_cannot_directly_reset_structural_damage() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0006));
    let mass = Mass::from_milligrams(10);
    let element = materialized_member(&registries, &mut state, mass);
    let activation = validate_activate_structural_element(&registries, &state, element)
        .unwrap_or_else(|error| panic!("damage-recovery activation failed: {error}"));
    activation
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("damage-recovery activation commit failed: {error}"));
    let overload = validate_set_structural_load(
        &registries,
        &state,
        element,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("damage-recovery overload failed: {error}"));
    overload
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("damage-recovery overload commit failed: {error}"));
    let failed = state
        .structures()
        .get_element(element)
        .unwrap_or_else(|| panic!("failed member disappeared before recovery"));
    assert_eq!(failed.lifecycle(), StructuralLifecycle::Failed);
    assert!(failed.is_cracked());

    let destination = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("damage-recovery destination failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("damage-recovery accounting before failed: {error}"))
        .total();
    let recovery = validate_structural_deconstruction(
        &registries,
        &state,
        make_test_deconstruction_resolution(element, destination),
    )
    .unwrap_or_else(|error| panic!("damage-recovery validation failed: {error}"));
    let outcome = recovery
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("damage-recovery commit failed: {error}"));
    assert_eq!(outcome.recovered_lots().len(), 1);
    let recovered = outcome.recovered_lots()[0];
    let lot = state
        .inventory()
        .get_lot(recovered)
        .unwrap_or_else(|| panic!("damage-recovery scrap disappeared"));
    assert_eq!(
        lot.commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP)
    );
    assert_eq!(lot.mass(), mass);
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(matter_before)
    );

    let bounds = VoxelBounds::new(VoxelCoord::new(4, 0, 0), VoxelCoord::new(5, 2, 1))
        .unwrap_or_else(|error| panic!("damage-reset replacement bounds failed: {error}"));
    let replacement = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            wood_length_for_mass(mass),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("damage-reset replacement member failed: {error}"));
    let resolution = bind_structural_construction_selection(
        &state,
        replacement,
        destination,
        &[MaterialLotSelection::new(recovered, mass)],
    )
    .unwrap_or_else(|error| panic!("damage-reset construction binding failed: {error:?}"));
    assert_eq!(
        validate_structural_construction(&registries, &state, resolution),
        Err(
            StructuralConstructionError::DamagedRecoveryFormNotLoadBearing {
                element: replacement,
                form: FORM_SCRAP,
            }
        )
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn deconstruction_restores_multiple_distinct_embodied_traces() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0005));
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("multi-trace deconstruction bounds failed: {error}"),
    };
    let element = match add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            wood_length_for_mass(Mass::from_milligrams(20)),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("multi-trace structural member failed: {error}"),
    };
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(source) => source,
        Err(error) => panic!("multi-trace construction source failed: {error}"),
    };
    let cold = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(8),
        Temperature::from_millikelvin(290_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("cold construction lot failed: {error}"),
    };
    let warm = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(12),
        Temperature::from_millikelvin(310_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("warm construction lot failed: {error}"),
    };
    let initial_matter = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("multi-trace initial matter accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &state);
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[
            MaterialLotSelection::new(cold, Mass::from_milligrams(8)),
            MaterialLotSelection::new(warm, Mass::from_milligrams(12)),
        ],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("multi-trace construction binding failed: {error:?}"),
    };
    let construction = match validate_structural_construction(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("multi-trace construction validation failed: {error}"),
    };
    if let Err(error) = construction.commit(&mut state) {
        panic!("multi-trace construction commit failed: {error}");
    }
    let record = match state.structures().get_element(element) {
        Some(record) => record,
        None => panic!("multi-trace member disappeared after construction"),
    };
    assert_eq!(record.embodied_material().len(), 2);
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(20));

    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(destination) => destination,
        Err(error) => panic!("multi-trace recovery destination failed: {error}"),
    };
    let deconstruction = match validate_structural_deconstruction(
        &registries,
        &state,
        make_test_deconstruction_resolution(element, destination),
    ) {
        Ok(token) => token,
        Err(error) => panic!("multi-trace deconstruction validation failed: {error}"),
    };
    let outcome = match deconstruction.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("multi-trace deconstruction commit failed: {error}"),
    };
    assert_eq!(outcome.recovered_lots().len(), 2);
    let mut recovered = outcome
        .recovered_lots()
        .iter()
        .map(|id| match state.inventory().get_lot(*id) {
            Some(lot) => (lot.mass(), lot.temperature()),
            None => panic!("multi-trace recovered lot disappeared"),
        })
        .collect::<Vec<_>>();
    recovered.sort_by_key(|(_, temperature)| temperature.millikelvin());
    assert_eq!(
        recovered,
        vec![
            (
                Mass::from_milligrams(8),
                Temperature::from_millikelvin(290_000)
            ),
            (
                Mass::from_milligrams(12),
                Temperature::from_millikelvin(310_000)
            ),
        ]
    );
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(initial_matter)
    );
    assert_eq!(explicit_energy(&registries, &state), initial_energy);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn deconstruction_capacity_failure_is_atomic() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0003));
    let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(5)) {
        Ok(destination) => destination,
        Err(error) => panic!("deconstruction capacity destination failed: {error}"),
    };
    let before = state.clone();
    assert!(matches!(
        validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ),
        Err(StructuralDeconstructionError::DestinationCapacityExceeded {
            stockpile: _stockpile,
            capacity: _capacity,
            committed: _committed,
            requested: _requested,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn deconstruction_rechecks_inventory_and_structure_before_any_cross_owner_commit() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5D00_0004));
    let element = materialized_member(&registries, &mut state, Mass::from_milligrams(10));
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(destination) => destination,
        Err(error) => panic!("stale deconstruction destination failed: {error}"),
    };

    let stale_inventory = match validate_structural_deconstruction(
        &registries,
        &state,
        make_test_deconstruction_resolution(element, destination),
    ) {
        Ok(token) => token,
        Err(error) => panic!("stale inventory deconstruction validation failed: {error}"),
    };
    if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        panic!("stale deconstruction inventory mutation failed: {error}");
    }
    let before_inventory_commit = state.clone();
    assert!(matches!(
        stale_inventory.commit(&mut state),
        Err(
            StructuralDeconstructionCommitError::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            }
        )
    ));
    assert_eq!(state, before_inventory_commit);
    assert!(state.structures().get_element(element).is_some());

    let stale_structure = match validate_structural_deconstruction(
        &registries,
        &state,
        make_test_deconstruction_resolution(element, destination),
    ) {
        Ok(token) => token,
        Err(error) => panic!("stale structure deconstruction validation failed: {error}"),
    };
    let bounds = match VoxelBounds::new(VoxelCoord::new(4, 0, 0), VoxelCoord::new(5, 2, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("stale deconstruction bounds failed: {error}"),
    };
    if let Err(error) = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        panic!("stale deconstruction structural mutation failed: {error}");
    }
    let before_structure_commit = state.clone();
    assert!(matches!(
        stale_structure.commit(&mut state),
        Err(StructuralDeconstructionCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            }
        ))
    ));
    assert_eq!(state, before_structure_commit);
    assert!(state.structures().get_element(element).is_some());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
}
