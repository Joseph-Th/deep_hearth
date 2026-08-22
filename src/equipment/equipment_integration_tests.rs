//! Tests for the sibling equipment integration module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityProfile,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind, evaluate_capabilities,
};
use crate::content::make_test_registries_with_equipment;
use crate::content::{FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION};
use crate::core::quantity::{Area, Force, Mass};
use crate::core::time::WorldSeed;
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    add_equipment, validate_mount_equipment,
};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(820_001);
const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(820_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("condition fixture failed: {error}"),
    }
}

fn add_active_support(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1))
        .unwrap_or_else(|error| panic!("support-aware structural bounds failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("support-aware structural fixture failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("support-aware activation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("support-aware activation commit failed: {error}"));
    support
}

#[test]
fn provider_resolution_keeps_static_capability_and_runtime_condition_separate() {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(75_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("maintenance fixture failed: {error}"),
    };
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "test fixture",
            Mass::from_milligrams(25_000),
            profile,
            thresholds,
        ),
    );
    let mut state = AppState::new(WorldSeed::new(29));
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(500_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("equipment creation failed: {error}"),
        };

    let provider = match resolve_equipment_provider(&registries, &state, equipment) {
        Ok(provider) => provider,
        Err(error) => panic!("provider resolution failed: {error}"),
    };
    assert_eq!(provider.condition(), condition(500_000));
    assert_eq!(provider.mass(), Mass::from_milligrams(25_000));
    assert_eq!(provider.maintenance_band(), MaintenanceBand::Warning);
    assert_eq!(
        provider.get_capability(TEST_CAPABILITY),
        Some(CapabilityValue::Mass(Mass::from_milligrams(75_000)))
    );
}

#[test]
fn provider_resolution_derates_authored_capability_from_runtime_condition() {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(100_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("maintenance fixture failed: {error}"),
    };
    let curve = CapabilityConditionCurve::new(
        TEST_CAPABILITY,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Mass(Mass::from_milligrams(25_000)),
            ),
            CapabilityConditionPoint::new(
                condition(500_000),
                CapabilityValue::Mass(Mass::from_milligrams(50_000)),
            ),
        ],
    );
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "test supported mass",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new_with_capability_condition_curves(
            TEST_DEFINITION,
            "condition-sensitive fixture",
            Mass::from_milligrams(25_000),
            profile,
            thresholds,
            vec![curve],
        ),
    );
    let mut state = AppState::new(WorldSeed::new(31));
    let equipment =
        match add_equipment(&registries, &mut state, TEST_DEFINITION, condition(750_000)) {
            Ok(equipment) => equipment,
            Err(error) => panic!("equipment creation failed: {error}"),
        };

    let provider = match resolve_equipment_provider(&registries, &state, equipment) {
        Ok(provider) => provider,
        Err(error) => panic!("provider resolution failed: {error}"),
    };

    assert_eq!(
        provider.get_capability(TEST_CAPABILITY),
        Some(CapabilityValue::Mass(Mass::from_milligrams(75_000)))
    );
    assert_eq!(
        evaluate_capabilities(
            registries.capabilities(),
            &provider,
            &[CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(75_000)),
            )],
        ),
        Ok(())
    );
    assert!(
        evaluate_capabilities(
            registries.capabilities(),
            &provider,
            &[CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(75_001)),
            )],
        )
        .is_err()
    );
}

#[test]
fn failed_equipment_exposes_no_capabilities() {
    let profile = CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(100_000)),
    )])
    .unwrap_or_else(|error| panic!("failed-equipment capability fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(condition(600_000), condition(250_000))
        .unwrap_or_else(|error| panic!("failed-equipment maintenance fixture failed: {error}"));
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "failed-equipment fixture capability",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "failed-equipment fixture",
            Mass::from_milligrams(25_000),
            profile,
            thresholds,
        ),
    );
    let mut state = AppState::new(WorldSeed::new(0x8200_0004));
    let equipment = add_equipment(&registries, &mut state, TEST_DEFINITION, Condition::FAILED)
        .unwrap_or_else(|error| panic!("failed-equipment fixture failed: {error}"));
    let provider = resolve_equipment_provider(&registries, &state, equipment)
        .unwrap_or_else(|error| panic!("failed-equipment provider resolution failed: {error}"));

    assert_eq!(provider.get_capability(TEST_CAPABILITY), None);
    assert!(matches!(
        evaluate_capabilities(
            registries.capabilities(),
            &provider,
            &[CapabilityRequirement::new(
                TEST_CAPABILITY,
                CapabilityComparison::AtMost,
                CapabilityValue::Mass(Mass::from_milligrams(100_000)),
            )],
        ),
        Err(
            crate::capability::CapabilityEvaluationError::MissingCapability {
                capability: TEST_CAPABILITY,
            }
        )
    ));
}

#[test]
fn fixed_equipment_requires_active_structural_installation_before_use() {
    let profile = CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(100_000)),
    )])
    .unwrap_or_else(|error| panic!("fixed-equipment capability fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(condition(600_000), condition(250_000))
        .unwrap_or_else(|error| panic!("fixed-equipment maintenance fixture failed: {error}"));
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "fixed-equipment fixture capability",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "fixed-equipment fixture",
            Mass::from_milligrams(25_000),
            profile,
            thresholds,
        )
        .with_required_structural_support(),
    );
    let mut state = AppState::new(WorldSeed::new(0x8200_0002));
    let equipment = add_equipment(
        &registries,
        &mut state,
        TEST_DEFINITION,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("fixed-equipment fixture failed: {error}"));

    assert_eq!(
        resolve_equipment_provider(&registries, &state, equipment).map(|_| ()),
        Err(EquipmentProviderError::StructuralSupportRequired { equipment })
    );

    let support = add_active_support(&registries, &mut state, 0);
    validate_mount_equipment(&registries, &state, equipment, support)
        .unwrap_or_else(|error| panic!("fixed-equipment mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fixed-equipment mount commit failed: {error}"));

    let provider = resolve_equipment_provider(&registries, &state, equipment)
        .unwrap_or_else(|error| panic!("mounted fixed equipment remained unusable: {error}"));
    assert_eq!(provider.id(), equipment);
    assert_eq!(provider.condition(), Condition::PRISTINE);
}

#[test]
fn collapsed_structural_support_blocks_new_equipment_use() {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(100_000)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("support-aware capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("support-aware maintenance fixture failed: {error}"),
    };
    let registries = make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "support-aware fixture capability",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "support-aware fixture",
            Mass::from_milligrams(25_000),
            profile,
            thresholds,
        ),
    );
    let mut state = AppState::new(WorldSeed::new(0x8200_0003));
    let support = add_active_support(&registries, &mut state, 0);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("support-aware equipment fixture failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(token) => token,
        Err(error) => panic!("support-aware mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("support-aware mount commit failed: {error}");
    }
    let overload = match validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("support-aware overload validation failed: {error}"),
    };
    if let Err(error) = overload.commit(&mut state) {
        panic!("support-aware overload commit failed: {error}");
    }

    assert!(matches!(
        resolve_equipment_provider(&registries, &state, equipment),
        Err(EquipmentProviderError::StructuralSupportNotActive {
            equipment: rejected_equipment,
            element,
            lifecycle: StructuralLifecycle::Failed,
        }) if rejected_equipment == equipment && element == support
    ));
}
