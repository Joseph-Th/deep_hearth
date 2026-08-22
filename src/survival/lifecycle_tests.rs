//! Tests for the sibling lifecycle module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::build_registries;
use crate::core::time::WorldSeed;

#[test]
fn work_exertion_adds_exactly_to_basal_survival_cost() {
    let registries = build_registries();
    let mut resting = AppState::new(WorldSeed::new(0x5100_0002));
    initialize_player_survival(&registries, &mut resting)
        .unwrap_or_else(|error| panic!("resting survival initialization failed: {error}"));
    let mut working = resting.clone();
    let exertion = SurvivalExertion::new(
        Energy::from_nanojoules(7_000_000_000_000),
        Volume::from_microliters(900),
    );

    let resting_plan = decide_survival_tick(&registries, &resting, SurvivalExertion::REST)
        .unwrap_or_else(|error| panic!("resting survival tick failed: {error:?}"));
    let working_plan = decide_survival_tick(&registries, &working, exertion)
        .unwrap_or_else(|error| panic!("working survival tick failed: {error:?}"));
    let resting_after = apply_survival_tick(&mut resting, resting_plan)
        .unwrap_or_else(|| panic!("resting survival player disappeared"));
    let working_after = apply_survival_tick(&mut working, working_plan)
        .unwrap_or_else(|| panic!("working survival player disappeared"));

    assert_eq!(
        resting_after
            .metabolic_energy()
            .checked_sub(working_after.metabolic_energy()),
        Some(exertion.energy_cost_per_tick())
    );
    assert_eq!(
        resting_after
            .hydration()
            .checked_sub(working_after.hydration()),
        Some(exertion.hydration_loss_per_tick())
    );
}

#[test]
fn exhausting_exact_reserves_does_not_apply_deficit_damage_until_the_next_tick() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0004));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("exact-reserve survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let vitality = Vitality::from_parts_per_million_unchecked(500_000);
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.basal_energy_cost_per_tick(),
            physiology.hydration_loss_per_tick(),
            vitality,
            NutritionReserves::FULL,
        ),
    );

    let exact_plan = decide_survival_tick(&registries, &state, SurvivalExertion::REST)
        .unwrap_or_else(|error| panic!("exact-reserve survival tick failed: {error:?}"));
    let exact_after = apply_survival_tick(&mut state, exact_plan)
        .unwrap_or_else(|| panic!("exact-reserve survival player disappeared"));
    assert_eq!(exact_after.metabolic_energy(), Energy::ZERO);
    assert_eq!(exact_after.hydration(), Volume::ZERO);
    assert_eq!(exact_after.vitality(), vitality);

    let deficit_plan = decide_survival_tick(&registries, &state, SurvivalExertion::REST)
        .unwrap_or_else(|error| panic!("deficit survival tick failed: {error:?}"));
    let deficit_after = apply_survival_tick(&mut state, deficit_plan)
        .unwrap_or_else(|| panic!("deficit survival player disappeared"));
    let expected_loss = physiology
        .starvation_vitality_loss_ppm_per_tick()
        .saturating_add(physiology.dehydration_vitality_loss_ppm_per_tick());
    assert_eq!(
        deficit_after.vitality().parts_per_million(),
        vitality.parts_per_million().saturating_sub(expected_loss)
    );
}

#[test]
fn survival_initialization_and_tick_are_deterministic_and_visible() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    let before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("initialized survival record is missing"));

    let plan = decide_survival_tick(&registries, &state, SurvivalExertion::REST)
        .unwrap_or_else(|error| panic!("survival tick failed: {error:?}"))
        .unwrap_or_else(|| panic!("survival tick did not produce a plan"));
    let after = apply_survival_tick(&mut state, Some(plan))
        .unwrap_or_else(|| panic!("survival tick lost the player"));

    assert!(after.metabolic_energy() < before.metabolic_energy());
    assert!(after.hydration() < before.hydration());
    assert_eq!(after.vitality(), Vitality::MAXIMUM);
    assert!(after.diet_quality_ppm() < before.diet_quality_ppm());
}

#[test]
fn balanced_recent_diet_recovers_vitality_faster_than_one_category() {
    let registries = build_registries();
    let mut balanced = AppState::new(WorldSeed::new(0x5100_0003));
    initialize_player_survival(&registries, &mut balanced)
        .unwrap_or_else(|error| panic!("balanced nutrition initialization failed: {error}"));
    let expected_revision = balanced.survival().revision();
    let next_revision = expected_revision + 1;
    let physiology = registries.survival().physiology();
    balanced.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::from_parts_per_million_unchecked(500_000),
            NutritionReserves::FULL,
        ),
    );
    let mut one_category = balanced.clone();
    let expected_revision = one_category.survival().revision();
    one_category.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::from_parts_per_million_unchecked(500_000),
            NutritionReserves::from_parts_per_million(NUTRITION_PARTS_PER_MILLION, 0, 0),
        ),
    );

    let balanced_plan = decide_survival_tick(&registries, &balanced, SurvivalExertion::REST)
        .unwrap_or_else(|error| panic!("balanced nutrition tick failed: {error:?}"));
    let one_category_plan =
        decide_survival_tick(&registries, &one_category, SurvivalExertion::REST)
            .unwrap_or_else(|error| panic!("single-category nutrition tick failed: {error:?}"));
    let balanced_after = apply_survival_tick(&mut balanced, balanced_plan)
        .unwrap_or_else(|| panic!("balanced nutrition player disappeared"));
    let one_category_after = apply_survival_tick(&mut one_category, one_category_plan)
        .unwrap_or_else(|| panic!("single-category nutrition player disappeared"));

    assert!(balanced_after.vitality() > one_category_after.vitality());
    assert!(balanced_after.diet_quality_ppm() > one_category_after.diet_quality_ppm());
}
