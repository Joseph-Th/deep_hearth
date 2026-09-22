//! Matched provisioning branch execution, conservation checks, and comparison evidence.

use super::*;

pub(super) fn run_provisioning_case(
    registries: &Registries,
    behavior_seed: u64,
    world: &ProvisioningWorld,
    prepared: &PreparedProvisioningWorld,
    plan: &ProvisioningPlan,
    policy: DietProvisioningPolicy,
    comparison_horizon_ticks: u64,
) -> SurvivalCaseReview {
    let foods = world.foods.as_slice();
    let witness_index = world.witness_index;
    let preservation_multiplier_ppm = world.inherited_preservation_multiplier_ppm;
    let age_ticks = world.age_ticks;
    let provisioning_wait_ticks = world.provisioning_wait_ticks;
    let drink = world.drink;
    let physiology = registries.survival().physiology();
    let witness_food = foods[witness_index];
    let witness_mass = world.preserved_reserve_mass;
    let selected_indices = plan.selected_indices.as_slice();
    let selected_masses = plan.selected_masses.as_slice();
    let ambient_age = prepared.ambient_age;
    let preserved_age = prepared.preserved_age;
    let preservation_age_saved_ticks = prepared.preservation_age_saved_ticks;
    // Counterfactual evaluation branches only after the shared world reaches the observable
    // provisioning decision point. The acting policy never sees or queries the comparison branch.
    let mut state = prepared.state.clone();

    let before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared before provisioning"));
    let energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        before.metabolic_energy(),
    );
    let hydration_deficit_ppm =
        normalized_hydration_deficit_ppm(physiology.maximum_hydration(), before.hydration());
    let provisioning_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        before.metabolic_energy(),
        physiology.maximum_hydration(),
        before.hydration(),
    );
    let drink_first = match provisioning_priority {
        ProvisioningPriority::Hydration => true,
        ProvisioningPriority::MetabolicEnergy => false,
        ProvisioningPriority::Balanced => {
            mix64(behavior_seed ^ 0x5052_4F56_4953_494F).is_multiple_of(2)
        }
    };
    let selections = selected_indices
        .iter()
        .zip(selected_masses)
        .map(|(index, mass)| MaterialLotSelection::new(prepared.prepared_lots[*index], *mass))
        .collect::<Vec<_>>();
    let selected_categories = selected_indices
        .iter()
        .map(|index| foods[*index].category())
        .collect::<Vec<_>>();
    assert_eq!(
        selected_categories
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        selected_categories.len(),
        "survival probe meal selection must not duplicate a dietary category"
    );
    let diet_quality_before = before.diet_quality_ppm();
    let recovery_rate_before = before.diet_supported_vitality_recovery_ppm_per_tick();
    let mut no_provision_baseline = state.clone();
    let actions = execute_provisioning_actions(
        registries,
        &mut state,
        prepared,
        drink,
        &selections,
        drink_first,
    );
    let provisioning_elapsed_ticks = actions.elapsed_ticks;
    let meal = actions.meal;
    let drank_volume = actions.drank_volume;
    let hydration_offered = actions.hydration_offered;
    let action_order = actions.action_order;
    assert!(
        provisioning_elapsed_ticks <= comparison_horizon_ticks,
        "provisioning branch exceeded the precomputed matched comparison horizon"
    );
    advance_idle_ticks(
        registries,
        &mut state,
        comparison_horizon_ticks - provisioning_elapsed_ticks,
        "provisioning matched horizon",
    );
    advance_idle_ticks(
        registries,
        &mut no_provision_baseline,
        comparison_horizon_ticks,
        "no-provision matched horizon",
    );
    assert_eq!(
        state.tick(),
        no_provision_baseline.tick(),
        "provisioning and no-provision branches must be observed at the same world tick"
    );
    assert_eq!(meal.portions().len(), selections.len());
    for category in selected_categories.iter().copied() {
        assert!(
            meal.nutrition_offered().get(category) > 0,
            "survival probe varied meal must contribute every selected food category"
        );
    }
    assert!(!meal.energy_offered().is_zero());
    if !drank_volume.is_zero() {
        assert!(!hydration_offered.is_zero());
    }
    assert_eq!(
        state
            .inventory()
            .get_lot(prepared.preserved_witness)
            .map(|lot| lot.mass()),
        Some(witness_mass),
        "food rotation must retain the fresher preserved witness instead of consuming it first"
    );

    let after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared after provisioning"));
    let reserve_recovered = after.metabolic_energy() > before.metabolic_energy()
        && after.hydration() > before.hydration();
    assert!(
        reserve_recovered,
        "one bounded provisioning pass must improve both depleted survival reserves"
    );
    assert!(after.metabolic_energy() <= physiology.maximum_metabolic_energy());
    assert!(after.hydration() <= physiology.maximum_hydration());
    let recovery_rate_after = after.diet_supported_vitality_recovery_ppm_per_tick();
    let baseline_after = assess_survival(registries, &no_provision_baseline)
        .unwrap_or_else(|| panic!("survival no-provision baseline player disappeared"));
    let baseline_recovery_rate = baseline_after.diet_supported_vitality_recovery_ppm_per_tick();
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    if selected_categories.len() == authored_category_count {
        assert!(after.diet_quality_ppm() > baseline_after.diet_quality_ppm());
        assert!(recovery_rate_after >= baseline_recovery_rate);
    } else {
        assert_eq!(after.diet_quality_ppm(), baseline_after.diet_quality_ppm());
        assert_eq!(recovery_rate_after, baseline_recovery_rate);
    }
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("survival probe final matter audit failed: {error}"))
            .total(),
        prepared.matter_total,
        "eating must transfer matter into survival ownership rather than delete it"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("survival probe final fluid audit failed: {error}"))
            .total(),
        prepared.fluid_total,
        "drinking must transfer finite fluid into survival ownership rather than delete it"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("survival probe final persistence audit failed: {error}"));

    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        let available_categories = foods
            .iter()
            .map(|food| format!("{:?}", food.category()))
            .collect::<Vec<_>>()
            .join("+");
        let selected_categories = selected_categories
            .iter()
            .map(|category| format!("{category:?}"))
            .collect::<Vec<_>>()
            .join("+");
        reviewln!(
            "PLAYABLE SURVIVAL behavior=0x{behavior_seed:016X} mode=matched-policy policy={} catalog=registry-derived world-bootstrap=[reserve-profile:{},authored-food,authored-drink,storage-profile] player-present-from=t0 available-categories={available_categories} selected-categories={selected_categories} food-rotation=[witness:{} elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient-age:{ambient_age}t preserved-age:{preserved_age}t age-saved:{preservation_age_saved_ticks}t consume:older-ambient retain-preserved:{}mg] wait={provisioning_wait_ticks}t lived-wait=[drinks:{} volume:{}uL] provisioning=[priority:{} action-order:{action_order}] meal=[mass:{}mg energy-offered:{}nJ nutrition-offered:{}ppm diet-quality:{}->{}ppm recovery-rate:{}->{}ppm/t] drink=[fluid:{} volume:{}uL hydration-offered:{}uL] reserves=improved matter=conserved fluid=conserved tick={}",
            policy.label(),
            world.start_profile.label(),
            witness_food.commodity().value(),
            witness_mass.milligrams(),
            prepared.midwait_drink_count,
            prepared.midwait_drink_volume_ul,
            provisioning_priority.label(),
            meal.total_mass().milligrams(),
            meal.energy_offered().nanojoules(),
            meal.nutrition_offered().total_ppm(),
            diet_quality_before,
            after.diet_quality_ppm(),
            recovery_rate_before,
            recovery_rate_after,
            drink.fluid().value(),
            drank_volume.microliters(),
            hydration_offered.microliters(),
            state.tick().value(),
        );
    }

    SurvivalCaseReview {
        policy,
        meal_mass_mg: meal.total_mass().milligrams(),
        drink_volume_ul: drank_volume.microliters(),
        selected_category_count: selected_categories.len(),
        diet_quality_before_ppm: diet_quality_before,
        diet_quality_after_ppm: after.diet_quality_ppm(),
        recovery_rate_before_ppm_per_tick: recovery_rate_before,
        recovery_rate_after_ppm_per_tick: recovery_rate_after,
        reserve_recovered,
        preservation_age_saved_ticks: prepared.preservation_age_saved_ticks,
        retained_preserved_mass_mg: witness_mass.milligrams(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        provisioning_priority,
        provisioning_elapsed_ticks,
        comparison_horizon_ticks,
    }
}

