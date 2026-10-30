//! Contract tests for survival tick evolution.

use super::*;
use crate::content::build_registries;
use crate::core::quantity::{Energy, Volume};
use crate::core::time::SimulationTick;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::survival::{FoodCategory, SurvivalExertion, ThermalExposure, assess_survival};

fn next_tick(state: &AppState) -> SimulationTick {
    SimulationTick::new(
        state
            .tick()
            .value()
            .checked_add(1)
            .unwrap_or_else(|| panic!("survival lifecycle test tick overflowed")),
    )
}
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

#[test]
fn work_exertion_adds_exactly_to_basal_survival_cost() {
    let registries = build_registries();
    let mut resting = AppState::new();
    initialize_player_survival(&registries, &mut resting)
        .unwrap_or_else(|error| panic!("resting survival initialization failed: {error}"));
    let mut working = resting.clone();
    let exertion = SurvivalExertion::new(
        Energy::from_nanojoules(7_000_000_000_000),
        Volume::from_microliters(900),
    );

    let resting_plan = decide_survival_tick(
        &registries,
        &resting,
        SurvivalExertion::REST,
        next_tick(&resting),
    )
    .unwrap_or_else(|error| panic!("resting survival tick failed: {error:?}"));
    let working_plan = decide_survival_tick(&registries, &working, exertion, next_tick(&working))
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
    let mut state = AppState::new();
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
            0,
        ),
    );

    let exact_plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
    .unwrap_or_else(|error| panic!("exact-reserve survival tick failed: {error:?}"));
    let exact_after = apply_survival_tick(&mut state, exact_plan)
        .unwrap_or_else(|| panic!("exact-reserve survival player disappeared"));
    assert_eq!(exact_after.metabolic_energy(), Energy::ZERO);
    assert_eq!(exact_after.hydration(), Volume::ZERO);
    assert_eq!(exact_after.vitality(), vitality);

    let deficit_plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
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
fn cold_player_needs_net_warming_to_stop_cold_injury() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap();
    let physiology = registries.survival().physiology();
    let thermal = physiology.thermal_comfort();
    // A 1.44 kJ local-heat sample offsets the authored open-air loss exactly.
    let balanced_heat = Energy::from_nanojoules(1_440_000_000_000);
    assert_eq!(thermal.local_heat_gain_ppm(balanced_heat), 1_000);
    assert_eq!(
        thermal.exposure_loss_ppm_per_tick(ThermalExposure::Open),
        1_000
    );
    let initial_vitality = Vitality::from_parts_per_million_unchecked(500_000);
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            initial_vitality,
            NutritionReserves::FULL,
            0,
            0,
        ),
    );

    let tick_with_heat = |state: &mut AppState, heat| {
        let plan = decide_survival_tick_with_local_heat(
            &registries,
            state,
            SurvivalExertion::REST,
            next_tick(state),
            heat,
            ThermalExposure::Open,
            0,
        )
        .unwrap();
        apply_survival_tick(state, plan).unwrap()
    };
    let mut unheated = state.clone();
    let mut balanced = state.clone();
    let mut warming = state;
    let cold = tick_with_heat(&mut unheated, Energy::ZERO);
    let sustained_cold = tick_with_heat(&mut balanced, balanced_heat);
    let heated = tick_with_heat(&mut warming, thermal.local_heat_for_maximum_gain());

    assert_eq!(cold.thermal_comfort_ppm(), 0);
    assert_eq!(sustained_cold.thermal_comfort_ppm(), 0);
    assert_eq!(
        sustained_cold.vitality().parts_per_million(),
        initial_vitality.parts_per_million() - thermal.cold_vitality_loss_ppm_per_tick(),
        "thermal balance at zero comfort is still sustained cold exposure"
    );
    assert_eq!(sustained_cold.vitality(), cold.vitality());
    assert!(heated.thermal_comfort_ppm() > 0);
    assert!(heated.vitality() > sustained_cold.vitality());

    // The tick that first spends exactly the last warmth reserve still has
    // the same exact-cover grace as energy and hydration. Continued cold on
    // the following tick must incur injury, even with balanced local heat.
    let mut just_warm = warming;
    let revision = just_warm.survival().revision();
    let record = *just_warm.survival().player().unwrap();
    just_warm.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            record.metabolic_energy(),
            record.hydration(),
            initial_vitality,
            record.nutrition(),
            record.vitality_recovery_remainder(),
            1_000,
        ),
    );
    let exact_cover = tick_with_heat(&mut just_warm, Energy::ZERO);
    assert_eq!(exact_cover.thermal_comfort_ppm(), 0);
    assert_eq!(exact_cover.vitality(), initial_vitality);
    let following = tick_with_heat(&mut just_warm, balanced_heat);
    assert_eq!(following.thermal_comfort_ppm(), 0);
    assert_eq!(following.vitality(), sustained_cold.vitality());
}

