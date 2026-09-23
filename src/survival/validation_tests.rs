//! Contract tests for persisted survival-state validation.

use super::*;
use crate::content::{FLUID_WATER, FORM_FOOD, MATERIAL_GRAIN, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::PlayerWork;
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::survival::{initialize_player_survival, validate_drink, validate_eat};

fn finish_pending_consumption(registries: &Registries, state: &mut AppState) {
    let completes_at = match state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("pending-consumption fixture has no active player work"))
    {
        PlayerWork::Eating { work } => work.completes_at(),
        PlayerWork::Drinking { work } => work.completes_at(),
        other @ (PlayerWork::ManualProduction { .. }
        | PlayerWork::Mining { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }
        | PlayerWork::EquipmentMaintenance { .. }
        | PlayerWork::StorageEnclosureDismantling { .. }) => {
            panic!("pending-consumption fixture has wrong active work: {other:?}")
        }
    };
    while state.tick() < completes_at {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("pending-consumption fixture tick failed: {error}"));
    }
}

#[test]
fn load_rejects_primary_player_reserves_above_their_authoritative_maxima() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("primary-reserve validation setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("primary-reserve serialization failed: {error}"));

    let mut excessive_energy = base.clone();
    excessive_energy["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(physiology.maximum_metabolic_energy().nanojoules() + 1);
    let excessive_energy: LoadedSaveEnvelope = serde_json::from_value(excessive_energy)
        .unwrap_or_else(|error| panic!("excessive-energy decode failed: {error}"));
    assert_eq!(
        excessive_energy.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::EnergyExceedsMaximum
        )))
    );

    let mut excessive_hydration = base.clone();
    excessive_hydration["state"]["systems"]["survival"]["player"]["hydration"] =
        serde_json::json!(physiology.maximum_hydration().microliters() + 1);
    let excessive_hydration: LoadedSaveEnvelope = serde_json::from_value(excessive_hydration)
        .unwrap_or_else(|error| panic!("excessive-hydration decode failed: {error}"));
    assert_eq!(
        excessive_hydration.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::HydrationExceedsMaximum
        )))
    );

    let mut excessive_vitality = base;
    excessive_vitality["state"]["systems"]["survival"]["player"]["vitality"] =
        serde_json::json!(NUTRITION_PARTS_PER_MILLION + 1);
    let excessive_vitality: LoadedSaveEnvelope = serde_json::from_value(excessive_vitality)
        .unwrap_or_else(|error| panic!("excessive-vitality decode failed: {error}"));
    assert_eq!(
        excessive_vitality.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::VitalityExceedsMaximum
        )))
    );
}

#[test]
fn load_rejects_pending_eating_reusing_historical_terminal_accounting() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("pending-eating baseline survival setup failed: {error}"));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("pending-eating baseline stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(2),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-eating baseline food failed: {error}"));

    let _ = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
    )
    .unwrap_or_else(|error| panic!("historical eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("historical eating commit failed: {error}"));
    finish_pending_consumption(&registries, &mut state);

    let _ = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
    )
    .unwrap_or_else(|error| panic!("pending eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pending eating commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-eating baseline serialization failed: {error}"));
    let baseline = &mut encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Eating"]
        ["consumed_before"][0]["total_before"];
    assert_eq!(baseline.as_u64(), Some(1));
    *baseline = serde_json::json!(0_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("pending-eating baseline tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingEatingAccountingMismatch {
                material: MATERIAL_GRAIN,
            }
        )))
    );
}

#[test]
fn load_rejects_pending_drinking_reusing_historical_terminal_accounting() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("pending-drinking baseline survival setup failed: {error}"));
    let drink_volume = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let stored_volume = drink_volume
        .checked_add(drink_volume)
        .unwrap_or_else(|| panic!("pending-drinking baseline volume overflowed"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        stored_volume,
        FLUID_WATER,
        stored_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-drinking baseline water failed: {error}"));

    let _ = validate_drink(&registries, &state, store, drink_volume)
        .unwrap_or_else(|error| panic!("historical drinking validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("historical drinking commit failed: {error}"));
    finish_pending_consumption(&registries, &mut state);

    let _ = validate_drink(&registries, &state, store, drink_volume)
        .unwrap_or_else(|error| panic!("pending drinking validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("pending drinking commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-drinking baseline serialization failed: {error}"));
    let baseline = &mut encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]
        ["consumed_before"];
    assert_eq!(baseline.as_u64(), Some(drink_volume.microliters()));
    *baseline = serde_json::json!(0_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("pending-drinking baseline tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingDrinkingAccountingMismatch { fluid: FLUID_WATER }
        )))
    );
}

#[test]
fn load_rejects_nutrition_reserve_above_normalized_maximum() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("nutrition validation setup failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("nutrition validation serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["nutrition"]["grain"] =
        serde_json::json!(NUTRITION_PARTS_PER_MILLION + 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("nutrition validation decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::NutritionExceedsMaximum {
                category: FoodCategory::Grain,
                value: NUTRITION_PARTS_PER_MILLION + 1,
            }
        )))
    );
}

#[test]
fn load_rejects_fractional_recovery_carry_at_maximum_vitality() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("maximum-vitality recovery validation setup failed: {error}")
    });
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("maximum-vitality recovery serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality_recovery_remainder"] =
        serde_json::json!(1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("maximum-vitality recovery decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::VitalityRecoveryRemainderAtMaximum { value: 1 }
        )))
    );
}

#[test]
fn load_rejects_vitality_recovery_remainder_outside_fractional_scale() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("recovery remainder validation setup failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("recovery remainder serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality_recovery_remainder"] =
        serde_json::json!(NUTRITION_PARTS_PER_MILLION);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("recovery remainder decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::VitalityRecoveryRemainderOutOfRange {
                value: NUTRITION_PARTS_PER_MILLION,
            }
        )))
    );
}
