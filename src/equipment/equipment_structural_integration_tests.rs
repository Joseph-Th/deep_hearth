//! Contract tests for equipment-owned structural loads.

use super::*;
use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityProfile, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    make_test_registries_with_equipment,
};
use crate::core::quantity::{Area, Mass};
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::equipment::{EquipmentDefinition, add_equipment};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralDamageEvent, StructuralMutationError, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_remove_structural_element, validate_set_structural_load,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(830_001);
const TEST_DEFINITION: EquipmentDefinitionId = EquipmentDefinitionId::new(830_001);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("equipment support condition fixture failed: {error}"),
    }
}

fn make_registries(equipment_mass: Mass) -> Registries {
    let profile = match CapabilityProfile::new([(
        TEST_CAPABILITY,
        CapabilityValue::Mass(Mass::from_milligrams(1)),
    )]) {
        Ok(profile) => profile,
        Err(error) => panic!("equipment support capability fixture failed: {error}"),
    };
    let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("equipment support thresholds fixture failed: {error}"),
    };
    make_test_registries_with_equipment(
        CapabilityDefinition::new(
            TEST_CAPABILITY,
            "equipment support fixture capability",
            CapabilityValueKind::Mass,
        ),
        EquipmentDefinition::new(
            TEST_DEFINITION,
            "equipment support fixture",
            equipment_mass,
            profile,
            thresholds,
        ),
    )
}

fn make_bounds(x: i64) -> VoxelBounds {
    match VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(x + 1, 1, 1)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("equipment support bounds fixture failed: {error}"),
    }
}

fn add_member(registries: &Registries, state: &mut AppState, x: i64) -> StructuralElementId {
    let element = match add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            make_bounds(x),
            crate::core::quantity::Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    ) {
        Ok(element) => element,
        Err(error) => panic!("equipment support member fixture failed: {error}"),
    };
    materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    element
}

fn activate_member(registries: &Registries, state: &mut AppState, element: StructuralElementId) {
    let token = match validate_activate_structural_element(registries, state, element) {
        Ok(token) => token,
        Err(error) => panic!("equipment support activation fixture failed: {error}"),
    };
    if let Err(error) = token.commit(state) {
        panic!("equipment support activation commit failed: {error}");
    }
}

fn add_test_equipment(registries: &Registries, state: &mut AppState) -> EquipmentId {
    match add_equipment(registries, state, TEST_DEFINITION, Condition::PRISTINE) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment support equipment fixture failed: {error}"),
    }
}

fn commit_support(
    token: ValidatedEquipmentSupportChange,
    state: &mut AppState,
) -> EquipmentSupportOutcome {
    match token.commit(state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("equipment support commit failed: {error}"),
    }
}

