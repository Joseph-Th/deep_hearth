//! Contracts that keep gameplay-audit fixture authority outside admitted actor runtime.

use deep_hearth::content::gameplay_fixture::{
    authorize_controlled_material_delivery, commit_controlled_material_delivery, seed_stockpile,
};
use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::material::CommodityKey;
use deep_hearth::survival::initialize_player_survival;

fn seed_delivery_endpoints(state: &mut AppState) -> (StockpileId, StockpileId) {
    let profile = StockpileStorageProfile::unbounded_solid_only();
    let source = seed_stockpile(state, Mass::from_milligrams(10), profile);
    let destination = seed_stockpile(state, Mass::from_milligrams(10), profile);
    (source, destination)
}

#[test]
#[should_panic(expected = "gameplay bootstrap stockpile seed must occur before actor admission")]
fn gameplay_bootstrap_rejects_world_seeding_after_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4501));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));

    let _ = seed_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    );
}

#[test]
#[should_panic(
    expected = "gameplay bootstrap controlled-delivery authorization must occur before actor admission"
)]
fn controlled_delivery_cannot_be_authorized_after_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4502));
    let (source, destination) = seed_delivery_endpoints(&mut state);
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fixture-boundary survival setup failed: {error}"));

    let _ = authorize_controlled_material_delivery(
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
    );
}

#[test]
#[should_panic(expected = "gameplay controlled delivery may only commit after actor admission")]
fn controlled_delivery_cannot_commit_before_actor_admission() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x4649_5854_5552_4503));
    let (source, destination) = seed_delivery_endpoints(&mut state);
    let delivery = authorize_controlled_material_delivery(
        &state,
        source,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
    );

    commit_controlled_material_delivery(&registries, &mut state, delivery);
}
