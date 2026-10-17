//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, admit_player_at_hunger_warning_boundary,
    admit_player_at_hydration_warning_boundary, seed_fluid_store, seed_geological_deposit,
    seed_lot, seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary, seed_preexisting_world_age, seed_stockpile,
};
use deep_hearth::content::{
    CAMPFIRE_FUEL_MASS, CAMPFIRE_FUEL_THERMAL_ENERGY, ENERGY_STONE_FLYWHEEL_DRIVE,
    ENERGY_STONE_RING_CAMPFIRE, EQUIPMENT_STONE_HAND_CRANK, FORM_LOG, FORM_LUMP, FORM_ORE,
    MANUAL_POWER_HAND_CRANK, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROSPECTING_REGIONAL_RECONNAISSANCE,
};
use deep_hearth::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Pressure, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{SimulationTick, TickSpan};
use deep_hearth::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use deep_hearth::energy::{EnergyStoreId, validate_assemble_energy_store, validate_combust_fuel};
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::fluid::{FluidStoreId, calculate_fluid_volume_accounting};
use deep_hearth::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialStackProjection, StockpileId,
    StockpileStorageProfile, StorageDefinitionId, project_material_stacks,
    validate_build_storage_enclosure, validate_start_storage_enclosure_dismantling,
};
use deep_hearth::labor::{
    ManualPowerRequest, PlayerWork, ProspectingMethodId, ProspectingSpatialResolution,
    project_manual_power, project_prospecting_work, validate_start_manual_power,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{
    DrinkOutcome, EatOutcome, FoodDefinition, FoodFreshness, FoodStackPortion, LocalDrinkSource,
    SurvivalExertion, ValidatedDrink, assess_food_freshness, assess_food_stack_freshness,
    assess_survival, available_local_drink_sources,
    project_food_freshness_after_storage_transition, project_survival_resource_budget,
    validate_drink, validate_drink_store_to_full, validate_drink_store_to_hydration_target,
    validate_eat, validate_eat_stack_portions,
};

use super::environment::ROOM_TEMPERATURE;
use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_witnesses::SURVIVAL_CONTINUATION_COVERAGE_SEED;
use super::manual_craft_selection::select_manual_craft_request;
use super::manual_power_timing::finish_manual_power_work;
#[cfg(not(test))]
use super::physical_time::format_physical_duration;
use super::production_timing::finish_uninterrupted_production_job;
use super::seed::mix64;
use super::survival_provisioning_policy::{
    DietProvisioningPolicy, diet_provisioning_policy_from_visible_recovery, mass_for_target_energy,
    selected_food_indices,
};
use super::survival_world_generation::{
    ProvisioningWorld, SurvivalStartProfile, provisioning_world,
};
use super::temporal::advance_idle_ticks;

#[path = "survival_probe/preservation.rs"]
pub(super) mod preservation;
pub(super) use preservation::{
    PreservationInvestmentPolicy, preservation_attention_value_ppm,
    preservation_material_budget_ppm, preservation_minimum_return_ppm,
};

#[path = "survival_probe/preservation_evaluation.rs"]
pub(super) mod preservation_evaluation;

#[path = "survival_probe/preservation_decision.rs"]
pub(super) mod preservation_decision;
use preservation_decision::evaluate_preservation_decision;

#[cfg(not(test))]
#[path = "survival_probe/explanation.rs"]
pub(super) mod explanation;
#[cfg(not(test))]
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

#[cfg(not(test))]
#[path = "survival_probe/report.rs"]
mod report;
#[cfg(not(test))]
pub(super) use report::run_survival_provisioning_probe;

#[path = "survival_probe/provisioning_support.rs"]
pub(super) mod provisioning_support;
use provisioning_support::*;
#[cfg(test)]
include_survival_probe_contract_tests!();

#[path = "survival_probe/provisioning_runtime.rs"]
pub(super) mod provisioning_runtime;
use provisioning_runtime::{
    PlannedFoodStackPortion, PreparedProvisioningWorld, ProvisioningPlan,
    maximum_direct_provisioning_ticks, observed_provisioning_drink, observed_provisioning_foods,
    prepare_provisioning_world, provisioning_plan,
};

#[path = "survival_probe/provisioning_evaluation.rs"]
mod provisioning_evaluation;
use provisioning_evaluation::evaluate_provisioning_comparison;
#[cfg(not(test))]
use provisioning_evaluation::{
    preservation_commodity_report_label, preservation_storage_report_label,
};

const DIET_RECOVERY_TARGET_VITALITY_PPM: u32 = 950_000;
const DIET_RECOVERY_FIRE_FUEL_CHARGES: u64 = 6;

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
    meal_mass_delta_mg: i128,
    water_saved_delta_ul: i128,
    diet_quality_delta_ppm: i64,
    recovery_rate_delta_ppm_per_tick: i64,
    reserve_recovered: bool,
    recovery: DietRecoveryReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DietRecoveryReview {
    pub(super) policy_sensitive: bool,
    pub(super) natural_policy: DietProvisioningPolicy,
    pub(super) decision_recovery_ppm_per_tick: u32,
    pub(super) maximum_recovery_ppm_per_tick: u32,
    pub(super) available_category_count: usize,
    pub(super) authored_category_count: usize,
    pub(super) deprivation_ticks: u64,
    pub(super) provisioning_horizon_ticks: u64,
    pub(super) compact_provisioning_ticks: u64,
    pub(super) balanced_provisioning_ticks: u64,
    pub(super) observation_ticks: u64,
    pub(super) vitality_before_ppm: u32,
    pub(super) compact_vitality_after_ppm: u32,
    pub(super) balanced_vitality_after_ppm: u32,
    pub(super) realized_vitality_delta_ppm: i64,
    pub(super) compact_diet_quality_ppm: u32,
    pub(super) balanced_diet_quality_ppm: u32,
    pub(super) compact_recovery_window_delta_ppm: i64,
    pub(super) balanced_recovery_window_delta_ppm: i64,
    pub(super) compact_meal_actions: u64,
    pub(super) compact_drink_actions: u64,
    pub(super) balanced_meal_actions: u64,
    pub(super) balanced_drink_actions: u64,
    pub(super) fire_warmup_ticks: u64,
    pub(super) fire_fuel_mass_mg: u64,
    pub(super) compact_fire_refuels: u64,
    pub(super) balanced_fire_refuels: u64,
    pub(super) compact_min_thermal_comfort_ppm: u32,
    pub(super) balanced_min_thermal_comfort_ppm: u32,
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "focused tests execute shared evaluation while report examples consume retained evidence"
    )
)]
struct SurvivalProbeEvaluation {
    seed: u64,
    sample: &'static str,
    behavior_seed: u64,
    world: ProvisioningWorld,
    preservation_decision: preservation_decision::PreservationDecisionReview,
    work_pressure: work_pressure::SurvivalWorkPressureReview,
    integrated_work: integrated_work::IntegratedSurvivalWorkReview,
    diet_comparison: DietComparisonReview,
}

