//! Contracts for ordinary player-local grounded structural construction.

use super::*;

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
use crate::logistics::{add_traversable_edge_for_test, validate_start_player_step};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    GroundedStructuralElementPlan, PlanPlayerStructuralElementError, StructuralElementGeometry,
    validate_plan_player_grounded_structural_element,
};

const MEMBER_MASS_MG: u64 = 6_500_000;

fn local_member_geometry() -> StructuralElementGeometry {
    let bounds = VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("player construction bounds failed: {error}"));
    StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("player construction geometry failed: {error}"))
}

fn admitted_player_with_planned_member() -> (
    crate::registry::Registries,
    AppState,
    StructuralElementId,
    crate::inventory::MaterialLotId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("player construction admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player construction admission commit failed: {error}"));
    let element = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            local_member_geometry(),
        ),
    )
    .unwrap_or_else(|error| panic!("player construction planning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("player construction planning commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("player construction carried custody disappeared"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(MEMBER_MASS_MG),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player construction carried timber failed: {error}"));
    (registries, state, element, lot)
}

#[test]
fn carried_timber_becomes_load_bearing_only_after_persisted_hand_setting() {
    let (registries, mut state, element, lot) = admitted_player_with_planned_member();
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("player construction initial matter audit failed: {error}"))
        .total();
    let work = validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        element,
        &[MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )],
    )
    .unwrap_or_else(|error| panic!("player construction start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("player construction start commit failed: {error}"));

    assert_eq!(work.completes_at().value() - work.started_at().value(), 65);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass(),)),
        Some((
            StructuralLifecycle::Planned,
            Mass::from_milligrams(MEMBER_MASS_MG),
        ))
    );
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::StructuralConstruction { work: active }) if active == work
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    for _ in 0..10 {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("player construction pre-save tick failed: {error}"));
        assert_eq!(outcome.structural_construction(), None);
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("player construction save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("player construction decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("player construction trusted load failed: {error}"));

    while loaded.tick().value() + 1 < work.completes_at().value() {
        let outcome = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("player construction resumed tick failed: {error}"));
        assert_eq!(outcome.structural_construction(), None);
    }
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Planned)
    );

    let outcome = advance_tick(&registries, &mut loaded)
        .unwrap_or_else(|error| panic!("player construction completion tick failed: {error}"));
    assert_eq!(outcome.structural_construction(), Some(element));
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass(),)),
        Some((
            StructuralLifecycle::Active,
            Mass::from_milligrams(MEMBER_MASS_MG),
        ))
    );
    assert_eq!(loaded.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!(
                "player construction final matter audit failed: {error}"
            ))
            .total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn ordinary_grounded_member_planning_and_hand_setting_stay_local_and_human_scale() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, player)
        .unwrap_or_else(|error| panic!("construction locality admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction locality admission commit failed: {error}"));
    let remote_bounds = VoxelBounds::new(VoxelCoord::new(2, 0, 0), VoxelCoord::new(3, 1, 1))
        .unwrap_or_else(|error| panic!("construction remote bounds failed: {error}"));
    let remote_geometry = StructuralElementGeometry::new(
        remote_bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("construction remote geometry failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validate_plan_player_grounded_structural_element(
            &registries,
            &state,
            GroundedStructuralElementPlan::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                remote_geometry,
            ),
        )
        .err(),
        Some(PlanPlayerStructuralElementError::TargetNotAdjacent { player })
    );
    assert_eq!(state, before);

    let broad_geometry = StructuralElementGeometry::new(
        VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(3, 1, 1))
            .unwrap_or_else(|error| panic!("construction broad bounds failed: {error}")),
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("construction broad geometry failed: {error}"));
    assert_eq!(
        validate_plan_player_grounded_structural_element(
            &registries,
            &state,
            GroundedStructuralElementPlan::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                broad_geometry,
            ),
        )
        .err(),
        Some(PlanPlayerStructuralElementError::TargetNotSingleVoxel)
    );

    let heavy_geometry = StructuralElementGeometry::new(
        VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1))
            .unwrap_or_else(|error| panic!("construction heavy bounds failed: {error}")),
        Length::from_micrometers(4_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("construction heavy geometry failed: {error}"));
    let heavy = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            heavy_geometry,
        ),
    )
    .unwrap_or_else(|error| panic!("construction heavy planning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("construction heavy planning commit failed: {error}"));
    let before_heavy_start = state.clone();
    assert_eq!(
        validate_start_player_structural_construction(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            heavy,
            &[],
        )
        .err(),
        Some(PlayerStructuralConstructionError::MemberTooHeavy {
            required: Mass::from_milligrams(13_000_000),
            maximum: Mass::from_milligrams(10_000_000),
        })
    );
    assert_eq!(state, before_heavy_start);
}

#[test]
fn validated_construction_start_rejects_player_movement_without_partial_commit() {
    let (registries, mut state, element, lot) = admitted_player_with_planned_member();
    let origin = VoxelCoord::new(0, 0, 0);
    let destination = VoxelCoord::new(-1, 0, 0);
    add_traversable_edge_for_test(&mut state, origin, destination);
    let validated = validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        element,
        &[MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )],
    )
    .unwrap_or_else(|error| panic!("construction race validation failed: {error}"));
    let step = validate_start_player_step(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("construction race movement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction race movement commit failed: {error}"));
    while state.tick() < step.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("construction race movement tick failed: {error}"));
    }
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(destination)
    );
    let expected = state.clone();
    assert!(matches!(
        validated.commit(&mut state),
        Err(PlayerStructuralConstructionCommitError::StaleLogistics { .. })
    ));
    assert_eq!(state, expected);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Planned, Mass::ZERO))
    );
    assert!(state.inventory().get_lot(lot).is_some());
}
