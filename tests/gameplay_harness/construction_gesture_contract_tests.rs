//! Ordinary-play contract for atomic grounded construction from actor-visible carried material.

use deep_hearth::content::gameplay_fixture::seed_surface_resource;
use deep_hearth::content::{
    FORM_LOG, MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    STRUCTURAL_PROFILE_AXIAL_COMPRESSION, SURFACE_GATHERING_HAND_COLLECT_TIMBER, build_registries,
};
use deep_hearth::core::quantity::{Area, Length, Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::project_player_inventory;
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::structural::{
    GroundedStructuralElementPlan, StructuralElementGeometry, StructuralLifecycle,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_structural_dismantling,
};
use deep_hearth::surface::validate_start_player_surface_gathering;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);
const MEMBER_MASS: Mass = Mass::from_milligrams(6_500_000);

#[test]
fn ordinary_player_gathers_timber_and_starts_grounded_member_from_visible_stack() {
    let registries = build_registries();
    let mut state = AppState::new();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    seed_surface_resource(
        &registries,
        &mut state,
        ORIGIN,
        timber,
        MEMBER_MASS,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_WOOD),
    );
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("construction-gesture matter baseline failed: {error}"));
    let carried = validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("construction-gesture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction-gesture admission commit failed: {error}"))
        .carried_stockpile();
    let resource = state
        .available_surface_resources()
        .find(|resource| resource.commodity() == timber)
        .map(|resource| resource.id())
        .unwrap_or_else(|| panic!("construction-gesture timber opportunity is not actor-visible"));
    let gathering = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .copied()
        .unwrap_or_else(|| panic!("construction-gesture timber gathering method disappeared"));
    let mut remaining = MEMBER_MASS;
    while !remaining.is_zero() {
        let batch = remaining.min(gathering.maximum_batch_mass());
        validate_start_player_surface_gathering(
            &registries,
            &state,
            SURFACE_GATHERING_HAND_COLLECT_TIMBER,
            resource,
            batch,
        )
        .unwrap_or_else(|error| panic!("construction-gesture gathering failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction-gesture gathering commit failed: {error}"));
        let completes_at = match state.player_work().active() {
            Some(PlayerWork::SurfaceGathering { work }) => work.completes_at(),
            other => panic!("construction-gesture gathering lost player work: {other:?}"),
        };
        while state.tick() < completes_at {
            let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
                panic!("construction-gesture gathering tick failed: {error}")
            });
            assert_tick_events_within(
                &outcome,
                TickEventAllowance {
                    surface_resources: &[resource],
                    ..TickEventAllowance::default()
                },
                "construction gesture timber gathering",
            );
        }
        remaining = remaining
            .checked_sub(batch)
            .unwrap_or_else(|| unreachable!("bounded gathering batch cannot exceed remainder"));
    }
    assert_eq!(state.player_work().active(), None);

    let view = project_player_inventory(&registries, &state).unwrap_or_else(|error| {
        panic!("construction-gesture inventory projection failed: {error}")
    });
    assert_eq!(view.carried().stockpile(), carried);
    let stack = view
        .carried()
        .stacks()
        .iter()
        .find(|stack| stack.commodity() == timber)
        .cloned()
        .unwrap_or_else(|| {
            panic!("construction-gesture gathered timber is not visible as a stack")
        });
    assert_eq!(stack.mass(), MEMBER_MASS);

    let bounds = VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1))
        .unwrap_or_else(|error| panic!("construction-gesture bounds failed: {error}"));
    let geometry = StructuralElementGeometry::new(
        bounds,
        Length::from_micrometers(2_000_000),
        Area::from_square_millimeters(5_000),
    )
    .unwrap_or_else(|error| panic!("construction-gesture geometry failed: {error}"));
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
    .unwrap_or_else(|error| panic!("construction-gesture start failed: {error}"));
    let element = start.element();
    assert!(state.structures().get_element(element).is_none());
    let work = start
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("construction-gesture start commit failed: {error}"));
    assert_eq!(work.element(), element);
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| (record.lifecycle(), record.embodied_mass())),
        Some((StructuralLifecycle::Planned, MEMBER_MASS))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(timber)),
        Some(Mass::ZERO)
    );

    while state.tick() < work.completes_at() {
        let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("construction-gesture hand-setting tick failed: {error}")
        });
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                structural_construction: Some(element),
                ..TickEventAllowance::default()
            },
            "construction gesture hand setting",
        );
    }
    assert_eq!(
        state
            .structures()
            .get_element(element)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Active)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("construction-gesture matter audit failed: {error}"))
            .total(),
        matter_before.total()
    );

    let dismantling = validate_start_player_structural_dismantling(
        &registries,
        &state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap_or_else(|error| panic!("construction-gesture dismantling failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("construction-gesture dismantling commit failed: {error}"));
    assert_eq!(dismantling.element(), element);
    assert_eq!(dismantling.recovered_mass(), MEMBER_MASS);
    assert!(state.structures().get_element(element).is_some());
    while state.tick() < dismantling.completes_at() {
        let outcome = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("construction-gesture dismantling tick failed: {error}")
        });
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                structural_dismantling: Some(element),
                ..TickEventAllowance::default()
            },
            "construction gesture dismantling",
        );
    }
    assert!(state.structures().get_element(element).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(timber)),
        Some(MEMBER_MASS)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "construction-gesture recovery matter audit failed: {error}"
            ))
            .total(),
        matter_before.total()
    );
}