fn evaluate_survival_provisioning_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> SurvivalProbeEvaluation {
    let seed = case.seed();
    let sample = case.role().label();
    let behavior_seed = case.required_behavior_seed("survival provisioning policy");
    let world = provisioning_world(registries, seed);
    let protected_food = world.foods[world.witness_index];
    let protected_reserve_mass = world.preserved_reserve_mass;
    let preservation_decision = evaluate_preservation_decision(
        registries,
        seed,
        behavior_seed,
        protected_food,
        protected_reserve_mass,
    );
    evaluate_survival_pressure_response_probe(registries, seed);
    let work_pressure = evaluate_survival_work_pressure_probe(registries, seed);
    let integrated_work = evaluate_integrated_survival_work_loop(registries, seed, behavior_seed);
    if case.role() == FocusedProbeRole::MaintainedCoverage
        && seed == SURVIVAL_CONTINUATION_COVERAGE_SEED
    {
        assert!(
            integrated_work.followup_prospecting_triggered
                && integrated_work.followup_found_continuation,
            "survival continuation witness must turn acquired local evidence into a productive neighboring survey"
        );
        assert!(
            integrated_work.power_triggered_by_observation
                && integrated_work.manual_power_ticks > 0
                && integrated_work.stored_work_nj > 0,
            "survival continuation witness must turn the newly observed continuation into stored-work preparation"
        );
    }

    let diet_comparison = evaluate_provisioning_comparison(registries, behavior_seed, &world);
    SurvivalProbeEvaluation {
        seed,
        sample,
        behavior_seed,
        world,
        preservation_decision,
        work_pressure,
        integrated_work,
        diet_comparison,
    }
}

