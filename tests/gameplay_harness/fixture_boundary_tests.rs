//! Contracts that keep gameplay-audit fixture authority outside admitted actor runtime.

use std::panic::{AssertUnwindSafe, catch_unwind};

use deep_hearth::content::gameplay_fixture::{
    authorize_controlled_material_delivery, commit_controlled_material_delivery, seed_lot,
    seed_stockpile,
};
use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::material::CommodityKey;
use deep_hearth::survival::initialize_player_survival;

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
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4501));
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
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4502));
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
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4503));
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
    let state = AppState::new(WorldSeed::new(0x4649_5854_5552_4504));
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
