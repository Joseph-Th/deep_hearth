//! Tests for the sibling construction execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_INGOT, FORM_LOG, FORM_MOLTEN, MATERIAL_CHARCOAL, MATERIAL_COPPER,
    MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::energy::{ExplicitEnergyAccountingError, calculate_explicit_energy_accounting};
use crate::inventory::{
    StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_composed_lot_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
    validate_mount_stockpile,
};
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialLotSpec, ParticleSizeRange,
};
use crate::matter::calculate_matter_accounting;

#[cfg(feature = "test-soak")]
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{add_structural_element, validate_activate_structural_element};

#[cfg(feature = "test-soak")]
use crate::structural::{make_test_deconstruction_resolution, validate_structural_deconstruction};

fn wood_length_for_mass(mass: Mass) -> Length {
    assert!(!mass.is_zero(), "test member mass must be nonzero");
    let numerator = (u128::from(mass.milligrams()) - 1) * 1_000_000;
    let denominator = 1_000_u128 * 650_u128;
    let micrometers = numerator / denominator + 1;
    Length::from_micrometers(micrometers as u64)
}

#[test]
fn liquid_material_cannot_become_structural_embodiment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0012));
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("liquid construction bounds failed: {error}"),
    };
    let element = match add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_COPPER,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("liquid construction member failed: {error}"),
    };
    let requirement = match resolve_structural_material_requirement(&registries, &state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("liquid construction requirement failed: {error}"),
    };
    let vessel_profile = match StockpileStorageProfile::new(
        false,
        true,
        crate::core::quantity::Temperature::from_millikelvin(1_500_000),
    ) {
        Ok(profile) => profile,
        Err(error) => panic!("liquid construction vessel profile failed: {error}"),
    };
    let source = match add_stockpile(&mut state, requirement.required_mass(), vessel_profile) {
        Ok(source) => source,
        Err(error) => panic!("liquid construction source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_MOLTEN),
        requirement.required_mass(),
        crate::core::quantity::Temperature::from_millikelvin(1_357_770),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("liquid construction lot failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[MaterialLotSelection::new(lot, requirement.required_mass())],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("liquid construction binding failed: {error:?}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_structural_construction(&registries, &state, resolution),
        Err(StructuralConstructionError::UnsupportedPhase {
            element,
            form: FORM_MOLTEN,
            phase: MaterialPhase::Liquid,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn particulate_material_requires_consolidation_before_structural_embodiment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0013));
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("particulate construction bounds failed: {error}"),
    };
    let element = match add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_COPPER,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("particulate construction member failed: {error}"),
    };
    let requirement = match resolve_structural_material_requirement(&registries, &state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("particulate construction requirement failed: {error}"),
    };
    let source = match add_solid_stockpile_for_test(&mut state, requirement.required_mass()) {
        Ok(source) => source,
        Err(error) => panic!("particulate construction source failed: {error}"),
    };
    let particle_size = match ParticleSizeRange::new(
        Length::from_micrometers(1),
        Length::from_micrometers(20_000),
    ) {
        Ok(range) => range,
        Err(error) => panic!("particulate construction size range failed: {error}"),
    };
    let specification = match MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        requirement.required_mass(),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
        MaterialComposition::pure(MATERIAL_COPPER),
        particle_size,
    ) {
        Ok(specification) => specification,
        Err(error) => panic!("particulate construction specification failed: {error}"),
    };
    let lot = match deposit_lot_spec_for_test(&registries, &mut state, source, specification) {
        Ok(lot) => lot,
        Err(error) => panic!("particulate construction lot failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[MaterialLotSelection::new(lot, requirement.required_mass())],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("particulate construction binding failed: {error:?}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_structural_construction(&registries, &state, resolution),
        Err(StructuralConstructionError::UnsupportedParticulateForm {
            element,
            form: FORM_CRUSHED,
        })
    );
    assert_eq!(state, before);
}

fn member(
    registries: &Registries,
    state: &mut AppState,
    required_mass: Mass,
) -> StructuralElementId {
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 2, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("construction bounds fixture failed: {error}"),
    };
    match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            wood_length_for_mass(required_mass),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("construction member fixture failed: {error}"),
    }
}

fn explicit_energy(registries: &Registries, state: &AppState) -> Energy {
    match calculate_explicit_energy_accounting(registries, state).and_then(|accounting| {
        accounting
            .total()
            .ok_or(ExplicitEnergyAccountingError::Overflow)
    }) {
        Ok(total) => total,
        Err(error) => panic!("construction explicit energy accounting failed: {error}"),
    }
}

