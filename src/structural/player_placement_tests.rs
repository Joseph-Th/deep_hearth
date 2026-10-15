//! Contracts for atomic ordinary-player grounded construction from visible carried stacks.

use super::*;

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length, Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{
    MaterialStackProjectionError, add_solid_stockpile_for_test, deposit_lot_for_test,
    project_material_stacks,
};
use crate::labor::PlayerWork;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralConstructionCommitError, StructuralElementGeometry, StructuralLifecycle,
};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const MEMBER_MASS: Mass = Mass::from_milligrams(6_500_000);

fn local_member_plan() -> GroundedStructuralElementPlan {
    let bounds = VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("atomic placement bounds failed: {error}"));
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("atomic placement geometry failed: {error}"));
    GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
    )
}

fn admitted_player_with_carried_member_mass() -> (
    crate::registry::Registries,
    AppState,
    crate::inventory::MaterialStackProjection,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("atomic placement admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("atomic placement admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("atomic placement carried timber failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("atomic placement stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("atomic placement carried timber stack disappeared"));
    (registries, state, stack)
}

#[test]
fn atomic_grounded_construction_materializes_and_starts_timed_work_without_blueprint_step() {
    let (registries, mut state, stack) = admitted_player_with_carried_member_mass();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("atomic placement matter baseline failed: {error}"));

    let validated = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        local_member_plan(),
        &stack,
    )
    .unwrap_or_else(|error| panic!("atomic placement validation failed: {error}"));
    let element = validated.element();
    assert!(state.structures().get_element(element).is_none());

    let work = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("atomic placement commit failed: {error}"));
    assert_eq!(work.element(), element);
    let record = state
        .structures()
        .get_element(element)
        .unwrap_or_else(|| panic!("atomic placement member disappeared after commit"));
    assert_eq!(record.lifecycle(), StructuralLifecycle::Planned);
    assert_eq!(record.embodied_mass(), MEMBER_MASS);
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::StructuralConstruction { work: active }) if active.element() == element
    ));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("atomic placement matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );

    let mut completions = 0_u32;
    while state.tick() < work.completes_at() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("atomic placement construction tick failed: {error}"));
        if outcome.structural_construction() == Some(element) {
            completions += 1;
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert!(state.player_work().active().is_none());
}

#[test]
fn atomic_grounded_construction_rejects_stale_stack_without_orphaning_planned_member() {
    let (registries, mut state, stack) = admitted_player_with_carried_member_mass();
    let validated = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        local_member_plan(),
        &stack,
    )
    .unwrap_or_else(|error| panic!("atomic stale placement validation failed: {error}"));
    let element = validated.element();

    let _unrelated = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| {
            panic!("atomic stale placement inventory mutation failed: {error}")
        });
    let before = state.clone();

    assert!(matches!(
        validated.commit(&mut state),
        Err(
            PlayerGroundedStructuralConstructionCommitError::Construction(
                PlayerStructuralConstructionCommitError::Material(
                    StructuralConstructionCommitError::StaleInventoryRevision { .. }
                )
            )
        )
    ));
    assert_eq!(state, before);
    assert!(state.structures().get_element(element).is_none());
}

#[test]
fn atomic_grounded_construction_rejects_noncarried_stack_before_allocating_member() {
    let registries = build_registries();
    let mut state = AppState::new();
    let remote = add_solid_stockpile_for_test(&mut state, MEMBER_MASS)
        .unwrap_or_else(|error| panic!("atomic source-mismatch stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        remote,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("atomic source-mismatch timber failed: {error}"));
    validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("atomic source-mismatch admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("atomic source-mismatch admission commit failed: {error}"));
    let stack = project_material_stacks(&registries, &state, remote)
        .unwrap_or_else(|error| panic!("atomic source-mismatch projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("atomic source-mismatch stack disappeared"));
    let before = state.clone();

    assert!(matches!(
        validate_start_player_grounded_structural_construction_from_stack(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            local_member_plan(),
            &stack,
        ),
        Err(PlayerGroundedStructuralConstructionError::Stack(
            MaterialStackProjectionError::SourceMismatch { .. }
        ))
    ));
    assert_eq!(state, before);
}
