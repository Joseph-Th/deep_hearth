//! Contracts that keep gameplay-audit fixture authority outside admitted actor runtime.

use std::panic::{AssertUnwindSafe, catch_unwind};

use deep_hearth::content::gameplay_fixture::{
    authorize_controlled_material_delivery, commit_controlled_material_delivery,
    seed_assembled_equipment_at, seed_lot, seed_preused_assembled_equipment_at, seed_stockpile,
    seed_surface_resource,
};
use deep_hearth::content::{
    EQUIPMENT_STONE_PICK, FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD,
    SURFACE_GATHERING_HAND_SCAVENGE, build_registries,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::labor::PlayerWork;
use deep_hearth::logistics::validate_initialize_player_logistics;
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::surface::{SurfaceGatheringRequest, validate_start_surface_gathering};
use deep_hearth::survival::initialize_player_survival;

use super::tick_observation::{TickEventAllowance, assert_tick_events_within};
use super::{exact_local_runtime, world_admission};

#[test]
fn exact_local_admission_accepts_a_fully_located_runtime_world() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let second = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[first, second],
        &[],
        "exact-local positive contract",
    );
}

#[test]
fn exact_local_admission_rejects_a_forgotten_unlocated_stockpile() {
    let registries = build_registries();
    let mut state = AppState::new();
    let located = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let forgotten = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    exact_local_runtime::locate_stationary_endpoints(&mut state, &[located], &[]);
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("exact-local guard survival setup failed: {error}"));
    exact_local_runtime::initialize_stationary_player_logistics(&mut state);

    let result = catch_unwind(AssertUnwindSafe(|| {
        exact_local_runtime::assert_exact_local_runtime_ready(
            &registries,
            &state,
            "forgotten-endpoint contract",
        );
    }));
    assert!(
        result.is_err(),
        "exact-local admission accepted unlocated stockpile {}",
        forgotten.value()
    );
}

#[test]
fn tick_observation_rejects_unexpected_surface_gathering_and_accepts_the_expected_resource() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = exact_local_runtime::STATIONARY_PLAYER_ORIGIN;
    let gathered_mass = Mass::from_milligrams(1);
    seed_surface_resource(
        &registries,
        &mut state,
        position,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        gathered_mass,
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    );
    let destination = seed_stockpile(
        &mut state,
        gathered_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    world_admission::admit_stationary_player(
        &registries,
        &mut state,
        &[destination],
        &[],
        "tick-observation surface gathering",
    );
    let resource = state
        .available_surface_resources()
        .next()
        .map(|record| record.id())
        .unwrap_or_else(|| panic!("admitted player cannot observe the local surface resource"));
    validate_start_surface_gathering(
        &registries,
        &state,
        SurfaceGatheringRequest::new(
            SURFACE_GATHERING_HAND_SCAVENGE,
            resource,
            destination,
            gathered_mass,
        ),
    )
    .unwrap_or_else(|error| panic!("tick-observation gathering admission failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("tick-observation gathering start failed: {error}"));
    let work = match state.player_work().active() {
        Some(PlayerWork::SurfaceGathering { work }) => work,
        other => panic!("tick-observation gathering did not become active: {other:?}"),
    };
    let remaining_ticks = work
        .completes_at()
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| unreachable!("validated gathering completes after it starts"));
    assert!(remaining_ticks > 0);

    for _ in 1..remaining_ticks {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("tick-observation pre-completion tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance::default(),
            "surface gathering pre-completion",
        );
    }
    let before_completion = state.clone();
    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("tick-observation completion tick failed: {error}"));
    assert!(outcome.surface_gathering().is_some());
    assert_tick_events_within(
        &outcome,
        TickEventAllowance {
            surface_resources: &[resource],
            ..TickEventAllowance::default()
        },
        "expected surface gathering",
    );

    let mut unexpected = before_completion;
    let unexpected_outcome = advance_tick(&registries, &mut unexpected)
        .unwrap_or_else(|error| panic!("tick-observation replay completion failed: {error}"));
    let rejection = catch_unwind(AssertUnwindSafe(|| {
        assert_tick_events_within(
            &unexpected_outcome,
            TickEventAllowance::default(),
            "unexpected surface gathering",
        );
    }));
    assert!(
        rejection.is_err(),
        "shared tick observation must fail closed on an unclaimed surface-gathering completion"
    );
}