#[test]
fn material_requirement_uses_member_geometry_and_authored_density() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0010));
    let bounds = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("material-requirement bounds failed: {error}"),
    };
    let element = match add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(10_000),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("material-requirement member failed: {error}"),
    };
    let requirement = match resolve_structural_material_requirement(&registries, &state, element) {
        Ok(requirement) => requirement,
        Err(error) => panic!("material requirement failed: {error}"),
    };

    assert_eq!(requirement.element(), element);
    assert_eq!(requirement.material(), MATERIAL_WOOD);
    assert_eq!(
        requirement.solid_volume_ceiling(),
        Volume::from_microliters(10_000)
    );
    assert_eq!(requirement.required_mass(), Mass::from_milligrams(6_500));
}

#[test]
fn construction_rejects_under_and_over_materialization_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0011));
    let element = member(&registries, &mut state, Mass::from_milligrams(10));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(source) => source,
        Err(error) => panic!("quantity-mismatch source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("quantity-mismatch lot failed: {error}"),
    };
    let before = state.clone();

    for selected in [9_u64, 11_u64] {
        let resolution = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(selected),
            )],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("quantity-mismatch binding failed: {error:?}"),
        };
        assert_eq!(
            validate_structural_construction(&registries, &state, resolution),
            Err(StructuralConstructionError::MaterialQuantityMismatch {
                element,
                required: Mass::from_milligrams(10),
                selected: Mass::from_milligrams(selected),
            })
        );
        assert_eq!(state, before);
    }
}

#[test]
fn activation_requires_conserved_construction_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0001));
    let element = member(&registries, &mut state, Mass::from_milligrams(1));
    assert_eq!(
        validate_activate_structural_element(&registries, &state, element),
        Err(super::super::StructuralMutationError::ActivationUnmaterialized { element })
    );
}

