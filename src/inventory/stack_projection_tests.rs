//! Contracts for familiar stack grouping over authoritative material lots.

use crate::content::{FORM_FOOD, FORM_LOG, MATERIAL_BERRIES, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::SimulationTick;
use crate::inventory::{
    MaterialStackProjectionError, StockpileStorageProfile, add_stockpile, deposit_lot_for_test,
    project_material_stacks,
};
use crate::material::CommodityKey;

#[test]
fn familiar_stack_projection_preserves_physical_and_perishable_cohorts() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_stockpile(
        &mut state,
        Mass::from_milligrams(1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("stack projection stockpile failed: {error}"));
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let ambient = Temperature::from_millikelvin(293_150);
    let warm = Temperature::from_millikelvin(303_150);

    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        berries,
        Mass::from_milligrams(100_000),
        ambient,
    )
    .unwrap_or_else(|error| panic!("older berry stack fixture failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(1));
    let _ = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        berries,
        Mass::from_milligrams(200_000),
        ambient,
    )
    .unwrap_or_else(|error| panic!("newer berry stack fixture failed: {error}"));
    for (temperature, mass) in [
        (ambient, Mass::from_milligrams(300_000)),
        (warm, Mass::from_milligrams(400_000)),
    ] {
        let _ = deposit_lot_for_test(&registries, &mut state, stockpile, wood, mass, temperature)
            .unwrap_or_else(|error| panic!("wood stack fixture failed: {error}"));
    }

    let stacks = project_material_stacks(&registries, &state, stockpile)
        .unwrap_or_else(|error| panic!("stack projection failed: {error}"));
    let berry_stacks = stacks
        .iter()
        .filter(|stack| stack.commodity() == berries)
        .collect::<Vec<_>>();
    assert_eq!(
        berry_stacks.len(),
        2,
        "different food-age cohorts must remain separate"
    );
    assert_eq!(
        berry_stacks
            .iter()
            .map(|stack| stack.mass().milligrams())
            .collect::<Vec<_>>(),
        vec![100_000, 200_000]
    );
    assert_eq!(
        berry_stacks
            .iter()
            .map(|stack| stack.ambient_storage_age_parts())
            .collect::<Vec<_>>(),
        vec![1_000_000, 0],
        "freshness-relevant stack boundaries must expose their current physical storage age"
    );
    let wood_stacks = stacks
        .iter()
        .filter(|stack| stack.commodity() == wood)
        .collect::<Vec<_>>();
    assert_eq!(
        wood_stacks.len(),
        2,
        "different temperatures must remain separate stacks"
    );
    assert_eq!(
        wood_stacks
            .iter()
            .map(|stack| stack.temperature())
            .collect::<Vec<_>>(),
        vec![ambient, warm]
    );
}

#[test]
fn fragmented_stockpile_projection_keeps_cohorts_and_stable_order() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_stockpile(
        &mut state,
        Mass::from_milligrams(1_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("fragmented stockpile creation failed: {error}"));
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for at in 0..64 {
        apply_clock_advance(&mut state, SimulationTick::new(at));
        deposit_lot_for_test(
            &registries,
            &mut state,
            stockpile,
            berries,
            Mass::from_milligrams(1_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("fragmented food creation failed: {error}"));
        if at % 8 == 0 {
            let temperature = if at % 16 == 0 { 293_150 } else { 303_150 };
            deposit_lot_for_test(
                &registries,
                &mut state,
                stockpile,
                wood,
                Mass::from_milligrams(1_000),
                Temperature::from_millikelvin(temperature),
            )
            .unwrap_or_else(|error| panic!("fragmented inert profile creation failed: {error}"));
        }
    }
    let before = state.clone();
    let stacks = project_material_stacks(&registries, &state, stockpile).unwrap();
    assert_eq!(state, before, "read-only projection must preserve custody");
    let food = stacks
        .iter()
        .filter(|stack| stack.commodity() == berries)
        .collect::<Vec<_>>();
    let inert = stacks
        .iter()
        .filter(|stack| stack.commodity() == wood)
        .collect::<Vec<_>>();
    assert_eq!(food.len(), 64);
    assert_eq!(inert.len(), 2);
    assert_eq!(stacks[0].commodity(), berries);
    assert_eq!(stacks[1].commodity(), wood);
    assert_eq!(
        food.iter()
            .map(|stack| stack.mass().milligrams())
            .sum::<u64>(),
        64_000
    );
    assert_eq!(
        inert
            .iter()
            .map(|stack| stack.mass().milligrams())
            .sum::<u64>(),
        8_000
    );
    assert_eq!(
        food.iter()
            .map(|stack| stack.ambient_storage_age_parts())
            .collect::<Vec<_>>(),
        (0..64)
            .rev()
            .map(|elapsed| elapsed as u128 * 1_000_000)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        stacks
            .iter()
            .map(|stack| stack.mass().milligrams())
            .sum::<u64>(),
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .stored_mass()
            .milligrams()
    );
    apply_clock_advance(&mut state, SimulationTick::new(64));
    let renewed = project_material_stacks(&registries, &state, stockpile).unwrap();
    assert_eq!(stacks.len(), renewed.len());
    for (prior, current) in stacks.iter().zip(renewed) {
        assert_eq!(
            prior.refreshed(&registries, &state),
            Ok(current),
            "each independently aged food cohort must retain its own physical identity"
        );
    }
}

#[test]
fn stack_projection_expiration_tracks_time_without_inventory_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = add_stockpile(
        &mut state,
        Mass::from_milligrams(100_000),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(50_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let projected = project_material_stacks(&registries, &state, stockpile).unwrap();
    let inventory_revision = state.inventory().revision();
    let before = state.tick();
    assert_eq!(projected[0].validate_current(&state), Ok(()));

    apply_clock_advance(&mut state, SimulationTick::new(before.value() + 1));
    assert_eq!(state.inventory().revision(), inventory_revision);
    let unchanged = state.clone();
    assert_eq!(
        projected[0].validate_current(&state),
        Err(MaterialStackProjectionError::StaleSimulationTick {
            expected: before,
            actual: state.tick(),
        })
    );
    assert_eq!(
        state, unchanged,
        "rejected retained projections cannot mutate custody"
    );
    let refreshed = project_material_stacks(&registries, &state, stockpile).unwrap();
    assert_eq!(refreshed[0].validate_current(&state), Ok(()));
    assert_eq!(
        projected[0].refreshed(&registries, &state),
        Ok(refreshed[0].clone()),
        "the inventory owner must recover the same physical cohort after aging"
    );
    assert!(
        refreshed[0].ambient_storage_age_parts() > projected[0].ambient_storage_age_parts(),
        "the same lot's time-dependent exposure must be refreshed"
    );
    let _unrelated_stockpile = add_stockpile(
        &mut state,
        Mass::from_milligrams(100_000),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap();
    assert!(matches!(
        projected[0].refreshed(&registries, &state),
        Err(MaterialStackProjectionError::StaleInventoryRevision { expected, actual })
            if expected == inventory_revision && actual != expected
    ));
}
