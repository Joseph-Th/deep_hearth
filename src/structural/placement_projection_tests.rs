//! Contracts for player-visible structural placement consequence projection.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{AggregateMass, Area, Force, Length, Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{deposit_lot_for_test, project_material_stacks};
use crate::material::CommodityKey;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementGeometry, StructuralStage, add_structural_element, analyze_structure,
    calculate_pristine_member_capacity, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_set_structural_load,
    validate_start_player_multi_supported_structural_construction_from_stack,
};

const MEMBER_MASS: Mass = Mass::from_milligrams(3_250_000);

fn geometry_at(position: VoxelCoord, context: &str) -> StructuralElementGeometry {
    StructuralElementGeometry::new(
        VoxelBounds::new(
            position,
            VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
        )
        .unwrap_or_else(|error| panic!("{context} bounds failed: {error}")),
        Length::from_micrometers(1_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("{context} geometry failed: {error}"))
}

fn active_support(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    position: VoxelCoord,
    context: &str,
) -> StructuralElementId {
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry_at(position, context),
        true,
    )
    .unwrap_or_else(|error| panic!("{context} allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("{context} activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("{context} activation commit failed: {error}"));
    support
}

fn two_support_player_state() -> (
    crate::registry::Registries,
    AppState,
    StructuralElementId,
    StructuralElementId,
    crate::inventory::StockpileId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let left = active_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 0),
        "placement projection left support",
    );
    let right = active_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 2),
        "placement projection right support",
    );
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 1))
        .unwrap_or_else(|error| panic!("placement projection admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("placement projection admission commit failed: {error}"))
        .carried_stockpile();
    (registries, state, left, right, carried)
}

fn plan(
    left: StructuralElementId,
    right: StructuralElementId,
) -> MultiSupportedStructuralElementPlan {
    MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry_at(VoxelCoord::new(1, 0, 1), "placement projection member"),
        vec![right, left],
    )
}

#[test]
fn projected_multi_support_loads_match_real_completed_construction_without_mutating_state() {
    let (registries, mut state, left, right, carried) = two_support_player_state();
    let before_projection = state.clone();
    let projection =
        project_player_multi_supported_structural_placement(&registries, &state, plan(left, right))
            .unwrap_or_else(|error| panic!("placement consequence projection failed: {error}"));
    assert_eq!(state, before_projection);
    assert_eq!(projection.required_mass(), MEMBER_MASS);
    assert_eq!(
        projection
            .supports()
            .iter()
            .map(|support| support.support())
            .collect::<Vec<_>>(),
        vec![left, right]
    );
    assert!(projection.damage_events().is_empty());
    assert!(projection.member().carried_load() > Force::ZERO);
    for support in projection.supports() {
        assert!(support.after().carried_load() > support.before().carried_load());
    }

    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("placement projection timber deposit failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("placement projection timber stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("placement projection timber disappeared"));
    let work = validate_start_player_multi_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        plan(left, right),
        &stack,
    )
    .unwrap_or_else(|error| panic!("placement projection real construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("placement projection real commit failed: {error}"));
    assert_eq!(work.element(), projection.prospective_element());
    while state.tick() < work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("placement projection completion failed: {error}"));
    }
    let actual = analyze_structure(
        registries.structural(),
        registries.materials(),
        state.structures(),
    )
    .unwrap_or_else(|error| panic!("placement projection final analysis failed: {error}"));
    let actual_member = actual
        .assessments()
        .iter()
        .copied()
        .find(|assessment| assessment.element() == work.element())
        .unwrap_or_else(|| panic!("completed projected member disappeared from analysis"));
    assert_eq!(actual_member, projection.member());
    for projected_support in projection.supports() {
        let actual_support = actual
            .assessments()
            .iter()
            .copied()
            .find(|assessment| assessment.element() == projected_support.support())
            .unwrap_or_else(|| panic!("completed projected support disappeared from analysis"));
        assert_eq!(actual_support, projected_support.after());
    }
}

#[test]
fn projection_surfaces_support_damage_before_material_is_committed() {
    let (registries, mut state, left, right, _carried) = two_support_player_state();
    let profile = registries
        .structural()
        .get_profile(STRUCTURAL_PROFILE_AXIAL_COMPRESSION)
        .unwrap_or_else(|| panic!("placement projection structural profile disappeared"));
    let material = registries
        .materials()
        .get_material(MATERIAL_WOOD)
        .unwrap_or_else(|| panic!("placement projection structural material disappeared"));
    let capacity =
        calculate_pristine_member_capacity(profile, material, Area::from_square_millimeters(5_000))
            .unwrap_or_else(|| panic!("placement projection support capacity disappeared"));
    let cracking_load = capacity
        .millinewtons()
        .checked_mul(u128::from(profile.cracking_at_ppm()))
        .unwrap_or_else(|| panic!("placement projection cracking threshold overflowed"))
        / 1_000_000;
    let support_self_weight = state
        .structures()
        .get_element(left)
        .map(|record| record.load(StructuralLoadKind::SelfWeight).millinewtons())
        .unwrap_or_else(|| panic!("placement projection support disappeared"));
    let headroom = projection_member_half_weight(&registries);
    let permanent = cracking_load
        .checked_sub(support_self_weight)
        .and_then(|load| load.checked_sub(headroom / 2))
        .unwrap_or_else(|| panic!("placement projection pre-crack load underflowed"));
    let token = validate_set_structural_load(
        &registries,
        &state,
        left,
        StructuralLoadKind::Permanent,
        Force::from_millinewtons(permanent),
    )
    .unwrap_or_else(|error| panic!("placement projection pre-load failed: {error}"));
    let _ = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("placement projection pre-load commit failed: {error}"));
    let before = state.clone();

    let projection =
        project_player_multi_supported_structural_placement(&registries, &state, plan(left, right))
            .unwrap_or_else(|error| panic!("placement damage projection failed: {error}"));
    assert_eq!(state, before);
    assert!(projection.supports()[0].before().stage() != StructuralStage::Cracking);
    assert!(
        projection
            .damage_events()
            .iter()
            .any(|event| event.element() == left),
        "placement projection must expose support damage before material commitment"
    );
    assert!(projection.supports()[0].after().stage() >= StructuralStage::Cracking);
}

fn projection_member_half_weight(registries: &crate::registry::Registries) -> u128 {
    let mass = crate::structural::calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        MATERIAL_WOOD,
        Area::from_square_millimeters(5_000),
        Length::from_micrometers(1_000_000),
    )
    .unwrap_or_else(|error| panic!("placement projection member mass failed: {error}"));
    crate::structural::calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(mass),
        registries.core().gravity(),
    )
    .unwrap_or_else(|| panic!("placement projection member weight overflowed"))
    .millinewtons()
        / 2
}
