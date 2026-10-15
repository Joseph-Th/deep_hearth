//! Contracts keeping ordinary harness crafting on the production input-planning boundary.

use std::num::NonZeroU64;

use deep_hearth::content::gameplay_fixture::{seed_ground_stockpile, seed_lot, seed_stockpile};
use deep_hearth::content::{
    FORM_CHIP, FORM_LUMP, FORM_TOOL, MATERIAL_STONE, PROCESS_KNAP_STONE_TOOL,
    PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::crafting::{
    ManualCraftInputMode, player_manual_craft_options, validate_start_player_manual_craft,
};
use deep_hearth::inventory::{StockpileStorageProfile, project_material_stacks};
use deep_hearth::logistics::validate_pickup_stack_from_ground;
use deep_hearth::material::CommodityKey;
use deep_hearth::player::validate_admit_player;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::VoxelCoord;

use super::manual_craft_selection::{plan_manual_craft_request, select_manual_craft_request};
use super::tick_observation::{TickEventAllowance, assert_tick_events_within};

#[test]
fn ordinary_player_crafts_from_recipe_and_batch_without_lot_level_input() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let source = seed_ground_stockpile(&mut state, position, Mass::from_milligrams(1_000_000));
    seed_lot(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    );
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("personal-craft gameplay admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("personal-craft gameplay admission commit failed: {error}"))
        .carried_stockpile();
    let stack = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("personal-craft gameplay source projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("personal-craft gameplay source stack disappeared"));
    validate_pickup_stack_from_ground(
        &registries,
        &state,
        source,
        &stack,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("personal-craft gameplay pickup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("personal-craft gameplay pickup commit failed: {error}"));

    let options = player_manual_craft_options(&registries, &state)
        .unwrap_or_else(|error| panic!("personal-craft gameplay catalog failed: {error}"));
    let knapping = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("personal-craft gameplay knapping recipe disappeared"));
    assert!(matches!(
        knapping.input_mode(),
        ManualCraftInputMode::Automatic(availability) if availability.maximum_batches() == 1
    ));

    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_KNAP_STONE_TOOL,
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one craft batch is nonzero")),
        None,
    )
    .unwrap_or_else(|error| panic!("personal-craft gameplay start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("personal-craft gameplay commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("personal-craft gameplay job disappeared"))
        .completes_at();
    let expected_ticks = completes_at
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("personal-craft gameplay completion precedes start"));
    for elapsed in 1..=expected_ticks {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("personal-craft gameplay tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[job],
                ..TickEventAllowance::default()
            },
            "personal recipe/batch craft",
        );
        let completed = outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == job);
        assert_eq!(
            completed,
            elapsed == expected_ticks,
            "personal recipe/batch craft completion must occur exactly on its admitted schedule"
        );
    }

    let carried = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("personal-craft gameplay carried inventory disappeared"));
    assert_eq!(
        carried.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        carried.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
}

#[test]
fn ordinary_harness_crafting_uses_the_unique_production_planned_cohort() {
    let registries = build_registries();
    let definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
        .unwrap_or_else(|| panic!("wood-handle manual craft disappeared"));
    let batch = definition.input_mass();
    let mut state = AppState::new();
    let source = seed_stockpile(
        &mut state,
        batch,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let lot = seed_lot(
        &registries,
        &mut state,
        source,
        definition.input(),
        batch,
        Temperature::from_millikelvin(293_150),
    );

    let request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_HANDLE,
        source,
        1,
        "manual-craft selection contract",
    );

    assert_eq!(request.process(), PROCESS_SHAPE_WOOD_HANDLE);
    assert_eq!(request.source(), source);
    assert_eq!(request.selections().len(), 1);
    assert_eq!(request.selections()[0].lot(), lot);
    assert_eq!(request.selections()[0].mass(), batch);
}

#[test]
fn ordinary_harness_crafting_combines_durable_temperature_cohorts_deterministically() {
    let registries = build_registries();
    let definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_HANDLE)
        .unwrap_or_else(|| panic!("wood-handle manual craft disappeared"));
    let batch = definition.input_mass();
    let capacity = Mass::from_milligrams(
        batch
            .milligrams()
            .checked_mul(2)
            .unwrap_or_else(|| panic!("manual-craft ambiguity fixture overflowed")),
    );
    let mut state = AppState::new();
    let source = seed_stockpile(
        &mut state,
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let cool = Temperature::from_millikelvin(293_150);
    let warm = Temperature::from_millikelvin(313_150);
    let cool_lot = seed_lot(
        &registries,
        &mut state,
        source,
        definition.input(),
        batch,
        cool,
    );
    let warm_lot = seed_lot(
        &registries,
        &mut state,
        source,
        definition.input(),
        batch,
        warm,
    );

    let request =
        plan_manual_craft_request(&registries, &state, PROCESS_SHAPE_WOOD_HANDLE, source, 2)
            .unwrap_or_else(|error| panic!("durable mixed-temperature planning failed: {error}"));
    assert_eq!(request.selections().len(), 2);
    assert_eq!(request.selections()[0].lot(), cool_lot);
    assert_eq!(request.selections()[0].mass(), batch);
    assert_eq!(request.selections()[1].lot(), warm_lot);
    assert_eq!(request.selections()[1].mass(), batch);
}