#[test]
fn survival_initialization_and_tick_are_deterministic_and_visible() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    let before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("initialized survival record is missing"));

    let plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
    .unwrap_or_else(|error| panic!("survival tick failed: {error:?}"))
    .unwrap_or_else(|| panic!("survival tick did not produce a plan"));
    let after = apply_survival_tick(&mut state, Some(plan))
        .unwrap_or_else(|| panic!("survival tick lost the player"));

    assert!(after.metabolic_energy() < before.metabolic_energy());
    assert!(after.hydration() < before.hydration());
    assert_eq!(after.vitality(), Vitality::MAXIMUM);
    assert!(after.diet_quality_ppm() < before.diet_quality_ppm());
    assert_eq!(after.diet_supported_vitality_recovery_ppm_per_tick(), 10);
}

#[test]
fn a_day_of_unreplenished_diet_exposes_a_useful_recovery_choice() {
    let registries = build_registries();
    let nutrition = registries.survival().physiology().nutrition();
    let day = registries.core().calendar().ticks_per_day();
    let daily_decay = u32::try_from(u64::from(nutrition.decay_ppm_per_tick()) * day)
        .unwrap_or_else(|_| panic!("authored daily nutrition decay exceeds u32"));
    // Daily reserve decay keeps food-group choices consequential during expeditions.
    assert_eq!(daily_decay, 600_000);
    let depleted = NutritionReserves::FULL.decay(daily_decay);
    assert_eq!(depleted.quality_ppm(), 400_000);
    let (fruit_refed, gained) = depleted.add(FoodCategory::Fruit, 600_000);
    assert_eq!(gained, 600_000);
    assert!(fruit_refed.quality_ppm() > depleted.quality_ppm());
    let (grain_refed, _) = fruit_refed.add(FoodCategory::Grain, 600_000);
    let (balanced, _) = grain_refed.add(FoodCategory::Protein, 600_000);
    assert_eq!(balanced.quality_ppm(), 1_000_000);
    assert!(balanced.quality_ppm() > fruit_refed.quality_ppm());
}

