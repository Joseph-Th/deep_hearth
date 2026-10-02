//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_fluid_store, seed_geological_deposit, seed_lot,
    seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary, seed_preexisting_world_age, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_ORE, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER, PROSPECTING_REGIONAL_RECONNAISSANCE,
};
use deep_hearth::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Pressure, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{SimulationTick, TickSpan};
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::fluid::{FluidStoreId, calculate_fluid_volume_accounting};
use deep_hearth::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile, StorageDefinitionId,
    validate_build_storage_enclosure, validate_start_storage_enclosure_dismantling,
};
use deep_hearth::labor::{
    ManualPowerRequest, PlayerWork, ProspectingMethodId, project_manual_power,
    project_prospecting_work, validate_start_manual_power,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{
    DrinkDefinition, DrinkOutcome, EatOutcome, FoodCategory, FoodDefinition, FoodFreshness,
    SurvivalExertion, ValidatedDrink, assess_food_freshness, assess_survival,
    initialize_player_survival, project_food_freshness_after_storage_transition,
    project_survival_resource_budget, validate_drink, validate_drink_store_to_full,
    validate_drink_store_to_hydration_target, validate_eat,
};

use super::environment::ROOM_TEMPERATURE;
use super::focused_case::FocusedProbeCase;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use super::temporal::advance_idle_ticks;

#[path = "survival_probe/preservation.rs"]
pub(super) mod preservation;
use preservation::preservation_candidates;
pub(super) use preservation::{
    PreservationInvestmentPolicy, preservation_attention_value_ppm,
    preservation_material_budget_ppm, preservation_minimum_return_ppm,
};

#[path = "survival_probe/preservation_evaluation.rs"]
pub(super) mod preservation_evaluation;

#[path = "survival_probe/preservation_decision.rs"]
pub(super) mod preservation_decision;
use preservation_decision::evaluate_preservation_decision;

#[path = "survival_probe/explanation.rs"]
pub(super) mod explanation;
use explanation::{diet_comparison_explanation, preservation_comparison_explanation};

#[path = "survival_probe/pressure_response.rs"]
mod pressure_response;
use pressure_response::evaluate_survival_pressure_response_probe;

#[path = "survival_probe/work_pressure.rs"]
mod work_pressure;
use work_pressure::evaluate_survival_work_pressure_probe;
pub(super) use work_pressure::prospecting_method_for_work_pressure;

#[path = "survival_probe/integrated_work.rs"]
mod integrated_work;
use integrated_work::evaluate_integrated_survival_work_loop;

#[path = "survival_probe/report.rs"]
mod report;
pub(super) use report::run_survival_provisioning_probe;

#[path = "survival_probe/provisioning_support.rs"]
pub(super) mod provisioning_support;
use provisioning_support::*;
#[cfg(test)]
#[path = "survival_probe/provisioning_support_tests.rs"]
mod provisioning_support_tests;

#[path = "survival_probe/provisioning_world.rs"]
pub(super) mod provisioning_world;
use provisioning_world::{
    PreparedProvisioningWorld, ProvisioningPlan, maximum_direct_provisioning_ticks,
    prepare_provisioning_world, provisioning_plan,
};
pub(super) use provisioning_world::{ProvisioningWorld, provisioning_world};

#[path = "survival_probe/provisioning_evaluation.rs"]
mod provisioning_evaluation;
use provisioning_evaluation::{
    evaluate_provisioning_comparison, preservation_commodity_report_label,
    preservation_storage_report_label,
};

const DIET_RECOVERY_TARGET_VITALITY_PPM: u32 = 950_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SurvivalStartProfile {
    FullReserve,
    HungerWarningBoundary,
    HydrationWarningBoundary,
}

impl SurvivalStartProfile {
    const fn label(self) -> &'static str {
        match self {
            Self::FullReserve => "full-reserve",
            Self::HungerWarningBoundary => "hunger-warning-boundary",
            Self::HydrationWarningBoundary => "hydration-warning-boundary",
        }
    }
}

