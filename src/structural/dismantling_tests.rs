//! Contracts for reversible player structural work and exact embodied-matter recovery.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
    validate_mount_stockpile,
};
use crate::labor::PlayerWork;
use crate::logistics::{relocate_player_for_test, validate_place_ground_stockpile};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    GroundedStructuralElementPlan, StructuralElementGeometry, StructuralLifecycle,
    SupportedStructuralElementPlan, validate_plan_player_grounded_structural_element,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_supported_structural_construction_from_stack,
};
use crate::survival::{Vitality, player_record};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const FRAMING_POSITION: VoxelCoord = VoxelCoord::new(0, 0, 1);
const MEMBER_MASS: Mass = Mass::from_milligrams(6_500_000);

fn local_member_plan() -> GroundedStructuralElementPlan {
    let bounds = VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("dismantling bounds failed: {error}"));
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("dismantling geometry failed: {error}"));
    GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
    )
}

#[test]
fn interrupted_planned_member_can_be_recovered_after_respawn() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("interrupted recovery admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("interrupted recovery admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("interrupted recovery timber setup failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("interrupted recovery stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("interrupted recovery timber stack disappeared"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("interrupted recovery matter baseline failed: {error}"))
        .total();
    let construction = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        local_member_plan(),
        &stack,
    )
    .unwrap_or_else(|error| panic!("interrupted recovery construction start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("interrupted recovery construction commit failed: {error}"));
    let element = construction.element();
    assert!(construction.completes_at().value() > state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("interrupted recovery fatal tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Planned, MEMBER_MASS))
    );

    let respawn = validate_respawn_player(&registries, &state)
        .unwrap_or_else(|error| panic!("interrupted recovery respawn failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("interrupted recovery respawn commit failed: {error}"));
    let recovery_destination = respawn.carried_stockpile();
    let dismantling = validate_start_player_structural_dismantling(
        &registries,
        &state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("interrupted planned member was not recoverable: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("interrupted recovery dismantling commit failed: {error}"));
    assert_eq!(dismantling.recovery_destination(), recovery_destination);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("interrupted recovery save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("interrupted recovery decode failed: {error}"));
    state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("interrupted recovery trusted load failed: {error}"));

    while state.tick() < dismantling.completes_at() {
        let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("interrupted recovery dismantling tick failed: {error}")
        });
    }

    assert!(state.structures().get_element(element).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(recovery_destination)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(MEMBER_MASS)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("interrupted recovery matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unmaterialized_planned_member_is_not_recoverable() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("empty planned recovery admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("empty planned recovery admission commit failed: {error}"));
    let element =
        validate_plan_player_grounded_structural_element(&registries, &state, local_member_plan())
            .unwrap_or_else(|error| panic!("empty planned recovery planning failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("empty planned recovery planning commit failed: {error}")
            });
    let before = state.clone();

    assert_eq!(
        validate_start_player_structural_dismantling(
            &registries,
            &state,
            element,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        )
        .err(),
        Some(PlayerStructuralDismantlingError::ElementNotRecoverable { element })
    );
    assert_eq!(state, before);
}

fn geometry_at(position: VoxelCoord, context: &str) -> StructuralElementGeometry {
    let bounds = VoxelBounds::new(
        position,
        VoxelCoord::new(position.x() + 1, position.y() + 1, position.z() + 1),
    )
    .unwrap_or_else(|error| panic!("{context} bounds failed: {error}"));
    StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("{context} geometry failed: {error}"))
}

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("dismantling fatality fixture lost survival player"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            player.hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
}

fn active_supported_leaf() -> (
    crate::registry::Registries,
    AppState,
    StructuralElementId,
    StructuralElementId,
    crate::inventory::StockpileId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("supported dismantling admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported dismantling admission commit failed: {error}"))
        .carried_stockpile();
    let total_mass = MEMBER_MASS
        .checked_add(MEMBER_MASS)
        .unwrap_or_else(|| panic!("supported dismantling fixture mass overflowed"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        total_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("supported dismantling timber setup failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("supported dismantling support stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported dismantling support timber disappeared"));
    let support_start = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry_at(VoxelCoord::new(1, 0, 0), "supported dismantling support"),
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported dismantling support start failed: {error}"));
    let support = support_start.element();
    let support_work = support_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported dismantling support commit failed: {error}"));
    while state.tick() < support_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported dismantling support tick failed: {error}"));
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
        .unwrap_or_else(|error| panic!("supported dismantling leaf stack failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("supported dismantling leaf timber disappeared"));
    let leaf_start = validate_start_player_supported_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            geometry_at(VoxelCoord::new(1, 0, 1), "supported dismantling leaf"),
            support,
        ),
        &stack,
    )
    .unwrap_or_else(|error| panic!("supported dismantling leaf start failed: {error}"));
    let leaf = leaf_start.element();
    let leaf_work = leaf_start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported dismantling leaf commit failed: {error}"));
    while state.tick() < leaf_work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported dismantling leaf tick failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .supports(leaf)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support])
    );
    assert_eq!(
        state
            .structures()
            .dependents(support)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![leaf])
    );
    (registries, state, support, leaf, carried)
}

#[test]
fn structural_dismantling_recovers_supported_leaf_and_cleans_support_edge() {
    let (registries, mut state, support, leaf, carried) = active_supported_leaf();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("supported dismantling matter baseline failed: {error}"))
        .total();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO)
    );

    let start = validate_start_player_structural_dismantling(
        &registries,
        &state,
        leaf,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("supported dismantling validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported dismantling commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("supported dismantling save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("supported dismantling decode failed: {error}"));
    let mut state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("supported dismantling trusted load failed: {error}"));

    let mut completion = None;
    while state.tick() < start.completes_at() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("supported dismantling completion failed: {error}"));
        if outcome.structural_dismantling().is_some() {
            completion = outcome.structural_dismantling().cloned();
        }
    }
    let completion =
        completion.unwrap_or_else(|| panic!("supported dismantling produced no completion"));
    assert_eq!(completion.element(), leaf);
    assert_eq!(completion.recovered_lots().len(), 1);
    assert!(state.structures().get_element(leaf).is_none());
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        state
            .structures()
            .dependents(support)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![])
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(MEMBER_MASS)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "supported dismantling final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn active_grounded_member() -> (
    crate::registry::Registries,
    AppState,
    StructuralElementId,
    crate::inventory::StockpileId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("dismantling player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dismantling player admission commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("dismantling timber setup failed: {error}"));
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("dismantling stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("dismantling timber stack disappeared"));
    let work = validate_start_player_grounded_structural_construction_from_stack(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        local_member_plan(),
        &stack,
    )
    .unwrap_or_else(|error| panic!("dismantling construction start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("dismantling construction commit failed: {error}"));
    let element = work.element();
    while state.tick() < work.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("dismantling construction tick failed: {error}"));
    }
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Active, MEMBER_MASS))
    );
    assert_eq!(state.player_work().active(), None);
    (registries, state, element, carried)
}

