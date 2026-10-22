//! Contracts for atomic ordinary-player grounded construction from visible carried stacks.

use super::*;

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Force, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{
    MaterialStackProjectionError, add_solid_stockpile_for_test, deposit_lot_for_test,
    project_material_stacks,
};
use crate::labor::PlayerWork;
use crate::logistics::relocate_player_for_test;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralConstructionCommitError, StructuralElementGeometry, StructuralLifecycle,
    StructuralLoadKind, add_structural_element, calculate_pristine_member_capacity,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load, validate_start_player_structural_dismantling,
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

fn active_fixture_support(
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
        member_geometry_at(position, context),
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

#[test]
fn atomic_multi_supported_construction_persists_all_supports_and_activates() {
    let registries = build_registries();
    let mut state = AppState::new();
    let left = active_fixture_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 0),
        "multi-support left post",
    );
    let right = active_fixture_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 2),
        "multi-support right post",
    );
    let player = VoxelCoord::new(0, 0, 1);
    let carried = validate_admit_player(&registries, &state, player)
        .unwrap_or_else(|error| panic!("multi-support admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("multi-support admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("multi-support carried timber failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("multi-support stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("multi-support stack disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("multi-support matter baseline failed: {error}"))
        .total();
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        member_geometry_at(VoxelCoord::new(1, 0, 1), "multi-support member"),
        vec![right, left],
    );
    let validated = validate_start_player_multi_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        plan,
        &stack,
    )
    .unwrap_or_else(|error| panic!("multi-support validation failed: {error}"));
    let element = validated.element();
    let work = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("multi-support commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .supports(element)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![left, right])
    );
    assert_eq!(
        state
            .structures()
            .dependents(left)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![element])
    );
    assert_eq!(
        state
            .structures()
            .dependents(right)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![element])
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("multi-support committed matter audit failed: {error}"))
            .total(),
        matter_before
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("multi-support save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("multi-support decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("multi-support trusted load failed: {error}"));
    assert_eq!(
        loaded
            .structures()
            .supports(element)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![left, right])
    );
    while loaded.tick() < work.completes_at() {
        let _ = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("multi-support completion failed: {error}"));
    }
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn multi_supported_construction_can_finish_when_one_of_two_posts_fails() {
    let registries = build_registries();
    let mut state = AppState::new();
    let left = active_fixture_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 0),
        "multi-support partial failure left",
    );
    let right = active_fixture_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 2),
        "multi-support partial failure right",
    );
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 1))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        member_geometry_at(
            VoxelCoord::new(1, 0, 1),
            "partially failed supported member",
        ),
        vec![left, right],
    );
    let authorized = validate_start_player_multi_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        plan,
        &stack,
    )
    .unwrap();
    let element = authorized.element();
    let work = authorized.commit(&mut state).unwrap();
    let record = state.structures().get_element(left).unwrap();
    let profile = registries
        .structural()
        .get_profile(record.profile())
        .unwrap();
    let material = registries
        .materials()
        .get_material(record.material())
        .unwrap();
    let capacity =
        calculate_pristine_member_capacity(profile, material, record.cross_section()).unwrap();
    let _ = validate_set_structural_load(
        &registries,
        &state,
        left,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(capacity.millinewtons() + 1),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state.structures().get_element(left).unwrap().lifecycle(),
        StructuralLifecycle::Failed
    );
    assert_eq!(
        state.structures().get_element(right).unwrap().lifecycle(),
        StructuralLifecycle::Active
    );
    let before = calculate_matter_accounting(&state).unwrap().total();
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut completed = 0;
    while loaded.tick() < work.completes_at() {
        let tick = advance_tick(&registries, &mut loaded).unwrap();
        completed += usize::from(tick.structural_construction() == Some(element));
    }
    assert_eq!(completed, 1);
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .unwrap()
            .lifecycle(),
        StructuralLifecycle::Active
    );
    assert_eq!(
        calculate_matter_accounting(&loaded).unwrap().total(),
        before
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn multi_supported_construction_rejects_empty_and_duplicate_support_sets_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = active_fixture_support(
        &registries,
        &mut state,
        VoxelCoord::new(1, 0, 0),
        "support-set rejection post",
    );
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 1))
        .unwrap_or_else(|error| panic!("support-set rejection admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("support-set rejection admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("support-set rejection timber failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("support-set rejection stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("support-set rejection stack disappeared"));
    let geometry = member_geometry_at(VoxelCoord::new(1, 0, 1), "support-set rejection member");

    for (supports, expected) in [
        (vec![], PlanPlayerStructuralElementError::SupportSetEmpty),
        (
            vec![support, support],
            PlanPlayerStructuralElementError::DuplicateSupport { support },
        ),
    ] {
        let before = state.clone();
        let result = validate_start_player_multi_supported_structural_construction_from_stack(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            MultiSupportedStructuralElementPlan::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                geometry,
                supports,
            ),
            &stack,
        );
        assert!(matches!(
            result,
            Err(PlayerGroundedStructuralConstructionError::Plan(error)) if error == expected
        ));
        assert_eq!(state, before);
    }
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
    relocate_player_for_test(&mut state, FRAMING_POSITION);
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
fn supported_construction_support_failure_interrupts_without_losing_material() {
    let (registries, mut state, support, _carried, stack) = active_support_with_remaining_timber();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("support-failure matter baseline failed: {error}"))
        .total();
    let validated = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        supported_member_plan(support),
        &stack,
    )
    .unwrap_or_else(|error| panic!("support-failure construction validation failed: {error}"));
    let element = validated.element();
    let work = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("support-failure construction commit failed: {error}"));

    let support_record = state
        .structures()
        .get_element(support)
        .unwrap_or_else(|| panic!("support-failure support disappeared before overload"));
    let profile = registries
        .structural()
        .get_profile(support_record.profile())
        .unwrap_or_else(|| panic!("support-failure profile disappeared"));
    let material = registries
        .materials()
        .get_material(support_record.material())
        .unwrap_or_else(|| panic!("support-failure material disappeared"));
    let capacity =
        calculate_pristine_member_capacity(profile, material, support_record.cross_section())
            .unwrap_or_else(|| panic!("support-failure capacity disappeared"));
    let overload = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(
            capacity
                .millinewtons()
                .checked_add(1)
                .unwrap_or_else(|| panic!("support-failure load overflowed")),
        ),
    )
    .unwrap_or_else(|error| panic!("support-failure overload validation failed: {error}"));
    let _ = overload
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("support-failure overload commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Planned, MEMBER_MASS))
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("support-failure save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("support-failure decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("support-failure trusted load failed: {error}"));

    while loaded.tick() < work.completes_at() {
        let outcome = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("support-failure continuation failed: {error}"));
        assert_ne!(outcome.structural_construction(), Some(element));
    }

    assert_eq!(loaded.player_work().active(), None);
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Planned, MEMBER_MASS))
    );
    assert_eq!(
        calculate_matter_accounting(&loaded)
            .unwrap_or_else(|error| panic!("support-failure matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));

    // The interrupted member remains embodied and is recovered through the
    // ordinary player dismantling, labor and reserved inventory ingress path.
    let dismantling = validate_start_player_structural_dismantling(
        &registries,
        &loaded,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("interrupted supported matter stranded: {error}"))
    .commit(&mut loaded)
    .unwrap_or_else(|error| panic!("supported recovery start failed: {error}"));
    assert_eq!(dismantling.recovered_mass(), MEMBER_MASS);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded))
        .unwrap_or_else(|error| panic!("supported recovery save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("supported recovery decode failed: {error}"));
    let mut restored = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("supported recovery trusted load failed: {error}"));
    while restored.tick() < dismantling.completes_at() {
        let _ = advance_tick(&registries, &mut restored)
            .unwrap_or_else(|error| panic!("supported recovery tick failed: {error}"));
    }
    assert_eq!(restored.structures().get_element(element), None);
    assert_eq!(
        restored
            .inventory()
            .get_stockpile(dismantling.recovery_destination())
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(MEMBER_MASS)
    );
    assert_eq!(
        calculate_matter_accounting(&restored)
            .unwrap_or_else(|error| panic!("supported recovered matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &restored), Ok(()));
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
    relocate_player_for_test(&mut state, FRAMING_POSITION);
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
