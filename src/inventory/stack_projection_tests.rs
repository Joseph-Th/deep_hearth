//! Contracts for familiar stack grouping over authoritative material lots.

use crate::content::{FORM_FOOD, FORM_LOG, MATERIAL_BERRIES, MATERIAL_WOOD, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::SimulationTick;
use crate::inventory::{
    StockpileStorageProfile, add_stockpile, deposit_lot_for_test, project_material_stacks,
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
}
