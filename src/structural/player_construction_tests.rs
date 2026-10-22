//! Contracts for ordinary player-local grounded structural construction.

use super::*;

use crate::content::{
    BLOCK_STATE_STONE, FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Length, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
use crate::logistics::relocate_player_for_test;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    CancelPlayerStructuralPlanError, GroundedStructuralElementPlan,
    PlanPlayerStructuralElementError, StructuralElementGeometry, SupportedStructuralElementPlan,
    add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_cancel_player_structural_plan,
    validate_plan_player_grounded_structural_element,
    validate_plan_player_supported_structural_element,
};
use crate::survival::{Vitality, player_record};
use crate::world::{
    BlockStateId, ChunkCoord, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_chunk,
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

fn elevated_member_geometry() -> StructuralElementGeometry {
    let bounds = VoxelBounds::new(VoxelCoord::new(1, 1, 0), VoxelCoord::new(2, 2, 1))
        .unwrap_or_else(|error| panic!("elevated player construction bounds failed: {error}"));
    StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("elevated player construction geometry failed: {error}"))
}

#[test]
fn persisted_empty_supported_blueprint_can_be_cancelled_without_matter_change() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("blueprint cancellation admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("blueprint cancellation admission commit failed: {error}"));
    let support = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        local_member_geometry(),
        true,
    )
    .unwrap_or_else(|error| panic!("blueprint cancellation support setup failed: {error}"));
    materialize_structural_element_for_test(&registries, &mut state, support, FORM_LOG);
    let _ = validate_activate_structural_element(&registries, &state, support)
        .unwrap_or_else(|error| panic!("blueprint cancellation support activation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("blueprint cancellation support activation commit failed: {error}")
        });
    let blueprint = validate_plan_player_supported_structural_element(
        &registries,
        &state,
        SupportedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            elevated_member_geometry(),
            support,
        ),
    )
    .unwrap_or_else(|error| panic!("supported blueprint planning failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported blueprint planning commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .supports(blueprint)
            .map(|supports| supports.collect::<Vec<_>>()),
        Some(vec![support])
    );
    assert_eq!(
        state
            .structures()
            .dependents(support)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![blueprint])
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("supported blueprint save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("supported blueprint decode failed: {error}"));
    let mut state = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("supported blueprint trusted load failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("blueprint cancellation matter baseline failed: {error}"))
        .total();

    let cancelled = validate_cancel_player_structural_plan(&registries, &state, blueprint)
        .unwrap_or_else(|error| panic!("supported blueprint cancellation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("supported blueprint cancellation commit failed: {error}"));

    assert_eq!(cancelled, blueprint);
    assert!(state.structures().get_element(blueprint).is_none());
    assert_eq!(
        state
            .structures()
            .dependents(support)
            .map(|dependents| dependents.collect::<Vec<_>>()),
        Some(vec![])
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("blueprint cancellation matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn materialized_planned_member_cannot_be_cancelled_as_blueprint() {
    let (registries, mut state, element, lot) = admitted_player_with_planned_member();
    validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        element,
        &[MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )],
    )
    .unwrap_or_else(|error| panic!("materialized cancellation construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("materialized cancellation commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_cancel_player_structural_plan(&registries, &state, element).err(),
        Some(CancelPlayerStructuralPlanError::ElementMaterialized { element })
    );
    assert_eq!(state, before);
}

#[test]
fn dead_admitted_player_cannot_plan_structural_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("dead planning admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dead planning admission commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dead planning serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("dead planning decode failed: {error}"));
    let dead = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("idle dead planning player should load: {error}"));

    assert_eq!(
        validate_plan_player_grounded_structural_element(
            &registries,
            &dead,
            GroundedStructuralElementPlan::new(
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                local_member_geometry(),
            ),
        )
        .err(),
        Some(PlanPlayerStructuralElementError::PlayerDead)
    );
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
fn player_construction_cannot_materialize_member_inside_generated_stone() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x5354_5255_4354_5552),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let player = VoxelCoord::new(0, 0, 0);
    set_generated_block_for_test(registries.world(), &mut state, player, BlockStateId::AIR);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(0, 1, 0),
        BlockStateId::AIR,
    );
    validate_admit_player(&registries, &state, player)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let element = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            local_member_geometry(),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(MEMBER_MASS_MG),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let target = VoxelCoord::new(1, 0, 0);
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_start_player_structural_construction(
            &registries,
            &state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            element,
            &[MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(MEMBER_MASS_MG)
            )],
        )
        .err(),
        Some(PlayerStructuralConstructionError::Material(
            StructuralConstructionError::EmbeddedInSolidTerrain {
                element,
                voxel: target,
            }
        )),
        "construction cannot transfer finite carried timber into a full-cube block"
    );
    assert_eq!(state, before);
    set_generated_block_for_test(registries.world(), &mut state, target, BlockStateId::AIR);
    let plan = validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        element,
        &[MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )],
    )
    .unwrap();
    let expected = state.world().revision();
    set_generated_block_for_test(registries.world(), &mut state, target, BLOCK_STATE_STONE);
    let actual = state.world().revision();
    let before = state.clone();
    assert_eq!(
        plan.commit(&mut state).err(),
        Some(PlayerStructuralConstructionCommitError::Material(
            StructuralConstructionCommitError::StaleWorldRevision { expected, actual }
        ))
    );
    assert_eq!(
        state, before,
        "stale construction must not consume material"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn make_next_tick_fatal(registries: &crate::registry::Registries, state: &mut AppState) {
    let physiology = registries.survival().physiology();
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("fatal construction fixture lost survival player"));
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

#[test]
fn fatal_tick_leaves_unfinished_member_materialized_but_inert() {
    let (registries, mut state, element, lot) = admitted_player_with_planned_member();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fatal construction matter-before failed: {error}"))
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
    .unwrap_or_else(|error| panic!("fatal construction start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal construction start commit failed: {error}"));
    assert!(
        work.completes_at().value() > state.tick().value() + 1,
        "fatal construction proof requires unfinished work after the next tick"
    );
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal construction tick failed: {error}"));

    assert_eq!(outcome.structural_construction(), None);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((
            StructuralLifecycle::Planned,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )),
        "fatal interruption must preserve positioned matter without making it load-bearing"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fatal construction matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fatal construction save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("fatal construction decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("fatal construction trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let post_death = advance_tick(&registries, &mut loaded)
        .unwrap_or_else(|error| panic!("fatal construction post-death tick failed: {error}"));
    assert_eq!(post_death.structural_construction(), None);
    assert_eq!(
        loaded
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Planned)
    );
}

#[test]
fn structural_construction_due_on_fatal_tick_becomes_load_bearing() {
    let (registries, mut state, element, lot) = admitted_player_with_planned_member();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("fatal due construction matter-before failed: {error}"))
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
    .unwrap_or_else(|error| panic!("fatal due construction start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal due construction start commit failed: {error}"));
    while state.tick().value() + 1 < work.completes_at().value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal due construction setup tick failed: {error}"));
        assert_eq!(outcome.structural_construction(), None);
    }
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due construction completion tick failed: {error}"));

    assert_eq!(outcome.structural_construction(), Some(element));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((
            StructuralLifecycle::Active,
            Mass::from_milligrams(MEMBER_MASS_MG),
        )),
        "construction due on the fatal tick must activate its already-positioned member"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("fatal due construction matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
fn ordinary_grounded_member_planning_and_hand_setting_stay_within_interaction_reach() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, player)
        .unwrap_or_else(|error| panic!("construction locality admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction locality admission commit failed: {error}"));
    let reachable_bounds = VoxelBounds::new(VoxelCoord::new(2, 0, 0), VoxelCoord::new(3, 1, 1))
        .unwrap_or_else(|error| panic!("construction reachable bounds failed: {error}"));
    let reachable_geometry = StructuralElementGeometry::new(
        reachable_bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("construction reachable geometry failed: {error}"));
    let reachable = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            reachable_geometry,
        ),
    )
    .unwrap_or_else(|error| panic!("reachable construction planning failed: {error}"));
    assert_eq!(reachable.record().bounds(), reachable_bounds);

    let remote_bounds = VoxelBounds::new(VoxelCoord::new(6, 0, 0), VoxelCoord::new(7, 1, 1))
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
        Some(PlanPlayerStructuralElementError::TargetOutOfReach { player })
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
    let destination = VoxelCoord::new(-1, 0, 0);
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
    relocate_player_for_test(&mut state, destination);
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
