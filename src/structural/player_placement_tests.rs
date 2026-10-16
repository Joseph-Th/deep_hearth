//! Contracts for atomic ordinary-player grounded construction from visible carried stacks.

use super::*;

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{
    MaterialStackProjectionError, add_solid_stockpile_for_test, deposit_lot_for_test,
    project_material_stacks,
};
use crate::labor::PlayerWork;
use crate::logistics::{add_traversable_edge_for_test, validate_start_player_step};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralConstructionCommitError, StructuralElementGeometry, StructuralLifecycle,
};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const MEMBER_MASS: Mass = Mass::from_milligrams(6_500_000);
const FRAMING_ORIGIN: VoxelCoord = VoxelCoord::new(2, 0, 0);
const FRAMING_POSITION: VoxelCoord = VoxelCoord::new(3, 0, 0);

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

fn member_geometry_at(position: VoxelCoord, context: &str) -> StructuralElementGeometry {
    let maximum = VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1);
    let bounds = VoxelBounds::new(position, maximum)
        .unwrap_or_else(|error| panic!("{context} bounds failed: {error}"));
    StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("{context} geometry failed: {error}"))
}

fn active_support_with_remaining_timber() -> (
    crate::registry::Registries,
    AppState,
    StructuralElementId,
    crate::inventory::StockpileId,
    crate::inventory::MaterialStackProjection,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, FRAMING_ORIGIN)
        .unwrap_or_else(|error| panic!("supported framing admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported framing admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS
            .checked_add(MEMBER_MASS)
            .unwrap_or_else(|| panic!("supported framing timber mass overflowed")),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("supported framing timber setup failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("supported framing initial stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported framing initial timber disappeared"));
    let support_plan = GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        member_geometry_at(VoxelCoord::new(1, 0, 0), "supported framing post"),
    );
    let support_start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        support_plan,
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported framing post validation failed: {error}"));
    let support = support_start.element();
    let support_work = support_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported framing post commit failed: {error}"));
    while state.tick() < support_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported framing post tick failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    add_traversable_edge_for_test(&mut state, FRAMING_ORIGIN, FRAMING_POSITION);
    let step = validate_start_player_step(&registries, &state, FRAMING_POSITION)
        .unwrap_or_else(|error| panic!("supported framing reposition failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported framing reposition commit failed: {error}"));
    while state.tick() < step.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported framing reposition tick failed: {error}"));
    }
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("supported framing remaining stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported framing remaining timber disappeared"));
    assert_eq!(stack.mass(), MEMBER_MASS);
    (registries, state, support, carried, stack)
}

fn supported_member_plan(support: StructuralElementId) -> SupportedStructuralElementPlan {
    SupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        member_geometry_at(VoxelCoord::new(2, 0, 0), "supported framing member"),
        support,
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
fn atomic_supported_construction_persists_support_and_activates_after_hand_setting() {
    let (registries, mut state, support, _carried, stack) = active_support_with_remaining_timber();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("supported framing matter baseline failed: {error}"))
        .total();
    let validated = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        supported_member_plan(support),
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported framing validation failed: {error}"));
    let element = validated.element();
    let work = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported framing commit failed: {error}"));

    let record = state
        .structures()
        .get_element(element)
        .unwrap_or_else(|| panic!("supported framing member disappeared"));
    assert!(!record.is_grounded());
    assert_eq!(record.lifecycle(), StructuralLifecycle::Planned);
    assert_eq!(record.embodied_mass(), MEMBER_MASS);
    assert_eq!(
        state
            .structures()
            .supports(element)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("supported framing matter commit audit failed: {error}"))
            .total(),
        matter_before
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("supported framing save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("supported framing decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("supported framing trusted load failed: {error}"));
    let mut completions = 0_u32;
    while loaded.tick() < work.completes_at() {
        let outcome = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("supported framing completion tick failed: {error}"));
        if outcome.structural_construction() == Some(element) {
            completions += 1;
        }
    }
    assert_eq!(completions, 1);
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        loaded
            .structures()
            .supports(element)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support])
    );
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("supported framing final matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

#[test]
fn atomic_supported_construction_rejects_inactive_support_before_allocating_member() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, FRAMING_ORIGIN)
        .unwrap_or_else(|error| panic!("inactive-support admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inactive-support admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("inactive-support timber setup failed: {error}"));
    let support = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            member_geometry_at(VoxelCoord::new(1, 0, 0), "inactive support"),
        ),
    )
    .unwrap_or_else(|error| panic!("inactive support planning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("inactive support planning commit failed: {error}"));
    add_traversable_edge_for_test(&mut state, FRAMING_ORIGIN, FRAMING_POSITION);
    let step = validate_start_player_step(&registries, &state, FRAMING_POSITION)
        .unwrap_or_else(|error| panic!("inactive-support reposition failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("inactive-support reposition commit failed: {error}"));
    while state.tick() < step.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("inactive-support reposition tick failed: {error}"));
    }
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("inactive-support stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("inactive-support timber disappeared"));
    let before = state.clone();

    assert!(matches!(
        validate_start_player_supported_structural_construction_from_stack(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            supported_member_plan(support),
            &stack,
        ),
        Err(PlayerGroundedStructuralConstructionError::Plan(
            PlanPlayerStructuralElementError::SupportNotActive { support: rejected }
        )) if rejected == support
    ));
    assert_eq!(state, before);
}

#[test]
fn atomic_supported_construction_rejects_stale_stack_without_linking_orphan_member() {
    let (registries, mut state, support, _carried, stack) = active_support_with_remaining_timber();
    let validated = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        supported_member_plan(support),
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported stale validation failed: {error}"));
    let element = validated.element();
    let dependents_before = state
        .structures()
        .dependents(support)
        .map(|dependents| dependents.collect::<Vec<_>>())
        .unwrap_or_default();
    let _ = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("supported stale inventory mutation failed: {error}"));
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
    assert_eq!(
        state
            .structures()
            .dependents(support)
            .map(|dependents| dependents.collect::<Vec<_>>())
            .unwrap_or_default(),
        dependents_before
    );
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