#[test]
fn structural_dismantling_reserves_capacity_then_recovers_exact_embodied_matter() {
    let (registries, mut state, element, carried) = active_grounded_member();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("dismantling matter baseline failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG))),
        Some(Mass::ZERO)
    );

    let start = validate_start_player_structural_dismantling(
        &registries,
        &state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("dismantling validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("dismantling commit failed: {error}"));

    assert_eq!(start.element(), element);
    assert_eq!(start.recovered_mass(), MEMBER_MASS);
    assert_eq!(start.recovery_destination(), carried);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.reserved_inbound()),
        Some(MEMBER_MASS)
    );
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::StructuralDismantling { work }) if work.element() == element
    ));
    assert!(state.structures().get_element(element).is_some());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut completion = None;
    while state.tick() < start.completes_at() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("dismantling completion tick failed: {error}"));
        if outcome.structural_dismantling().is_some() {
            completion = outcome.structural_dismantling().cloned();
        }
    }
    let completion = completion.unwrap_or_else(|| panic!("dismantling produced no completion"));
    assert_eq!(completion.element(), element);
    assert_eq!(completion.recovered_lots().len(), 1);
    assert!(state.structures().get_element(element).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.inventory().get_stockpile(carried).map(|record| {
            (
                record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
                record.reserved_inbound(),
            )
        }),
        Some((MEMBER_MASS, Mass::ZERO))
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("dismantling matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn fatal_tick_cancels_structural_dismantling_without_losing_member_matter() {
    let (registries, mut state, element, carried) = active_grounded_member();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fatal dismantling matter baseline failed: {error}"));
    let start = validate_start_player_structural_dismantling(
        &registries,
        &state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("fatal dismantling start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal dismantling start commit failed: {error}"));
    assert!(start.completes_at().value() > state.tick().value() + 1);
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal dismantling tick failed: {error}"));

    assert!(outcome.structural_dismantling().is_none());
    assert!(state.structures().get_element(element).is_some());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|record| record.reserved_inbound()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fatal dismantling matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );
}

#[test]
fn structural_dismantling_rejects_member_that_still_supports_a_stockpile() {
    let (registries, mut state, element, _) = active_grounded_member();
    let support_position = VoxelCoord::new(1, 0, 0);
    relocate_player_for_test(&mut state, support_position);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("dismantling mounted stockpile setup failed: {error}"));
    validate_place_ground_stockpile(&state, stockpile, support_position)
        .unwrap_or_else(|error| panic!("dismantling mounted stockpile placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("dismantling mounted stockpile placement commit failed: {error}")
        });
    let _ = validate_mount_stockpile(&registries, &state, stockpile, element)
        .unwrap_or_else(|error| panic!("dismantling stockpile mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dismantling stockpile mount commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_player_structural_dismantling(
            &registries,
            &state,
            element,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        )
        .err(),
        Some(PlayerStructuralDismantlingError::ElementSupportsStockpile { element })
    );
    assert_eq!(state, before);
}