#[test]
fn multiple_equipment_records_aggregate_one_structural_load_without_rounding_per_record() {
    let registries = make_registries(Mass::from_milligrams(1));
    let mut state = AppState::new(WorldSeed::new(0x8300_0001));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let first = add_test_equipment(&registries, &mut state);
    let second = add_test_equipment(&registries, &mut state);

    let first_mount = match validate_mount_equipment(&registries, &state, first, member) {
        Ok(token) => token,
        Err(error) => panic!("first equipment mount validation failed: {error}"),
    };
    let _ = commit_support(first_mount, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .map(|record| { record.load(StructuralLoadKind::Equipment) }),
        Some(Force::from_millinewtons(1))
    );

    let second_mount = match validate_mount_equipment(&registries, &state, second, member) {
        Ok(token) => token,
        Err(error) => panic!("second equipment mount validation failed: {error}"),
    };
    let _ = commit_support(second_mount, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .map(|record| { record.load(StructuralLoadKind::Equipment) }),
        Some(Force::from_millinewtons(1))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let first_unmount = match validate_unmount_equipment(&registries, &state, first) {
        Ok(token) => token,
        Err(error) => panic!("first equipment unmount validation failed: {error}"),
    };
    let _ = commit_support(first_unmount, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .map(|record| { record.load(StructuralLoadKind::Equipment) }),
        Some(Force::from_millinewtons(1))
    );

    let second_unmount = match validate_unmount_equipment(&registries, &state, second) {
        Ok(token) => token,
        Err(error) => panic!("second equipment unmount validation failed: {error}"),
    };
    let _ = commit_support(second_unmount, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .map(|record| { record.load(StructuralLoadKind::Equipment) }),
        Some(Force::ZERO)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn relocation_remains_revision_bound_when_force_rounding_hides_both_load_deltas() {
    let registries = make_registries(Mass::from_milligrams(1));
    let mut state = AppState::new(WorldSeed::new(0x8300_0012));
    let source = add_member(&registries, &mut state, 0);
    let target = add_member(&registries, &mut state, 2);
    activate_member(&registries, &mut state, source);
    activate_member(&registries, &mut state, target);
    let moved = add_test_equipment(&registries, &mut state);
    let source_peer = add_test_equipment(&registries, &mut state);
    let target_peer = add_test_equipment(&registries, &mut state);
    for (equipment, support) in [
        (moved, source),
        (source_peer, source),
        (target_peer, target),
    ] {
        let mount = validate_mount_equipment(&registries, &state, equipment, support)
            .unwrap_or_else(|error| panic!("rounding relocation mount failed: {error}"));
        let _ = commit_support(mount, &mut state);
    }
    let source_load = state
        .structures()
        .get_element(source)
        .map(|record| record.load(StructuralLoadKind::Equipment))
        .unwrap_or_else(|| panic!("rounding relocation source disappeared"));
    let target_load = state
        .structures()
        .get_element(target)
        .map(|record| record.load(StructuralLoadKind::Equipment))
        .unwrap_or_else(|| panic!("rounding relocation target disappeared"));
    assert_eq!(source_load, Force::from_millinewtons(1));
    assert_eq!(target_load, Force::from_millinewtons(1));
    let structural_revision = state.structures().revision();

    let relocation = validate_relocate_equipment(&registries, &state, moved, target)
        .unwrap_or_else(|error| panic!("rounding relocation validation failed: {error}"));
    let _ = commit_support(relocation, &mut state);

    assert_eq!(state.structures().revision(), structural_revision + 1);
    assert_eq!(
        state
            .structures()
            .get_element(source)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        Some(source_load)
    );
    assert_eq!(
        state
            .structures()
            .get_element(target)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        Some(target_load)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(moved)
            .and_then(|record| record.supported_by()),
        Some(target)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn relocation_moves_equipment_and_structural_load_as_one_transaction() {
    let registries = make_registries(Mass::from_milligrams(3_600_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0010));
    let source = add_member(&registries, &mut state, 0);
    let target = add_member(&registries, &mut state, 2);
    activate_member(&registries, &mut state, source);
    activate_member(&registries, &mut state, target);
    let equipment = add_test_equipment(&registries, &mut state);
    let mount = validate_mount_equipment(&registries, &state, equipment, source)
        .unwrap_or_else(|error| panic!("relocation source mount failed: {error}"));
    let _ = commit_support(mount, &mut state);
    let source_load = state
        .structures()
        .get_element(source)
        .map(|record| record.load(StructuralLoadKind::Equipment))
        .unwrap_or_else(|| panic!("relocation source support disappeared"));

    let relocation = validate_relocate_equipment(&registries, &state, equipment, target)
        .unwrap_or_else(|error| panic!("equipment relocation validation failed: {error}"));
    assert!(
        relocation
            .structural_analysis()
            .assessments()
            .iter()
            .any(|assessment| assessment.element() == target)
    );
    let _ = commit_support(relocation, &mut state);

    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .and_then(|record| record.supported_by()),
        Some(target)
    );
    assert_eq!(
        state
            .structures()
            .get_element(source)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        Some(Force::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(target)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        Some(source_load)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn stale_relocation_leaves_equipment_on_original_support() {
    let registries = make_registries(Mass::from_milligrams(3_600_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0011));
    let source = add_member(&registries, &mut state, 0);
    let target = add_member(&registries, &mut state, 2);
    activate_member(&registries, &mut state, source);
    activate_member(&registries, &mut state, target);
    let equipment = add_test_equipment(&registries, &mut state);
    let mount = validate_mount_equipment(&registries, &state, equipment, source)
        .unwrap_or_else(|error| panic!("stale relocation source mount failed: {error}"));
    let _ = commit_support(mount, &mut state);
    let relocation = validate_relocate_equipment(&registries, &state, equipment, target)
        .unwrap_or_else(|error| panic!("stale relocation validation failed: {error}"));

    let snow = validate_set_structural_load(
        &registries,
        &state,
        target,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(1),
    )
    .unwrap_or_else(|error| panic!("stale relocation structure mutation failed: {error}"));
    let _ = snow
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale relocation structure commit failed: {error}"));

    assert!(matches!(
        relocation.commit(&mut state),
        Err(EquipmentSupportCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            }
        ))
    ));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .and_then(|record| record.supported_by()),
        Some(source)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn heavy_equipment_cracks_support_and_unloading_does_not_repair_damage() {
    let registries = make_registries(Mass::from_milligrams(3_600_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0002));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let equipment = add_test_equipment(&registries, &mut state);

    let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
        Ok(token) => token,
        Err(error) => panic!("heavy equipment mount validation failed: {error}"),
    };
    assert!(matches!(
        mount.structural_analysis().damage_events(),
        [StructuralDamageEvent::Cracked { element, .. }] if *element == member
    ));
    let outcome = commit_support(mount, &mut state);
    assert!(matches!(
        outcome.structural_analysis().damage_events(),
        [StructuralDamageEvent::Cracked { element, .. }] if *element == member
    ));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .and_then(|record| record.supported_by()),
        Some(member)
    );
    let member_record = match state.structures().get_element(member) {
        Some(record) => record,
        None => panic!("heavy equipment support disappeared"),
    };
    assert!(member_record.is_cracked());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    assert_eq!(
        validate_remove_structural_element(&registries, &state, member),
        Err(StructuralMutationError::ElementSupportsEquipment {
            element: member,
            equipment,
        })
    );

    let unmount = match validate_unmount_equipment(&registries, &state, equipment) {
        Ok(token) => token,
        Err(error) => panic!("heavy equipment unmount validation failed: {error}"),
    };
    let outcome = commit_support(unmount, &mut state);
    assert!(outcome.structural_analysis().damage_events().is_empty());
    let member_record = match state.structures().get_element(member) {
        Some(record) => record,
        None => panic!("unloaded cracked support disappeared"),
    };
    assert!(member_record.is_cracked());
    assert_eq!(
        member_record.load(StructuralLoadKind::Equipment),
        Force::ZERO
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn failed_support_can_be_unloaded_without_resurrecting_it() {
    let registries = make_registries(Mass::from_milligrams(4_100_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0003));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let equipment = add_test_equipment(&registries, &mut state);

    let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
        Ok(token) => token,
        Err(error) => panic!("failing equipment mount validation failed: {error}"),
    };
    let _ = commit_support(mount, &mut state);
    assert_eq!(
        state
            .structures()
            .get_element(member)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );

    let unmount = match validate_unmount_equipment(&registries, &state, equipment) {
        Ok(token) => token,
        Err(error) => panic!("failed-support unmount validation failed: {error}"),
    };
    let _ = commit_support(unmount, &mut state);
    let record = match state.structures().get_element(member) {
        Some(record) => record,
        None => panic!("failed support disappeared while unloading"),
    };
    assert_eq!(record.lifecycle(), StructuralLifecycle::Failed);
    assert!(record.is_cracked());
    assert_eq!(record.load(StructuralLoadKind::Equipment), Force::ZERO);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn stale_equipment_revision_rejects_mount_without_structural_mutation() {
    let registries = make_registries(Mass::from_milligrams(1_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0004));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let equipment = add_test_equipment(&registries, &mut state);
    let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
        Ok(token) => token,
        Err(error) => panic!("stale mount validation failed: {error}"),
    };
    let expected_revision = state.equipment().revision();
    add_test_equipment(&registries, &mut state);
    let before = state.clone();

    assert_eq!(
        mount.commit(&mut state),
        Err(EquipmentSupportCommitError::StaleEquipmentRevision {
            expected: expected_revision,
            actual: expected_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn stale_structural_revision_rejects_mount_without_equipment_mutation() {
    let registries = make_registries(Mass::from_milligrams(1_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0006));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let equipment = add_test_equipment(&registries, &mut state);
    let expected_structure_revision = state.structures().revision();
    let mount = match validate_mount_equipment(&registries, &state, equipment, member) {
        Ok(token) => token,
        Err(error) => panic!("stale-structure mount validation failed: {error}"),
    };
    let snow = match validate_set_structural_load(
        &registries,
        &state,
        member,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(1),
    ) {
        Ok(token) => token,
        Err(error) => panic!("intervening structural mutation validation failed: {error}"),
    };
    if let Err(error) = snow.commit(&mut state) {
        panic!("intervening structural mutation commit failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        mount.commit(&mut state),
        Err(EquipmentSupportCommitError::Structure(
            StructuralCommitError::StaleRevision {
                expected: expected_structure_revision,
                actual: expected_structure_revision + 1,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn equipment_load_channel_rejects_direct_structural_writes() {
    let registries = make_registries(Mass::from_milligrams(1_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0007));
    let member = add_member(&registries, &mut state, 0);
    activate_member(&registries, &mut state, member);
    let before = state.clone();

    assert_eq!(
        validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Equipment,
            Force::from_millinewtons(1),
        ),
        Err(StructuralMutationError::LoadOwnedBySubsystem {
            kind: StructuralLoadKind::Equipment,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mounting_requires_an_active_structural_target() {
    let registries = make_registries(Mass::from_milligrams(1_000_000));
    let mut state = AppState::new(WorldSeed::new(0x8300_0005));
    let planned = add_member(&registries, &mut state, 0);
    let equipment = add_test_equipment(&registries, &mut state);
    let before = state.clone();

    assert_eq!(
        validate_mount_equipment(&registries, &state, equipment, planned),
        Err(EquipmentSupportError::TargetNotActive {
            element: planned,
            lifecycle: StructuralLifecycle::Planned,
        })
    );
    assert_eq!(state, before);
}
