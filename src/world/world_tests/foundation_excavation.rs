//! Removing an actual footing is a structural decision, not free block cleanup.

use super::*;
use crate::content::{
    FORM_LOG, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use crate::core::quantity::{Area, Length};
use crate::inventory::project_material_stacks;
use crate::labor::PlayerWork;
use crate::material::CommodityKey;
use crate::spatial::VoxelBounds;
use crate::structural::{
    GroundedStructuralElementPlan, StructuralElementGeometry, StructuralElementId,
    StructuralLifecycle, add_structural_element, materialize_structural_element_for_test,
    validate_start_player_grounded_structural_construction_from_stack,
};

fn almost_excavated_foundation() -> (Registries, AppState, EquipmentId, VoxelCoord) {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let full = registries
        .world()
        .represented_mass_for_state(BLOCK_STATE_STONE)
        .unwrap();
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        target,
        BLOCK_STATE_STONE,
        full,
        Mass::from_milligrams(2),
    );
    (registries, state, pick, target)
}

fn build_ground_anchor(
    registries: &Registries,
    state: &mut AppState,
    floor: VoxelCoord,
    materialize: bool,
) -> StructuralElementId {
    let base = VoxelCoord::new(floor.x(), floor.y() + 1, floor.z());
    let extent = VoxelCoord::new(base.x() + 1, base.y() + 1, base.z() + 1);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(base, extent).unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap();
    let element = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    if materialize {
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
    }
    element
}

fn dig_request(pick: EquipmentId, mass: u64) -> BlockExcavationRequest {
    BlockExcavationRequest::new(
        positive_x_view(),
        MINING_METHOD_HAND_PICK,
        pick,
        Mass::from_milligrams(mass),
    )
}

#[test]
fn player_built_timber_post_must_be_dismantled_before_its_stone_footing_can_be_mined() {
    let (registries, mut state, pick, floor) = generated_excavation_fixture();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let member_mass = Mass::from_milligrams(3_250_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        timber,
        member_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .find(|stack| stack.commodity() == timber)
        .unwrap();
    let base = VoxelCoord::new(floor.x(), floor.y() + 1, floor.z());
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(
            base,
            VoxelCoord::new(base.x() + 1, base.y() + 1, base.z() + 1),
        )
        .unwrap(),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap();
    let start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry,
        ),
        &stack,
    )
    .expect("real carried timber must permit an ordinary grounded post");
    let post = start.element();
    let before_matter = calculate_matter_accounting(&state).unwrap().total();
    let work = start.commit(&mut state).unwrap();
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::StructuralConstruction { work })
    );
    while state.tick() < work.completes_at() {
        let _ = crate::simulation::advance_tick(&registries, &mut state).unwrap();
    }
    let member = state.structures().get_element(post).unwrap();
    assert_eq!(member.lifecycle(), StructuralLifecycle::Active);
    assert_eq!(member.embodied_mass(), member_mass);
    let source_mass = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    assert_eq!(
        validate_start_player_block_excavation(
            &registries,
            &state,
            dig_request(pick, source_mass.milligrams()),
        )
        .err(),
        Some(BlockExcavationError::GroundedStructureRequiresFloor {
            target: floor,
            element: post,
        })
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        before_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn grounded_member_protects_last_floor_mass_but_not_partial_excavation() {
    let (registries, mut state, pick, target) = almost_excavated_foundation();
    let post = build_ground_anchor(&registries, &mut state, target, true);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 2)).err(),
        Some(BlockExcavationError::GroundedStructureRequiresFloor {
            target,
            element: post,
        }),
    );
    assert_eq!(
        state, before,
        "rejected undermining must be side-effect free"
    );

    let start = validate_start_player_block_excavation(&registries, &state, dig_request(pick, 1))
        .expect("partial excavation leaves the supporting cell collidable")
        .commit(&mut state)
        .unwrap();
    while state.control_tick() < start.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .world()
            .represented_mass_at(registries.world(), target),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 1)).err(),
        Some(BlockExcavationError::GroundedStructureRequiresFloor {
            target,
            element: post,
        }),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state,
    );
}

#[test]
fn opening_resident_footing_invalidates_an_earlier_full_excavation_plan() {
    let (registries, mut state, pick, target) = almost_excavated_foundation();
    let den = VoxelCoord::new(target.x(), target.y() + 1, target.z());
    let junction = VoxelCoord::new(den.x() + 1, den.y(), den.z());
    let tip = VoxelCoord::new(junction.x() + 1, den.y(), den.z());
    for resident in [junction, tip] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(resident.x(), target.y(), resident.z()),
            BLOCK_STATE_STONE,
        );
    }
    for (from, to) in [(den, junction), (junction, tip)] {
        crate::logistics::add_traversable_edge_for_test(&mut state, from, to);
    }
    let pending = validate_start_player_block_excavation(&registries, &state, dig_request(pick, 2))
        .expect("unoccupied block is initially eligible for complete excavation");
    // A resident enters after the authorization but before commit. Kobold
    // positions carry no separate revision, so commit must check the actual
    // floor occupancy, not assume the older world revision proves clearance.
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        pending.commit(&mut state),
        Err(BlockExcavationCommitError::OpeningResidentRequiresFloor { target })
    );
    assert_eq!(
        state, before,
        "stale excavation must leave all custody unchanged"
    );
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 2)).err(),
        Some(BlockExcavationError::OpeningResidentRequiresFloor { target })
    );
    assert!(
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 1)).is_ok(),
        "removing one milligram must not remove the resident's supporting voxel"
    );
}

#[test]
fn grounding_added_after_excavation_validation_invalidates_the_commit() {
    let (registries, mut state, pick, target) = almost_excavated_foundation();
    let validated =
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 2))
            .expect("unoccupied foundation should initially be mineable");
    let post = build_ground_anchor(&registries, &mut state, target, true);
    let before = state.clone();
    // Building the member also consumes an inventory lot; that revision guard
    // rejects the already validated excavation before its footing guard runs.
    // Neither rejection path is permitted to commit the undermining.
    assert!(state.structures().get_element(post).is_some());
    assert!(matches!(
        validated.commit(&mut state),
        Err(BlockExcavationCommitError::StaleInventoryRevision { .. })
            | Err(BlockExcavationCommitError::GroundedStructureRequiresFloor { .. })
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unbuilt_ground_anchor_plan_does_not_claim_physical_terrain_support() {
    let (registries, mut state, pick, target) = almost_excavated_foundation();
    let plan = build_ground_anchor(&registries, &mut state, target, false);
    assert!(
        state
            .structures()
            .get_element(plan)
            .unwrap()
            .embodied_material()
            .is_empty()
    );
    assert!(
        validate_start_player_block_excavation(&registries, &state, dig_request(pick, 2)).is_ok(),
        "paper plans cannot prevent earthworks without an embodied footing"
    );
}
