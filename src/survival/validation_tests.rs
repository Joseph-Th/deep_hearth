//! Contract tests for persisted survival-state validation.

use super::*;
use crate::content::build_registries;
use crate::core::state::{AppState, StateValidationError};
use crate::core::time::WorldSeed;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::survival::initialize_player_survival;

#[test]
fn load_rejects_primary_player_reserves_above_their_authoritative_maxima() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_1004));
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
fn load_rejects_nutrition_reserve_above_normalized_maximum() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_1001));
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_1003));
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_1002));
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