#[test]
fn balanced_recent_diet_recovers_vitality_faster_than_one_category() {
    let registries = build_registries();
    let mut balanced = AppState::new();
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
            NutritionReserves::from_parts_per_million(300_000, 300_000, 300_000),
            0,
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
            NutritionReserves::from_parts_per_million(900_000, 0, 0),
            0,
        ),
    );

    let balanced_before = assess_survival(&registries, &balanced)
        .unwrap_or_else(|| panic!("balanced nutrition assessment disappeared"));
    assert_eq!(
        balanced_before.diet_supported_vitality_recovery_ppm_per_tick(),
        3
    );

    let balanced_plan = decide_survival_tick(
        &registries,
        &balanced,
        SurvivalExertion::REST,
        next_tick(&balanced),
    )
    .unwrap_or_else(|error| panic!("balanced nutrition tick failed: {error:?}"));
    let one_category_plan = decide_survival_tick(
        &registries,
        &one_category,
        SurvivalExertion::REST,
        next_tick(&one_category),
    )
    .unwrap_or_else(|error| panic!("single-category nutrition tick failed: {error:?}"));
    let balanced_after = apply_survival_tick(&mut balanced, balanced_plan)
        .unwrap_or_else(|| panic!("balanced nutrition player disappeared"));
    let one_category_after = apply_survival_tick(&mut one_category, one_category_plan)
        .unwrap_or_else(|| panic!("single-category nutrition player disappeared"));

    assert!(balanced_after.vitality() > one_category_after.vitality());
    assert_eq!(
        balanced_after.vitality().parts_per_million(),
        500_000 + balanced_before.diet_supported_vitality_recovery_ppm_per_tick()
    );
    assert_eq!(one_category_after.vitality().parts_per_million(), 500_001);
    assert!(balanced_after.diet_quality_ppm() > one_category_after.diet_quality_ppm());
    assert_eq!(balanced_after.diet_quality_ppm(), 299_975);
    assert_eq!(one_category_after.diet_quality_ppm(), 149_995);
    assert_eq!(
        balanced_after.diet_supported_vitality_recovery_ppm_per_tick(),
        3
    );
    assert_eq!(
        one_category_after.diet_supported_vitality_recovery_ppm_per_tick(),
        1
    );
}

#[test]
fn fruit_protein_reserves_support_recovery_without_grain_after_save_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("forager admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("forager admission commit failed: {error}"));
    let physiology = registries.survival().physiology();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::from_parts_per_million_unchecked(500_000),
            NutritionReserves::from_parts_per_million(0, 600_000, 600_000),
            0,
        ),
    );
    let before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("forager assessment missing"));
    assert_eq!(before.diet_quality_ppm(), 200_000);
    assert_eq!(before.diet_supported_vitality_recovery_ppm_per_tick(), 2);

    // The fixture establishes the injured player's current reserves. Subsequent neutral
    // simulation ticks, with no additional fixture mutation, must support recovery without grain.
    for _ in 0..20 {
        let _ = crate::simulation::advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("forager continuation failed: {error}"));
    }
    let after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("forager continuation lost survival"));
    assert_eq!(after.nutrition().get(FoodCategory::Grain), 0);
    assert!(after.vitality().parts_per_million() > 500_000);
    assert!(after.diet_quality_ppm() < before.diet_quality_ppm());
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("forager save failed: {error}"));
    let envelope: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("forager decode failed: {error}"));
    let mut restored = envelope
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("forager trusted load failed: {error}"));
    let mut original = state;
    for _ in 0..20 {
        let expected = crate::simulation::advance_tick(&registries, &mut original)
            .unwrap_or_else(|error| panic!("forager original continuation failed: {error}"));
        let actual = crate::simulation::advance_tick(&registries, &mut restored)
            .unwrap_or_else(|error| panic!("forager loaded continuation failed: {error}"));
        assert_eq!(expected, actual);
    }
    assert_eq!(original, restored);
}