fn seed_delivery_endpoints(
    registries: &deep_hearth::registry::Registries,
    state: &mut AppState,
) -> (StockpileId, StockpileId) {
    let profile = StockpileStorageProfile::unbounded_solid_only();
    let source = seed_stockpile(state, Mass::from_milligrams(10), profile);
    let destination = seed_stockpile(state, Mass::from_milligrams(10), profile);
    let _ = seed_lot(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        deep_hearth::core::quantity::Temperature::from_millikelvin(293_150),
    );
    (source, destination)
}

#[test]
fn preused_material_backed_equipment_fixture_rejects_post_admission_condition_seeding() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));
    let condition = Condition::new(900_000)
        .unwrap_or_else(|error| panic!("fixture-boundary condition failed: {error}"));

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        let _ = seed_preused_assembled_equipment_at(
            &registries,
            state,
            EQUIPMENT_STONE_PICK,
            StockpileId::new(91),
            VoxelCoord::new(0, 0, 0),
            condition,
        );
    });
}

#[test]
fn material_backed_equipment_fixture_rejects_post_admission_placement_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        let _ = seed_assembled_equipment_at(
            &registries,
            state,
            EQUIPMENT_STONE_PICK,
            StockpileId::new(91),
            VoxelCoord::new(0, 0, 0),
        );
    });
}

#[test]
fn gameplay_bootstrap_rejects_world_seeding_after_logistics_admission() {
    let mut state = AppState::new();
    validate_initialize_player_logistics(
        &state,
        VoxelCoord::new(0, 0, 0),
        Mass::from_milligrams(1),
    )
    .unwrap_or_else(|error| panic!("fixture-boundary logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fixture-boundary logistics commit failed: {error}"));

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        let _ = seed_stockpile(
            state,
            Mass::from_milligrams(1),
            StockpileStorageProfile::unbounded_solid_only(),
        );
    });
}

fn assert_fixture_rejected_without_mutation(
    state: &mut AppState,
    operation: impl FnOnce(&mut AppState),
) {
    let before = state.clone();
    let result = catch_unwind(AssertUnwindSafe(|| operation(state)));
    assert!(
        result.is_err(),
        "gameplay fixture boundary accepted an operation outside its authorized lifecycle"
    );
    assert_eq!(
        *state, before,
        "rejected gameplay fixture operation must not mutate authoritative state"
    );
}

#[test]
fn gameplay_bootstrap_rejects_world_seeding_after_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        let _ = seed_stockpile(
            state,
            Mass::from_milligrams(1),
            StockpileStorageProfile::unbounded_solid_only(),
        );
    });
}

#[test]
fn controlled_delivery_cannot_be_authorized_after_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    let (source, destination) = seed_delivery_endpoints(&registries, &mut state);
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        let _ = authorize_controlled_material_delivery(
            &registries,
            state,
            source,
            destination,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        );
    });
}

#[test]
fn controlled_delivery_cannot_commit_before_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new();
    let (source, destination) = seed_delivery_endpoints(&registries, &mut state);
    let delivery = authorize_controlled_material_delivery(
        &registries,
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
    );

    assert_fixture_rejected_without_mutation(&mut state, |state| {
        commit_controlled_material_delivery(&registries, state, delivery);
    });
}

#[test]
fn controlled_delivery_authorization_requires_a_real_pre_admission_transfer() {
    let registries = build_registries();
    let state = AppState::new();
    let before = state.clone();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let _ = authorize_controlled_material_delivery(
            &registries,
            &state,
            StockpileId::new(91),
            StockpileId::new(92),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        );
    }));

    assert!(result.is_err());
    assert_eq!(state, before);
}