#[test]
fn construction_moves_exact_matter_and_derives_self_weight() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0002));
    let element = member(&registries, &mut state, Mass::from_milligrams(2_000_000));
    let support_bounds =
        match VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(11, 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("construction storage support bounds failed: {error}"),
        };
    let support = match add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            support_bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(support) => support,
        Err(error) => panic!("construction storage support failed: {error}"),
    };
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let activation = match validate_activate_structural_element(&registries, &state, support) {
        Ok(activation) => activation,
        Err(error) => panic!("construction storage support activation failed: {error}"),
    };
    if let Err(error) = activation.commit(&mut state) {
        panic!("construction storage support activation commit failed: {error}");
    }
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000)) {
        Ok(source) => source,
        Err(error) => panic!("construction source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(2_000_000),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("construction material failed: {error}"),
    };
    let mount = match validate_mount_stockpile(&registries, &state, source, support) {
        Ok(mount) => mount,
        Err(error) => panic!("construction source mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("construction source mount commit failed: {error}");
    }
    let initial = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("construction initial matter accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &state);
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(2_000_000),
        )],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("construction binding failed: {error:?}"),
    };
    let token = match validate_structural_construction(&registries, &state, resolution) {
        Ok(token) => token,
        Err(error) => panic!("construction validation failed: {error}"),
    };
    let expected_weight = token.self_weight();
    if let Err(error) = token.commit(&mut state) {
        panic!("construction commit failed: {error}");
    }
    let record = match state.structures().get_element(element) {
        Some(record) => record,
        None => panic!("constructed member disappeared"),
    };
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(2_000_000));
    assert_eq!(record.embodied_material().len(), 1);
    assert_eq!(record.load(StructuralLoadKind::SelfWeight), expected_weight);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        Some(Force::ZERO)
    );
    let final_total = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("construction final matter accounting failed: {error}"),
    };
    assert_eq!(final_total, initial);
    assert_eq!(explicit_energy(&registries, &state), initial_energy);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn wrong_material_cannot_become_structural_strength_material() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0003));
    let element = member(&registries, &mut state, Mass::from_milligrams(100));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("wrong-material source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(100),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("wrong-material fixture failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("wrong-material binding failed: {error:?}"),
    };
    let before = state.clone();
    assert_eq!(
        validate_structural_construction(&registries, &state, resolution),
        Err(StructuralConstructionError::MaterialMismatch {
            element,
            expected: MATERIAL_WOOD,
            found: MATERIAL_COPPER,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mixed_composition_cannot_claim_pure_material_structural_strength() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0004));
    let element = member(&registries, &mut state, Mass::from_milligrams(100));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(source) => source,
        Err(error) => panic!("mixed construction source failed: {error}"),
    };
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_WOOD, 900_000),
        CompositionComponent::new(MATERIAL_CHARCOAL, 100_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("mixed construction composition failed: {error}"),
    };
    let lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(100),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
        composition,
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("mixed construction lot failed: {error}"),
    };
    let resolution = match bind_structural_construction_selection(
        &state,
        element,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
    ) {
        Ok(resolution) => resolution,
        Err(error) => panic!("mixed construction binding failed: {error:?}"),
    };
    let before = state.clone();
    assert_eq!(
        validate_structural_construction(&registries, &state, resolution),
        Err(StructuralConstructionError::UnsupportedComposition {
            element,
            material: MATERIAL_WOOD,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn construction_rechecks_both_owner_revisions_before_consuming_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0005));
    let element = member(&registries, &mut state, Mass::from_milligrams(10));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20)) {
        Ok(source) => source,
        Err(error) => panic!("stale construction source failed: {error}"),
    };
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        crate::core::quantity::Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("stale construction material failed: {error}"),
    };
    let selection = [MaterialLotSelection::new(lot, Mass::from_milligrams(10))];

    let inventory_resolution =
        match bind_structural_construction_selection(&state, element, source, &selection) {
            Ok(resolution) => resolution,
            Err(error) => panic!("stale inventory construction binding failed: {error:?}"),
        };
    let stale_inventory =
        match validate_structural_construction(&registries, &state, inventory_resolution) {
            Ok(token) => token,
            Err(error) => panic!("stale inventory construction validation failed: {error}"),
        };
    if let Err(error) = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1)) {
        panic!("stale inventory independent mutation failed: {error}");
    }
    let before_inventory_commit = state.clone();
    assert!(matches!(
        stale_inventory.commit(&mut state),
        Err(StructuralConstructionCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_inventory_commit);

    let structure_resolution =
        match bind_structural_construction_selection(&state, element, source, &selection) {
            Ok(resolution) => resolution,
            Err(error) => panic!("stale structure construction binding failed: {error:?}"),
        };
    let stale_structure =
        match validate_structural_construction(&registries, &state, structure_resolution) {
            Ok(token) => token,
            Err(error) => panic!("stale structure construction validation failed: {error}"),
        };
    member(&registries, &mut state, Mass::from_milligrams(1));
    let before_structure_commit = state.clone();
    assert!(matches!(
        stale_structure.commit(&mut state),
        Err(StructuralConstructionCommitError::StaleStructureRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_structure_commit);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(20))
    );
}

#[cfg(feature = "test-soak")]
fn run_construction_ownership_soak(seed: WorldSeed) -> AppState {
    let registries = build_registries();
    let mut state = AppState::new(seed);
    let mut source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("construction soak source failed: {error}"),
    };
    let mut destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
    {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("construction soak destination failed: {error}"),
    };
    let mut lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        crate::core::quantity::Temperature::from_millikelvin(293_150),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("construction soak initial material failed: {error}"),
    };
    let initial_matter = match calculate_matter_accounting(&state) {
        Ok(accounting) => accounting.total(),
        Err(error) => panic!("construction soak initial matter accounting failed: {error}"),
    };
    let initial_energy = explicit_energy(&registries, &state);

    for step in 0_u64..1_000 {
        let element = member(&registries, &mut state, Mass::from_milligrams(10));
        let construction = match bind_structural_construction_selection(
            &state,
            element,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
        ) {
            Ok(resolution) => resolution,
            Err(error) => panic!("construction soak binding failed at step {step}: {error:?}"),
        };
        let token = match validate_structural_construction(&registries, &state, construction) {
            Ok(token) => token,
            Err(error) => {
                panic!("construction soak validation failed at step {step}: {error}")
            }
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("construction soak commit failed at step {step}: {error}");
        }

        let activation = match validate_activate_structural_element(&registries, &state, element) {
            Ok(token) => token,
            Err(error) => {
                panic!("construction soak activation failed at step {step}: {error}")
            }
        };
        if let Err(error) = activation.commit(&mut state) {
            panic!("construction soak activation commit failed at step {step}: {error}");
        }

        let deconstruction = match validate_structural_deconstruction(
            &registries,
            &state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => {
                panic!("construction soak deconstruction failed at step {step}: {error}")
            }
        };
        let outcome = match deconstruction.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => {
                panic!("construction soak deconstruction commit failed at step {step}: {error}")
            }
        };
        assert_eq!(outcome.recovered_lots().len(), 1);
        lot = outcome.recovered_lots()[0];
        std::mem::swap(&mut source, &mut destination);

        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("construction soak tick failed at step {step}: {error}");
        }
        if step.is_multiple_of(97) {
            if let Err(error) = validate_loaded_state(&registries, &state) {
                panic!("construction soak exhaustive audit failed at step {step}: {error}");
            }
            let matter = match calculate_matter_accounting(&state) {
                Ok(accounting) => accounting.total(),
                Err(error) => {
                    panic!("construction soak matter accounting failed at step {step}: {error}")
                }
            };
            assert_eq!(matter, initial_matter);
            assert_eq!(explicit_energy(&registries, &state), initial_energy);
        }
    }

    assert_eq!(state.structures().elements().count(), 0);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(state.tick().value(), 1_000);
    assert_eq!(
        calculate_matter_accounting(&state).map(|accounting| accounting.total()),
        Ok(initial_matter)
    );
    assert_eq!(explicit_energy(&registries, &state), initial_energy);
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn construction_deconstruction_soak_preserves_conservation_and_replay() {
    let seed = WorldSeed::new(0x5C00_5000);
    let first = run_construction_ownership_soak(seed);
    let second = run_construction_ownership_soak(seed);
    assert_eq!(first, second);
}