#[test]
fn vitality_recovery_requires_both_energy_and_hydration_thresholds() {
    let registries = build_registries();
    let physiology = registries.survival().physiology();
    let vitality = Vitality::from_parts_per_million_unchecked(500_000);
    let energy_one_below_after_cost = physiology
        .hungry_below()
        .checked_add(physiology.basal_energy_cost_per_tick())
        .and_then(|energy| energy.checked_sub(Energy::from_nanojoules(1)))
        .unwrap_or_else(|| panic!("energy-threshold recovery fixture arithmetic failed"));
    let hydration_one_below_after_cost = physiology
        .thirsty_below()
        .checked_add(physiology.hydration_loss_per_tick())
        .and_then(|hydration| hydration.checked_sub(Volume::from_microliters(1)))
        .unwrap_or_else(|| panic!("hydration-threshold recovery fixture arithmetic failed"));

    let mut energy_limited = AppState::new();
    initialize_player_survival(&registries, &mut energy_limited)
        .unwrap_or_else(|error| panic!("energy-threshold survival initialization failed: {error}"));
    let expected_revision = energy_limited.survival().revision();
    energy_limited.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            energy_one_below_after_cost,
            physiology.maximum_hydration(),
            vitality,
            NutritionReserves::FULL,
            0,
        ),
    );

    let mut hydration_limited = AppState::new();
    initialize_player_survival(&registries, &mut hydration_limited).unwrap_or_else(|error| {
        panic!("hydration-threshold survival initialization failed: {error}")
    });
    let expected_revision = hydration_limited.survival().revision();
    hydration_limited.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            hydration_one_below_after_cost,
            vitality,
            NutritionReserves::FULL,
            0,
        ),
    );

    for (state, context) in [
        (&mut energy_limited, "energy-threshold"),
        (&mut hydration_limited, "hydration-threshold"),
    ] {
        let plan =
            decide_survival_tick(&registries, state, SurvivalExertion::REST, next_tick(state))
                .unwrap_or_else(|error| panic!("{context} recovery tick failed: {error:?}"));
        let after = apply_survival_tick(state, plan)
            .unwrap_or_else(|| panic!("{context} recovery player disappeared"));
        assert_eq!(
            after.vitality(),
            vitality,
            "{context} must block diet-supported recovery when either resource is below its warning threshold"
        );
    }
}

#[test]
fn paused_vitality_recovery_preserves_fractional_progress() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("paused recovery initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let hydration_one_below_after_cost = physiology
        .thirsty_below()
        .checked_add(physiology.hydration_loss_per_tick())
        .and_then(|hydration| hydration.checked_sub(Volume::from_microliters(1)))
        .unwrap_or_else(|| panic!("paused recovery hydration arithmetic failed"));
    let vitality = Vitality::from_parts_per_million_unchecked(500_000);
    let recovery_remainder = 123_456;
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            hydration_one_below_after_cost,
            vitality,
            NutritionReserves::FULL,
            recovery_remainder,
        ),
    );

    let plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
    .unwrap_or_else(|error| panic!("paused recovery tick failed: {error:?}"));
    let after = apply_survival_tick(&mut state, plan)
        .unwrap_or_else(|| panic!("paused recovery player disappeared"));

    assert_eq!(after.vitality(), vitality);
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.vitality_recovery_remainder()),
        Some(recovery_remainder),
        "temporary hunger or thirst must pause fractional recovery rather than discard earned progress"
    );
}

#[test]
fn fractional_diet_recovery_accumulates_instead_of_creating_rate_cliffs() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("fractional recovery admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("fractional recovery admission commit failed: {error}"));
    let physiology = registries.survival().physiology();
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::from_parts_per_million_unchecked(500_000),
            NutritionReserves::from_parts_per_million(999_995, 999_995, 999_995),
            0,
        ),
    );

    let first_plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
    .unwrap_or_else(|error| panic!("first fractional recovery tick failed: {error:?}"));
    let first = apply_survival_tick(&mut state, first_plan)
        .unwrap_or_else(|| panic!("first fractional recovery assessment disappeared"));
    assert_eq!(first.vitality().parts_per_million(), 500_009);
    assert_eq!(first.diet_supported_vitality_recovery_ppm_per_tick(), 10);
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| player.vitality_recovery_remainder()),
        Some(999_950)
    );

    let second_plan = decide_survival_tick(
        &registries,
        &state,
        SurvivalExertion::REST,
        next_tick(&state),
    )
    .unwrap_or_else(|error| panic!("second fractional recovery tick failed: {error:?}"));
    let second = apply_survival_tick(&mut state, second_plan)
        .unwrap_or_else(|| panic!("second fractional recovery assessment disappeared"));
    assert_eq!(second.vitality().parts_per_million(), 500_019);
    assert_eq!(second.diet_supported_vitality_recovery_ppm_per_tick(), 10);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("fractional recovery serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("fractional recovery deserialization failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("fractional recovery load validation failed: {error}"));
    assert_eq!(loaded, state);
}
