//! Direct-consumption revision budgeting and prospective freshness contracts.

use super::*;

#[test]
fn direct_consumption_reserves_attention_release_revision_before_admission() {
    let registries = build_registries();
    let (state, stockpile, food, _) = direct_consumption_fixture(&registries, 0x5A70_0030);
    let state = load_with_owner_revisions(&registries, &state, Some(u64::MAX - 1), None);

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::PlayerWorkRevisionExhausted)
    );
}

#[test]
fn direct_consumption_reserves_survival_revisions_through_completion() {
    let registries = build_registries();
    let (state, stockpile, food, water) = direct_consumption_fixture(&registries, 0x5A70_0031);
    let state = load_with_owner_revisions(&registries, &state, None, Some(u64::MAX - 1));

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::SurvivalRevisionExhausted)
    );
    assert_eq!(
        validate_drink(&registries, &state, water, Volume::from_microliters(1)).err(),
        Some(DrinkError::SurvivalRevisionExhausted)
    );
}

#[test]
fn trusted_load_rejects_active_consumption_without_survival_revision_capacity() {
    let registries = build_registries();
    let (mut state, _, _, water) = direct_consumption_fixture(&registries, 0x5A70_0032);
    let _ = validate_drink(&registries, &state, water, Volume::from_microliters(10))
        .unwrap_or_else(|error| panic!("active-consumption revision validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("active-consumption revision commit failed: {error}"));
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("active-consumption revision serialization failed: {error}")
        });
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("active-consumption revision decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::SurvivalRevisionExhausted
        )))
    );
}

#[test]
fn prospective_storage_freshness_matches_the_canonical_future_enclosure_transition() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0028));
    let food_store = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("freshness projection food stockpile failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        food_store,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("freshness projection berry fixture failed: {error}"));
    let enclosure_mass = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("freshness projection storage definition disappeared"))
        .assembly_profile()
        .input_mass();
    let construction =
        add_solid_stockpile_for_test(&mut state, enclosure_mass).unwrap_or_else(|error| {
            panic!("freshness projection construction stockpile failed: {error}")
        });
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        enclosure_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("freshness projection enclosure body failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(100));
    let transition_at = SimulationTick::new(230);
    let assessment_at = SimulationTick::new(430);
    let forecast = project_food_freshness_after_storage_transition(
        &registries,
        &state,
        berries,
        transition_at,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        assessment_at,
    )
    .unwrap_or_else(|error| panic!("prospective freshness projection failed: {error:?}"));

    apply_clock_advance(&mut state, transition_at);
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        food_store,
        construction,
    )
    .unwrap_or_else(|error| panic!("forecast comparison enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("forecast comparison enclosure commit failed: {error}"));
    apply_clock_advance(&mut state, assessment_at);

    assert_eq!(
        assess_food_freshness(&registries, &state, berries),
        Ok(forecast)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("forecast comparison state audit failed: {error}"));
}

#[test]
fn prospective_storage_freshness_rejects_invalid_horizons_and_unknown_storage() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0029));
    let food_store = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("freshness projection rejection stockpile failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        food_store,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("freshness projection rejection berry failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(10));

    assert_eq!(
        project_food_freshness_after_storage_transition(
            &registries,
            &state,
            berries,
            SimulationTick::new(9),
            STORAGE_TIMBER_PROVISIONS_CHEST,
            SimulationTick::new(20),
        ),
        Err(FoodFreshnessProjectionError::TransitionBeforeCurrent {
            transition_at: SimulationTick::new(9),
            current: SimulationTick::new(10),
        })
    );
    assert_eq!(
        project_food_freshness_after_storage_transition(
            &registries,
            &state,
            berries,
            SimulationTick::new(20),
            STORAGE_TIMBER_PROVISIONS_CHEST,
            SimulationTick::new(19),
        ),
        Err(FoodFreshnessProjectionError::AssessmentBeforeTransition {
            assessment_at: SimulationTick::new(19),
            transition_at: SimulationTick::new(20),
        })
    );
    assert_eq!(
        project_food_freshness_after_storage_transition(
            &registries,
            &state,
            berries,
            SimulationTick::new(20),
            crate::inventory::StorageDefinitionId::new(u32::MAX),
            SimulationTick::new(30),
        ),
        Err(FoodFreshnessProjectionError::UnknownStorageDefinition {
            definition: crate::inventory::StorageDefinitionId::new(u32::MAX),
        })
    );
}
