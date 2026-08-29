//! Contract tests for structural construction execution.

use super::*;
use crate::content::{
    FORM_CRUSHED, FORM_INGOT, FORM_LOG, FORM_MOLTEN, MATERIAL_CHARCOAL, MATERIAL_COPPER,
    MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{ExplicitEnergyAccountingError, calculate_explicit_energy_accounting};
use crate::inventory::{
    StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_composed_lot_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
    validate_mount_stockpile,
};
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialLotSpec,
    ParticleSizeDistribution, ParticleSizeRange, ParticleSizeStateError,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};

use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructureValidationError, add_structural_element, validate_activate_structural_element,
};

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
        Err(StructuralConstructionError::UnconsolidatedForm {
            element,
            form: FORM_MOLTEN,
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
        Err(StructuralConstructionError::UnconsolidatedForm {
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

fn unmaterialized_construction_fixture() -> (
    Registries,
    AppState,
    StructuralElementId,
    crate::inventory::StockpileId,
    crate::inventory::MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_E001));
    let mass = Mass::from_milligrams(10);
    let element = member(&registries, &mut state, mass);
    let source = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("construction exhaustion source failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        mass,
        crate::core::quantity::Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("construction exhaustion material failed: {error}"));
    (registries, state, element, source, lot)
}

#[test]
fn construction_rejects_exhausted_inventory_revision_without_consuming_material() {
    let (registries, state, element, source, lot) = unmaterialized_construction_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("construction inventory exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("construction inventory exhaustion decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("construction inventory exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();
    let resolution = bind_structural_construction_selection(
        &loaded,
        element,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("construction inventory exhaustion binding failed: {error:?}"));

    assert_eq!(
        validate_structural_construction(&registries, &loaded, resolution).err(),
        Some(StructuralConstructionError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn construction_rejects_exhausted_structure_revision_without_consuming_material() {
    let (registries, state, element, source, lot) = unmaterialized_construction_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("construction structure exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("construction structure exhaustion decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("construction structure exhaustion fixture should load: {error}")
    });
    let before = loaded.clone();
    let resolution = bind_structural_construction_selection(
        &loaded,
        element,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    )
    .unwrap_or_else(|error| panic!("construction structure exhaustion binding failed: {error:?}"));

    assert_eq!(
        validate_structural_construction(&registries, &loaded, resolution).err(),
        Some(StructuralConstructionError::StructureRevisionExhausted)
    );
    assert_eq!(loaded, before);
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
fn persisted_structure_rejects_forged_embodied_particle_state() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0013));
    let element = member(&registries, &mut state, Mass::from_milligrams(100));
    materialize_structural_element_for_test(&registries, &mut state, element, FORM_LOG);
    let particle_size = ParticleSizeDistribution::from(
        ParticleSizeRange::new(Length::from_micrometers(1), Length::from_micrometers(10))
            .unwrap_or_else(|error| panic!("structural particle tamper range failed: {error}")),
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("structural particle tamper serialization failed: {error}"));
    encoded["state"]["systems"]["structures"]["elements"][element.value().to_string()]["embodied_material"]
        [0]["profile"]["particle_size"] = serde_json::to_value(particle_size)
        .unwrap_or_else(|error| panic!("structural particle tamper encoding failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("structural particle tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::InvalidEmbodiedParticleSizeState {
                element,
                error: ParticleSizeStateError::UnexpectedForUntrackedForm { form: FORM_LOG },
            }
        )))
    );
}

#[test]
fn persisted_structure_rejects_trace_mass_overflow_before_derived_mass_is_used() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5C00_0014));
    let element = member(&registries, &mut state, Mass::from_milligrams(100));
    materialize_structural_element_for_test(&registries, &mut state, element, FORM_LOG);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("structural trace-overflow serialization failed: {error}"));
    let traces = encoded["state"]["systems"]["structures"]["elements"][element.value().to_string()]
        ["embodied_material"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("structural element lost embodied trace array"));
    let mut duplicate = traces
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("structural element lost embodied material"));
    traces[0]["mass"] = serde_json::json!(u64::MAX);
    duplicate["mass"] = serde_json::json!(u64::MAX);
    traces.push(duplicate);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("structural trace-overflow decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::EmbodiedMassOverflow { element }
        )))
    );
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