fn fresh_age(registries: &Registries, state: &AppState, lot: MaterialLotId) -> u64 {
    match assess_food_freshness(registries, state, lot)
        .unwrap_or_else(|error| panic!("survival probe freshness projection failed: {error:?}"))
    {
        FoodFreshness::Fresh { age, remaining: _ } => age.value(),
        FoodFreshness::Spoiled { age } => {
            panic!(
                "survival probe bounded setup unexpectedly spoiled food at age {} ticks",
                age.value()
            )
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProvisioningPriority {
    MetabolicEnergy,
    Hydration,
    Balanced,
}

impl ProvisioningPriority {
    const fn label(self) -> &'static str {
        match self {
            Self::MetabolicEnergy => "energy",
            Self::Hydration => "hydration",
            Self::Balanced => "balanced",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DietProvisioningPolicy {
    CompactCalories,
    BalancedRecovery,
}

impl DietProvisioningPolicy {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::CompactCalories => "compact-calories",
            Self::BalancedRecovery => "balanced-recovery",
        }
    }
}

pub(super) fn diet_provisioning_policy_for_behavior_seed(
    behavior_seed: u64,
) -> DietProvisioningPolicy {
    // Focused behavior generation deliberately stratifies this low bit in exploratory samples while
    // leaving the physical world seed independent. Maintained/replay seeds remain exact and stable.
    if behavior_seed.is_multiple_of(2) {
        DietProvisioningPolicy::CompactCalories
    } else {
        DietProvisioningPolicy::BalancedRecovery
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurvivalCaseReview {
    policy: DietProvisioningPolicy,
    meal_mass_mg: u64,
    drink_volume_ul: u64,
    selected_category_count: usize,
    diet_quality_before_ppm: u32,
    diet_quality_after_ppm: u32,
    recovery_rate_before_ppm_per_tick: u32,
    recovery_rate_after_ppm_per_tick: u32,
    reserve_recovered: bool,
    preservation_age_saved_ticks: u64,
    retained_preserved_mass_mg: u64,
    energy_deficit_ppm: u32,
    hydration_deficit_ppm: u32,
    provisioning_priority: ProvisioningPriority,
    provisioning_elapsed_ticks: u64,
    comparison_horizon_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DietComparisonReview {
    compact: SurvivalCaseReview,
    balanced: SurvivalCaseReview,
    natural_policy: DietProvisioningPolicy,
    natural: SurvivalCaseReview,
    available_category_count: usize,
    policy_sensitive: bool,
    comparison_horizon_ticks: u64,
    midwait_drink_count: u64,
    midwait_drink_volume_ul: u64,
    meal_mass_delta_mg: i128,
    water_saved_delta_ul: i128,
    diet_quality_delta_ppm: i64,
    recovery_rate_delta_ppm_per_tick: i64,
    reserve_recovered: bool,
    recovery: DietRecoveryReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DietRecoveryReview {
    actionable: bool,
    deprivation_ticks: u64,
    provisioning_horizon_ticks: u64,
    observation_ticks: u64,
    vitality_before_ppm: u32,
    compact_vitality_after_ppm: u32,
    balanced_vitality_after_ppm: u32,
    realized_vitality_delta_ppm: i64,
    compact_diet_quality_ppm: u32,
    balanced_diet_quality_ppm: u32,
    compact_recovery_window_delta_ppm: i64,
    balanced_recovery_window_delta_ppm: i64,
    compact_meal_actions: u64,
    compact_drink_actions: u64,
    balanced_meal_actions: u64,
    balanced_drink_actions: u64,
}

impl DietRecoveryReview {
    const fn supply_collapsed() -> Self {
        Self {
            actionable: false,
            deprivation_ticks: 0,
            provisioning_horizon_ticks: 0,
            observation_ticks: 0,
            vitality_before_ppm: 0,
            compact_vitality_after_ppm: 0,
            balanced_vitality_after_ppm: 0,
            realized_vitality_delta_ppm: 0,
            compact_diet_quality_ppm: 0,
            balanced_diet_quality_ppm: 0,
            compact_recovery_window_delta_ppm: 0,
            balanced_recovery_window_delta_ppm: 0,
            compact_meal_actions: 0,
            compact_drink_actions: 0,
            balanced_meal_actions: 0,
            balanced_drink_actions: 0,
        }
    }
}

pub(super) fn selected_food_indices(
    foods: &[FoodDefinition],
    policy: DietProvisioningPolicy,
) -> Vec<usize> {
    fn compact_category_rank(category: FoodCategory) -> u8 {
        // Explicit actor policy for otherwise equivalent calorie-density choices. Keeping this
        // exhaustive prevents enum declaration order from becoming an accidental tie-breaker.
        match category {
            FoodCategory::Grain => 0,
            FoodCategory::Fruit => 1,
            FoodCategory::Protein => 2,
        }
    }

    let mut indices = (0..foods.len()).collect::<Vec<_>>();
    match policy {
        DietProvisioningPolicy::BalancedRecovery => indices,
        DietProvisioningPolicy::CompactCalories => {
            indices.sort_by(|left, right| {
                foods[*right]
                    .dietary_energy()
                    .nanojoules_per_milligram()
                    .cmp(&foods[*left].dietary_energy().nanojoules_per_milligram())
                    .then_with(|| {
                        compact_category_rank(foods[*left].category())
                            .cmp(&compact_category_rank(foods[*right].category()))
                    })
            });
            indices.truncate(indices.len().min(2));
            indices
        }
    }
}

struct DietRecoveryBranch<'a> {
    prepared: &'a AppState,
    foods: &'a [FoodDefinition],
    food_store: StockpileId,
    food_lots: &'a [MaterialLotId],
    drink_store: FluidStoreId,
    matter_total: AggregateMass,
    fluid_total: AggregateVolume,
}

struct DietRecoveryProvisioned {
    state: AppState,
    elapsed_ticks: u64,
    meal_actions: u64,
    drink_actions: u64,
}

fn provision_diet_recovery_branch(
    registries: &Registries,
    branch: &DietRecoveryBranch<'_>,
    policy: DietProvisioningPolicy,
    recovery_ticks: u64,
) -> DietRecoveryProvisioned {
    let mut state = branch.prepared.clone();
    let mut provisioning_elapsed_ticks = 0_u64;
    let mut meal_actions = 0_u64;
    let mut drink_actions = 0_u64;
    let physiology = registries.survival().physiology();
    let recovery_budget = project_survival_resource_budget(
        physiology,
        SurvivalExertion::REST,
        TickSpan::new(recovery_ticks),
    )
    .unwrap_or_else(|error| panic!("diet-recovery resource projection failed: {error:?}"));
    let metabolic_target = physiology
        .hungry_below()
        .checked_add(recovery_budget.metabolic_energy())
        .unwrap_or(physiology.maximum_metabolic_energy())
        .min(physiology.maximum_metabolic_energy());
    let hydration_target = physiology
        .thirsty_below()
        .checked_add(recovery_budget.hydration())
        .unwrap_or(physiology.maximum_hydration())
        .min(physiology.maximum_hydration());
    let before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared before provisioning"));
    let selected_indices = selected_food_indices(branch.foods, policy);
    assert!(
        before.metabolic_energy() <= physiology.maximum_metabolic_energy(),
        "diet-recovery metabolic reserve exceeded authored maximum"
    );
    loop {
        let current = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared while eating"));
        // Recovery is the disclosed task. The actor can therefore provision enough reserve to stay
        // above the hunger warning through that known rest window, while still taking at least one
        // nutrition-bearing meal because low vitality makes dietary recovery immediately relevant.
        if meal_actions > 0 && current.metabolic_energy() >= metabolic_target {
            break;
        }
        let selected_masses =
            desired_policy_meal_masses(registries, &state, branch.foods, &selected_indices);
        let selections = selected_indices
            .iter()
            .zip(&selected_masses)
            .filter_map(|(index, desired)| {
                let available = state
                    .inventory()
                    .get_lot(branch.food_lots[*index])
                    .unwrap_or_else(|| panic!("diet-recovery food lot disappeared"))
                    .mass();
                let portion = (*desired).min(available);
                (!portion.is_zero())
                    .then(|| MaterialLotSelection::new(branch.food_lots[*index], portion))
            })
            .collect::<Vec<_>>();
        let offered = selections
            .iter()
            .try_fold(Mass::ZERO, |total, selection| {
                total.checked_add(selection.mass())
            })
            .unwrap_or_else(|| panic!("diet-recovery serving mass overflowed"));
        assert!(
            offered >= physiology.direct_consumption().minimum_meal_mass(),
            "diet-recovery finite food supply cannot provide another legal serving"
        );
        let meal = validate_eat(registries, &state, branch.food_store, &selections)
            .unwrap_or_else(|error| panic!("diet-recovery meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("diet-recovery meal commit failed: {error}"));
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                meal.completes_at(),
            ))
            .unwrap_or_else(|| panic!("diet-recovery provisioning duration overflowed"));
        meal_actions = meal_actions
            .checked_add(1)
            .unwrap_or_else(|| panic!("diet-recovery meal action count overflowed"));
        let after_meal = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared after eating"));
        assert!(
            after_meal.metabolic_energy() > current.metabolic_energy(),
            "diet-recovery meal did not make progress toward the disclosed metabolic reserve target"
        );
    }

    while assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared while drinking"))
        .hydration()
        < hydration_target
    {
        let current_hydration = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared before drinking"))
            .hydration();
        let validated = match validate_drink_store_to_hydration_target(
            registries,
            &state,
            branch.drink_store,
            hydration_target,
        ) {
            Ok(Some(drink)) => drink,
            Ok(None) => unreachable!("thirst threshold was checked as unmet"),
            Err(deep_hearth::survival::DrinkStoreToTargetError::Projection(
                deep_hearth::survival::DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            )) => validate_drink(registries, &state, branch.drink_store, maximum_drink_volume)
                .unwrap_or_else(|error| {
                    panic!("diet-recovery serving drink validation failed: {error}")
                }),
            Err(error) => panic!("diet-recovery drink-to-warning validation failed: {error}"),
        };
        let drink = validated
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("diet-recovery drink commit failed: {error}"));
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                drink.completes_at(),
            ))
            .unwrap_or_else(|| panic!("diet-recovery provisioning duration overflowed"));
        drink_actions = drink_actions
            .checked_add(1)
            .unwrap_or_else(|| panic!("diet-recovery drink action count overflowed"));
        let after_drink = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared after drinking"));
        assert!(
            after_drink.hydration() > current_hydration,
            "diet-recovery drink did not make progress toward the disclosed hydration target"
        );
    }
    DietRecoveryProvisioned {
        state,
        elapsed_ticks: provisioning_elapsed_ticks,
        meal_actions,
        drink_actions,
    }
}

struct DietRecoveryObservation {
    diet_quality_ppm: u32,
    vitality_after_ppm: u32,
    vitality_window_delta_ppm: i64,
}

fn observe_diet_recovery_branch(
    registries: &Registries,
    branch: &DietRecoveryBranch<'_>,
    mut provisioned: DietRecoveryProvisioned,
    comparison_horizon_ticks: u64,
    observation_ticks: u64,
) -> DietRecoveryObservation {
    assert!(
        provisioned.elapsed_ticks <= comparison_horizon_ticks,
        "diet-recovery branch exceeded the policy-independent comparison horizon"
    );
    advance_idle_ticks(
        registries,
        &mut provisioned.state,
        comparison_horizon_ticks - provisioned.elapsed_ticks,
        "diet-recovery matched horizon",
    );
    let restored = assess_survival(registries, &provisioned.state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared at matched observation start"));
    let diet_quality_ppm = restored.diet_quality_ppm();
    advance_idle_ticks(
        registries,
        &mut provisioned.state,
        observation_ticks,
        "diet-recovery observation",
    );
    let recovered = assess_survival(registries, &provisioned.state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared during recovery"));
    assert_eq!(
        calculate_matter_accounting(&provisioned.state)
            .unwrap_or_else(|error| panic!("diet-recovery matter audit failed: {error}"))
            .total(),
        branch.matter_total,
        "diet-recovery eating must conserve represented matter"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&provisioned.state)
            .unwrap_or_else(|error| panic!("diet-recovery fluid audit failed: {error}"))
            .total(),
        branch.fluid_total,
        "diet-recovery drinking must conserve represented fluid"
    );
    validate_loaded_state(registries, &provisioned.state)
        .unwrap_or_else(|error| panic!("diet-recovery persistence audit failed: {error}"));
    DietRecoveryObservation {
        diet_quality_ppm,
        vitality_after_ppm: recovered.vitality().parts_per_million(),
        vitality_window_delta_ppm: i64::from(recovered.vitality().parts_per_million())
            - i64::from(restored.vitality().parts_per_million()),
    }
}

fn evaluate_diet_recovery_consequence(
    registries: &Registries,
    world: &ProvisioningWorld,
) -> DietRecoveryReview {
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    if food_category_count(&world.foods) < authored_category_count {
        return DietRecoveryReview::supply_collapsed();
    }

    let mut state = AppState::new();
    let physiology = registries.survival().physiology();
    let offered_masses = world
        .foods
        .iter()
        .map(|food| mass_for_target_energy(*food, physiology.maximum_metabolic_energy()))
        .collect::<Vec<_>>();
    let food_capacity = offered_masses
        .iter()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("diet-recovery food capacity overflowed"));
    let food_store = seed_stockpile(
        &mut state,
        food_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let food_lots = world
        .foods
        .iter()
        .zip(&offered_masses)
        .map(|(food, mass)| {
            seed_lot(
                registries,
                &mut state,
                food_store,
                food.commodity(),
                *mass,
                ROOM_TEMPERATURE,
            )
        })
        .collect::<Vec<_>>();
    let drink_supply = world
        .drink
        .minimum_volume_for_hydration(physiology.maximum_hydration())
        .unwrap_or_else(|| panic!("diet-recovery fluid supply exceeds authoritative range"));
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        drink_supply,
        world.drink.fluid(),
        drink_supply,
        ROOM_TEMPERATURE,
    );
    // Every provision exists before admission. The vitality deficit is created only by canonical
    // simulation ticks so the recovery decision does not depend on post-admission fixture mutation.
    super::world_admission::admit_stationary_player(
        registries,
        &mut state,
        &[food_store],
        &[drink_store],
        "diet recovery",
    );
    let maximum_deprivation_ticks = registries
        .core()
        .calendar()
        .ticks_per_day()
        .checked_mul(2)
        .unwrap_or_else(|| panic!("diet-recovery setup horizon overflowed"));
    let mut deprivation_ticks = 0_u64;
    loop {
        let assessment = assess_survival(registries, &state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared during deprivation"));
        if assessment.vitality().parts_per_million() <= DIET_RECOVERY_TARGET_VITALITY_PPM {
            break;
        }
        assert!(
            deprivation_ticks < maximum_deprivation_ticks,
            "diet-recovery setup could not create a bounded real vitality deficit within two authored world days"
        );
        advance_idle_ticks(registries, &mut state, 1, "diet-recovery deprivation");
        deprivation_ticks += 1;
    }
    let vitality_before_ppm = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared at decision point"))
        .vitality()
        .parts_per_million();

    let matter_total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("diet-recovery initial matter audit failed: {error}"))
        .total();
    let fluid_total = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("diet-recovery initial fluid audit failed: {error}"))
        .total();
    let branch = DietRecoveryBranch {
        prepared: &state,
        foods: &world.foods,
        food_store,
        food_lots: &food_lots,
        drink_store,
        matter_total,
        fluid_total,
    };
    // Nutrition is a long-horizon recovery lever, not an immediate consumption bonus. Observe an
    // eighth of one authored day so the harness measures a player-relevant multi-hour recovery
    // window without making exploratory reports pay for an unnecessarily long idle simulation.
    let observation_ticks = (registries.core().calendar().ticks_per_day() / 8).max(1);

    let compact_provisioned = provision_diet_recovery_branch(
        registries,
        &branch,
        DietProvisioningPolicy::CompactCalories,
        observation_ticks,
    );
    let balanced_provisioned = provision_diet_recovery_branch(
        registries,
        &branch,
        DietProvisioningPolicy::BalancedRecovery,
        observation_ticks,
    );
    // The comparison horizon is evaluator-only. Neither actor sees the other branch or this
    // alignment; both choose servings from the same decision state, then the earlier-finishing
    // branch idles until the slower legitimate provisioning sequence completes.
    let comparison_horizon_ticks = compact_provisioned
        .elapsed_ticks
        .max(balanced_provisioned.elapsed_ticks);
    let compact_meal_actions = compact_provisioned.meal_actions;
    let compact_drink_actions = compact_provisioned.drink_actions;
    let balanced_meal_actions = balanced_provisioned.meal_actions;
    let balanced_drink_actions = balanced_provisioned.drink_actions;
    let compact_observation = observe_diet_recovery_branch(
        registries,
        &branch,
        compact_provisioned,
        comparison_horizon_ticks,
        observation_ticks,
    );
    let balanced_observation = observe_diet_recovery_branch(
        registries,
        &branch,
        balanced_provisioned,
        comparison_horizon_ticks,
        observation_ticks,
    );
    assert!(
        balanced_observation.diet_quality_ppm > compact_observation.diet_quality_ppm,
        "all-category provisioning must create stronger diet quality in the real recovery challenge: compact={}ppm ({compact_meal_actions} meals/{compact_drink_actions} drinks) balanced={}ppm ({balanced_meal_actions} meals/{balanced_drink_actions} drinks) horizon={comparison_horizon_ticks}t",
        compact_observation.diet_quality_ppm,
        balanced_observation.diet_quality_ppm,
    );
    let realized_vitality_delta_ppm = i64::from(balanced_observation.vitality_after_ppm)
        - i64::from(compact_observation.vitality_after_ppm);
    DietRecoveryReview {
        actionable: true,
        deprivation_ticks,
        provisioning_horizon_ticks: comparison_horizon_ticks,
        observation_ticks,
        vitality_before_ppm,
        compact_vitality_after_ppm: compact_observation.vitality_after_ppm,
        balanced_vitality_after_ppm: balanced_observation.vitality_after_ppm,
        realized_vitality_delta_ppm,
        compact_diet_quality_ppm: compact_observation.diet_quality_ppm,
        balanced_diet_quality_ppm: balanced_observation.diet_quality_ppm,
        compact_recovery_window_delta_ppm: compact_observation.vitality_window_delta_ppm,
        balanced_recovery_window_delta_ppm: balanced_observation.vitality_window_delta_ppm,
        compact_meal_actions,
        compact_drink_actions,
        balanced_meal_actions,
        balanced_drink_actions,
    }
}

fn normalized_energy_deficit_ppm(
    maximum: deep_hearth::core::quantity::Energy,
    current: deep_hearth::core::quantity::Energy,
) -> u32 {
    let deficit = maximum
        .checked_sub(current)
        .unwrap_or_else(|| panic!("survival probe metabolic reserve exceeded authored maximum"));
    u32::try_from(
        deficit
            .nanojoules()
            .checked_mul(1_000_000)
            .map(|scaled| scaled / maximum.nanojoules())
            .unwrap_or_else(|| panic!("survival probe energy deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe energy deficit normalization exceeded u32"))
}

fn normalized_hydration_deficit_ppm(maximum: Volume, current: Volume) -> u32 {
    let deficit = maximum
        .checked_sub(current)
        .unwrap_or_else(|| panic!("survival probe hydration reserve exceeded authored maximum"));
    u32::try_from(
        u128::from(deficit.microliters())
            .checked_mul(1_000_000)
            .map(|scaled| scaled / u128::from(maximum.microliters()))
            .unwrap_or_else(|| panic!("survival probe hydration deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe hydration deficit normalization exceeded u32"))
}

fn provisioning_priority_from_reserves(
    maximum_energy: Energy,
    current_energy: Energy,
    maximum_hydration: Volume,
    current_hydration: Volume,
) -> ProvisioningPriority {
    let energy_deficit = maximum_energy
        .checked_sub(current_energy)
        .unwrap_or_else(|| panic!("survival probe metabolic reserve exceeded authored maximum"));
    let hydration_deficit = maximum_hydration
        .checked_sub(current_hydration)
        .unwrap_or_else(|| panic!("survival probe hydration reserve exceeded authored maximum"));
    let energy_pressure = energy_deficit
        .nanojoules()
        .checked_mul(u128::from(maximum_hydration.microliters()))
        .unwrap_or_else(|| panic!("survival probe normalized energy pressure overflowed"));
    let hydration_pressure = u128::from(hydration_deficit.microliters())
        .checked_mul(maximum_energy.nanojoules())
        .unwrap_or_else(|| panic!("survival probe normalized hydration pressure overflowed"));
    match hydration_pressure.cmp(&energy_pressure) {
        std::cmp::Ordering::Greater => ProvisioningPriority::Hydration,
        std::cmp::Ordering::Less => ProvisioningPriority::MetabolicEnergy,
        std::cmp::Ordering::Equal => ProvisioningPriority::Balanced,
    }
}