#[cfg(test)]
pub(super) fn run_survival_provisioning_probe(registries: &Registries, case: FocusedProbeCase) {
    let _ = evaluate_survival_provisioning_probe(registries, case);
}

struct DietRecoveryBranch<'a> {
    prepared: &'a AppState,
    food_store: StockpileId,
    campfire: EnergyStoreId,
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
        let available = observed_provisioning_foods(registries, &state, branch.food_store);
        let available_foods = available
            .iter()
            .map(|observed| observed.food)
            .collect::<Vec<_>>();
        let selected_indices = selected_food_indices(&available_foods, policy);
        assert!(
            !selected_indices.is_empty(),
            "diet-recovery actor found no fresh edible food in the current local stockpile"
        );
        let selected_masses =
            desired_policy_meal_masses(registries, &state, &available_foods, &selected_indices);
        let portions = selected_indices
            .iter()
            .zip(selected_masses)
            .map(|(index, desired)| {
                let observed = &available[*index];
                FoodStackPortion::new(&observed.stack, desired.min(observed.stack.mass()))
            })
            .collect::<Vec<_>>();
        let offered = portions
            .iter()
            .try_fold(Mass::ZERO, |total, portion| {
                total.checked_add(portion.mass())
            })
            .unwrap_or_else(|| panic!("diet-recovery serving mass overflowed"));
        assert!(
            offered >= physiology.direct_consumption().minimum_meal_mass(),
            "diet-recovery finite food supply cannot provide another legal serving"
        );
        let meal = validate_eat_stack_portions(registries, &state, &portions)
            .unwrap_or_else(|error| {
                panic!("diet-recovery visible-stack meal validation failed: {error}")
            })
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
        let drink_store = observed_provisioning_drink(registries, &state).store();
        let validated = match validate_drink_store_to_hydration_target(
            registries,
            &state,
            drink_store,
            hydration_target,
        ) {
            Ok(Some(drink)) => drink,
            Ok(None) => unreachable!("thirst threshold was checked as unmet"),
            Err(deep_hearth::survival::DrinkStoreToTargetError::Projection(
                deep_hearth::survival::DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            )) => validate_drink(registries, &state, drink_store, maximum_drink_volume)
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
    fire_refuels: u64,
    minimum_thermal_comfort_ppm: u32,
}