pub(super) fn evaluate_provisioning_comparison(
    registries: &Registries,
    seed: u64,
    behavior_seed: u64,
    world: &ProvisioningWorld,
) -> DietComparisonReview {
    let foods = world.foods.as_slice();
    let available_category_count = food_category_count(foods);
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    let drink_supply = provisioning_drink_supply(registries, world);
    let prepared = prepare_provisioning_world(registries, seed, world, drink_supply);
    let compact_plan = provisioning_plan(
        registries,
        world,
        &prepared,
        DietProvisioningPolicy::CompactCalories,
    );
    let balanced_plan = provisioning_plan(
        registries,
        world,
        &prepared,
        DietProvisioningPolicy::BalancedRecovery,
    );
    let comparison_horizon_ticks = maximum_direct_provisioning_ticks(registries);
    let compact = run_provisioning_case(
        registries,
        behavior_seed,
        world,
        &prepared,
        &compact_plan,
        DietProvisioningPolicy::CompactCalories,
        comparison_horizon_ticks,
    );
    let balanced = run_provisioning_case(
        registries,
        behavior_seed,
        world,
        &prepared,
        &balanced_plan,
        DietProvisioningPolicy::BalancedRecovery,
        comparison_horizon_ticks,
    );
    assert_eq!(compact.policy, DietProvisioningPolicy::CompactCalories);
    assert_eq!(balanced.policy, DietProvisioningPolicy::BalancedRecovery);
    assert_eq!(compact.energy_deficit_ppm, balanced.energy_deficit_ppm);
    assert_eq!(
        compact.hydration_deficit_ppm,
        balanced.hydration_deficit_ppm
    );
    assert_eq!(
        compact.provisioning_priority,
        balanced.provisioning_priority
    );
    assert_eq!(compact.comparison_horizon_ticks, comparison_horizon_ticks);
    assert_eq!(balanced.comparison_horizon_ticks, comparison_horizon_ticks);
    assert!(compact.provisioning_elapsed_ticks <= comparison_horizon_ticks);
    assert!(balanced.provisioning_elapsed_ticks <= comparison_horizon_ticks);
    assert_eq!(
        compact.preservation_age_saved_ticks,
        balanced.preservation_age_saved_ticks
    );
    assert_eq!(
        compact.retained_preserved_mass_mg,
        balanced.retained_preserved_mass_mg
    );
    assert!(compact.reserve_recovered && balanced.reserve_recovered);

    if available_category_count == authored_category_count {
        assert!(compact.selected_category_count < balanced.selected_category_count);
        assert!(compact.meal_mass_mg <= balanced.meal_mass_mg);
        assert!(balanced.diet_quality_after_ppm > compact.diet_quality_after_ppm);
        assert!(
            balanced.recovery_rate_after_ppm_per_tick >= compact.recovery_rate_after_ppm_per_tick
        );
        if world.start_profile == SurvivalStartProfile::FullReserve {
            assert!(
                balanced.recovery_rate_after_ppm_per_tick
                    > compact.recovery_rate_after_ppm_per_tick,
                "balanced provisioning must buy measurably stronger recovery resilience in the maintained long-horizon survival world"
            );
        }
    }

    let natural_policy = diet_provisioning_policy_for_behavior_seed(behavior_seed);
    let natural = match natural_policy {
        DietProvisioningPolicy::CompactCalories => compact,
        DietProvisioningPolicy::BalancedRecovery => balanced,
    };
    DietComparisonReview {
        compact,
        balanced,
        natural_policy,
        natural,
        available_category_count,
        policy_sensitive: available_category_count == authored_category_count,
        comparison_horizon_ticks,
        midwait_drink_count: prepared.midwait_drink_count,
        midwait_drink_volume_ul: prepared.midwait_drink_volume_ul,
        meal_mass_delta_mg: i128::from(balanced.meal_mass_mg) - i128::from(compact.meal_mass_mg),
        water_saved_delta_ul: i128::from(compact.drink_volume_ul)
            - i128::from(balanced.drink_volume_ul),
        diet_quality_delta_ppm: i64::from(balanced.diet_quality_after_ppm)
            - i64::from(compact.diet_quality_after_ppm),
        recovery_rate_delta_ppm_per_tick: i64::from(balanced.recovery_rate_after_ppm_per_tick)
            - i64::from(compact.recovery_rate_after_ppm_per_tick),
        reserve_recovered: compact.reserve_recovered && balanced.reserve_recovered,
        recovery: evaluate_diet_recovery_consequence(registries, seed, world),
    }
}

pub(super) fn preservation_storage_report_label(
    registries: &Registries,
    definition: StorageDefinitionId,
) -> String {
    let storage = registries.storage().get(definition).unwrap_or_else(|| {
        panic!(
            "preservation report storage {} disappeared",
            definition.value()
        )
    });
    format!(
        "{}:{}",
        definition.value(),
        storage.name().replace(' ', "-")
    )
}

pub(super) fn preservation_commodity_report_label(
    registries: &Registries,
    commodity: CommodityKey,
) -> String {
    let material = registries
        .materials()
        .get_material(commodity.material())
        .unwrap_or_else(|| panic!("preservation report material disappeared"));
    let form = registries
        .materials()
        .get_form(commodity.form())
        .unwrap_or_else(|| panic!("preservation report form disappeared"));
    format!(
        "{}:{}/{}",
        commodity.value(),
        material.name().replace(' ', "-"),
        form.name().replace(' ', "-")
    )
}
