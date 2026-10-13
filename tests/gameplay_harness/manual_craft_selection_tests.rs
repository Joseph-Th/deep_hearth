//! Contracts keeping ordinary harness crafting on the production input-planning boundary.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{PROCESS_SHAPE_WOOD_HANDLE, build_registries};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::inventory::StockpileStorageProfile;

use super::manual_craft_selection::{plan_manual_craft_request, select_manual_craft_request};

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