fn try_refuel_diet_recovery_fire(
    registries: &Registries,
    state: &mut AppState,
    campfire: EnergyStoreId,
    fuel_store: StockpileId,
) -> bool {
    let record = state
        .energy()
        .get_store(campfire)
        .unwrap_or_else(|| panic!("diet-recovery campfire disappeared"));
    let definition = registries
        .energy()
        .get_store(record.definition())
        .unwrap_or_else(|| panic!("diet-recovery campfire definition disappeared"));
    let can_fit = record
        .stored()
        .checked_add(CAMPFIRE_FUEL_THERMAL_ENERGY)
        .is_some_and(|stored| stored <= definition.capacity());
    let fuel_available = state
        .inventory()
        .get_stockpile(fuel_store)
        .is_some_and(|stockpile| {
            stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)) >= CAMPFIRE_FUEL_MASS
        });
    if !can_fit || !fuel_available {
        return false;
    }
    let released = validate_combust_fuel(registries, state, campfire, fuel_store)
        .unwrap_or_else(|error| panic!("diet-recovery campfire refuel failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("diet-recovery campfire refuel commit failed: {error}"));
    assert_eq!(released, CAMPFIRE_FUEL_THERMAL_ENERGY);
    true
}

fn advance_diet_recovery_with_fire(
    registries: &Registries,
    state: &mut AppState,
    campfire: EnergyStoreId,
    fuel_store: StockpileId,
    ticks: u64,
    context: &'static str,
) -> (u64, u32) {
    let warning = registries
        .survival()
        .physiology()
        .thermal_comfort()
        .warning_below_ppm();
    let mut refuels = 0_u64;
    let mut minimum_comfort = u32::MAX;
    for _ in 0..ticks {
        refuels += u64::from(try_refuel_diet_recovery_fire(
            registries, state, campfire, fuel_store,
        ));
        advance_idle_ticks(registries, state, 1, context);
        let comfort = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("diet-recovery player disappeared during warm recovery"))
            .thermal_comfort_ppm();
        minimum_comfort = minimum_comfort.min(comfort);
        assert!(
            comfort >= warning,
            "diet-recovery warm recovery dropped below the authored comfort threshold"
        );
    }
    (refuels, minimum_comfort)
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
    let (matched_refuels, matched_minimum_comfort) = advance_diet_recovery_with_fire(
        registries,
        &mut provisioned.state,
        branch.campfire,
        branch.food_store,
        comparison_horizon_ticks - provisioned.elapsed_ticks,
        "diet-recovery matched horizon",
    );
    let restored = assess_survival(registries, &provisioned.state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared at matched observation start"));
    let diet_quality_ppm = restored.diet_quality_ppm();
    let (observation_refuels, observation_minimum_comfort) = advance_diet_recovery_with_fire(
        registries,
        &mut provisioned.state,
        branch.campfire,
        branch.food_store,
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
        fire_refuels: matched_refuels + observation_refuels,
        minimum_thermal_comfort_ppm: matched_minimum_comfort.min(observation_minimum_comfort),
    }
}

pub(super) fn evaluate_diet_recovery_consequence(
    registries: &Registries,
    world: &ProvisioningWorld,
) -> DietRecoveryReview {
    let mut state = AppState::new();
    let physiology = registries.survival().physiology();
    let offered_masses = world
        .foods
        .iter()
        .map(|food| mass_for_target_energy(*food, physiology.maximum_metabolic_energy()))
        .collect::<Vec<_>>();
    let campfire_stone_mass = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .and_then(|definition| definition.assembly_profile())
        .and_then(|profile| profile.inputs().first())
        .map(|input| {
            assert_eq!(
                input.commodity(),
                CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
            );
            input.mass()
        })
        .unwrap_or_else(|| panic!("diet-recovery campfire stone input disappeared"));
    let fire_fuel_mass = Mass::from_milligrams(
        CAMPFIRE_FUEL_MASS
            .milligrams()
            .checked_mul(DIET_RECOVERY_FIRE_FUEL_CHARGES)
            .unwrap_or_else(|| panic!("diet-recovery fire fuel mass overflowed")),
    );
    let food_capacity = offered_masses
        .iter()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(*mass))
        .and_then(|total| total.checked_add(campfire_stone_mass))
        .and_then(|total| total.checked_add(fire_fuel_mass))
        .unwrap_or_else(|| panic!("diet-recovery food capacity overflowed"));
    let food_store = seed_stockpile(
        &mut state,
        food_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for (food, mass) in world.foods.iter().zip(&offered_masses) {
        seed_lot(
            registries,
            &mut state,
            food_store,
            food.commodity(),
            *mass,
            ROOM_TEMPERATURE,
        );
    }
    seed_lot(
        registries,
        &mut state,
        food_store,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        campfire_stone_mass,
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        food_store,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        fire_fuel_mass,
        ROOM_TEMPERATURE,
    );
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

    // The deprivation episode intentionally uses cold exposure to create a real vitality deficit.
    // Recovery is a different decision state: the actor establishes a finite local fire through
    // ordinary material-backed assembly and combustion, then warms back above the authored
    // recovery threshold before either diet policy branches.
    let campfire =
        validate_assemble_energy_store(registries, &state, ENERGY_STONE_RING_CAMPFIRE, food_store)
            .unwrap_or_else(|error| panic!("diet-recovery campfire assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("diet-recovery campfire assembly commit failed: {error}")
            });
    assert!(try_refuel_diet_recovery_fire(
        registries, &mut state, campfire, food_store,
    ));
    assert!(try_refuel_diet_recovery_fire(
        registries, &mut state, campfire, food_store,
    ));
    let thermal_warning = physiology.thermal_comfort().warning_below_ppm();
    let mut fire_warmup_ticks = 0_u64;
    while assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared during fire warmup"))
        .thermal_comfort_ppm()
        < thermal_warning
    {
        advance_idle_ticks(registries, &mut state, 1, "diet-recovery fire warmup");
        fire_warmup_ticks += 1;
        assert!(
            fire_warmup_ticks < 100,
            "diet-recovery campfire failed to restore recovery-capable warmth"
        );
    }

    let recovery_decision = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared at warm decision point"));
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let available_category_count = world
        .foods
        .iter()
        .map(|food| food.category())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let maximum_recovery_ppm_per_tick = physiology.nutrition().vitality_recovery_ppm_per_tick();
    let decision_recovery_ppm_per_tick =
        recovery_decision.diet_supported_vitality_recovery_ppm_per_tick();
    let natural_policy = diet_provisioning_policy_from_visible_recovery(
        decision_recovery_ppm_per_tick,
        maximum_recovery_ppm_per_tick,
        available_category_count == authored_category_count,
    );

    let matter_total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("diet-recovery initial matter audit failed: {error}"))
        .total();
    let fluid_total = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("diet-recovery initial fluid audit failed: {error}"))
        .total();
    let branch = DietRecoveryBranch {
        prepared: &state,
        food_store,
        campfire,
        matter_total,
        fluid_total,
    };
    // Nutrition is a recovery lever rather than an immediate consumption bonus. One authored hour
    // is long enough to expose accumulated recovery while keeping the actor's finite campfire
    // management visible instead of hiding the comparison inside a multi-hour idle fast-forward.
    let observation_ticks = (registries.core().calendar().ticks_per_day() / 24).max(1);
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
    let compact_provisioning_ticks = compact_provisioned.elapsed_ticks;
    let balanced_provisioning_ticks = balanced_provisioned.elapsed_ticks;
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
    // Classify the counterfactual from what the two actors actually did and experienced, not from
    // the category set they intended to use. Finite supply, serving limits, and integer rounding
    // can legitimately collapse distinct plans to the same canonical actions and consequences.
    let policy_sensitive = compact_provisioning_ticks != balanced_provisioning_ticks
        || compact_meal_actions != balanced_meal_actions
        || compact_drink_actions != balanced_drink_actions
        || compact_observation.diet_quality_ppm != balanced_observation.diet_quality_ppm
        || compact_observation.vitality_after_ppm != balanced_observation.vitality_after_ppm
        || compact_observation.vitality_window_delta_ppm
            != balanced_observation.vitality_window_delta_ppm;
    let realized_vitality_delta_ppm = i64::from(balanced_observation.vitality_after_ppm)
        - i64::from(compact_observation.vitality_after_ppm);
    DietRecoveryReview {
        policy_sensitive,
        natural_policy,
        decision_recovery_ppm_per_tick,
        maximum_recovery_ppm_per_tick,
        available_category_count,
        authored_category_count,
        deprivation_ticks,
        provisioning_horizon_ticks: comparison_horizon_ticks,
        compact_provisioning_ticks,
        balanced_provisioning_ticks,
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
        fire_warmup_ticks,
        fire_fuel_mass_mg: fire_fuel_mass.milligrams(),
        compact_fire_refuels: compact_observation.fire_refuels,
        balanced_fire_refuels: balanced_observation.fire_refuels,
        compact_min_thermal_comfort_ppm: compact_observation.minimum_thermal_comfort_ppm,
        balanced_min_thermal_comfort_ppm: balanced_observation.minimum_thermal_comfort_ppm,
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
