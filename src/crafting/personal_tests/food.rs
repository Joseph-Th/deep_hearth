//! Food-preservation admission, persisted freshness and conservation contracts.

use crate::content::{
    FORM_FOOD, FORM_WINTER_PROVISIONS, MATERIAL_WILD_NUTS, MATERIAL_WILD_TUBERS,
    PROCESS_CURE_WILD_TUBERS, PROCESS_SORT_WILD_NUTS_FOR_WINTER, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError, apply_clock_advance};
use crate::core::time::{SimulationTick, TickSpan};
use crate::crafting::{
    CraftingJobValidationError, ManualCraftError, ManualCraftRequest, ManualCraftStartRequest,
    resolve_manual_craft, validate_start_manual_craft,
};
use crate::inventory::{MaterialLotSelection, STORAGE_AGE_PARTS_PER_TICK, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{FoodFreshness, assess_food_freshness};

#[test]
fn winter_preparation_rejects_spoiled_forage_without_mutating_inventory() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let lot = state
        .inventory()
        .lot_ids_for_commodity(carried, commodity)
        .next()
        .unwrap();
    let craft = ManualCraftRequest::single(
        PROCESS_SORT_WILD_NUTS_FOR_WINTER,
        carried,
        MaterialLotSelection::new(lot, Mass::from_milligrams(500_000)),
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh { .. })
    ));
    assert!(resolve_manual_craft(&registries, &state, &craft).is_ok());
    apply_clock_advance(
        &mut state,
        SimulationTick::new(11 * registries.core().calendar().ticks_per_day()),
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Spoiled { .. })
    ));
    let before = state.clone();
    assert!(matches!(
        resolve_manual_craft(&registries, &state, &craft),
        Err(ManualCraftError::SpoiledFoodInput { lot: found }) if found == lot
    ));
    assert_eq!(state, before);
}

#[test]
fn loaded_preservation_requires_fresh_input_at_admission_not_current_time() {
    let registries = build_registries();
    for (material, process, expected_provisions_mg) in [
        (
            MATERIAL_WILD_NUTS,
            PROCESS_SORT_WILD_NUTS_FOR_WINTER,
            450_000,
        ),
        (MATERIAL_WILD_TUBERS, PROCESS_CURE_WILD_TUBERS, 400_000),
    ] {
        let mut state = AppState::new();
        let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
            .unwrap()
            .commit(&mut state)
            .unwrap()
            .carried_stockpile();
        let commodity = CommodityKey::new(material, FORM_FOOD);
        let shelf_life = registries
            .survival()
            .get_food(commodity)
            .unwrap()
            .shelf_life()
            .value();
        let mass = Mass::from_milligrams(500_000);
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        let lot = state
            .inventory()
            .lot_ids_for_commodity(carried, commodity)
            .next()
            .unwrap();
        apply_clock_advance(&mut state, SimulationTick::new(shelf_life - 1));
        assert!(
            matches!(assess_food_freshness(&registries, &state, lot), Ok(FoodFreshness::Fresh { remaining, .. }) if remaining.value() == 1)
        );
        let conserved = calculate_matter_accounting(&state).unwrap().total();
        let craft =
            ManualCraftRequest::single(process, carried, MaterialLotSelection::new(lot, mass));
        let job = validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::in_place(craft),
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        // The legitimate input crosses its raw shelf-life boundary while in
        // production. Reload must replay admission, not reject elapsed work.
        let _ = advance_tick(&registries, &mut state).unwrap();
        assert_eq!(state.tick(), SimulationTick::new(shelf_life));
        let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
        let mut restored = serde_json::from_value::<LoadedSaveEnvelope>(encoded.clone())
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(restored, state);
        let mut forged = encoded;
        forged["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["material_storage_history"]
            ["ambient_age_parts"] =
            serde_json::json!(u128::from(shelf_life) * STORAGE_AGE_PARTS_PER_TICK);
        let loaded = serde_json::from_value::<LoadedSaveEnvelope>(forged).unwrap();
        assert_eq!(
            loaded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::CraftingJob(
                CraftingJobValidationError::FoodInputFreshnessInvalid { job, commodity }
            )))
        );
        let remaining = restored
            .production()
            .get_job(job)
            .unwrap()
            .completes_at()
            .checked_duration_since(restored.tick())
            .unwrap();
        for _ in 0..remaining.value() {
            let _ = advance_tick(&registries, &mut state).unwrap();
            let _ = advance_tick(&registries, &mut restored).unwrap();
        }
        assert_eq!(restored, state);
        assert!(restored.production().get_job(job).is_none());
        let provisions = restored
            .inventory()
            .lot_ids_for_commodity(carried, CommodityKey::new(material, FORM_WINTER_PROVISIONS))
            .next()
            .unwrap();
        assert_eq!(
            restored.inventory().get_lot(provisions).unwrap().mass(),
            Mass::from_milligrams(expected_provisions_mg)
        );
        assert!(matches!(
            assess_food_freshness(&registries, &restored, provisions),
            Ok(FoodFreshness::Fresh { age, .. }) if age == TickSpan::new(restored.tick().value())
        ));
        assert_eq!(
            calculate_matter_accounting(&restored).unwrap().total(),
            conserved
        );
    }
}
