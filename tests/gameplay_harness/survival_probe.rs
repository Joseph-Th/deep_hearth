//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{
    GeologicalDepositSeed, seed_fluid_store, seed_geological_deposit, seed_lot,
    seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary, seed_preexisting_world_age, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, FORM_ORE, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER,
};
use deep_hearth::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Pressure, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{SimulationTick, WorldSeed};
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
use deep_hearth::material::{CommodityKey, MaterialComposition};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{
    DrinkDefinition, DrinkHydrationProjectionError, DrinkOutcome, EatOutcome, FoodCategory,
    FoodDefinition, FoodFreshness, assess_food_freshness, assess_survival,
    initialize_player_survival, project_food_freshness_after_storage_transition,
    project_minimum_drink_to_hydration_target, validate_drink, validate_eat,
};

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
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
mod provisioning_support;
use provisioning_support::*;

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
const DIET_RECOVERY_OBSERVATION_TICKS: u64 = 1_000;

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
    vitality_advantage_ppm: u32,
    compact_diet_quality_ppm: u32,
    balanced_diet_quality_ppm: u32,
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
            vitality_advantage_ppm: 0,
            compact_diet_quality_ppm: 0,
            balanced_diet_quality_ppm: 0,
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
    drink: DrinkDefinition,
    drink_store: FluidStoreId,
    matter_total: AggregateMass,
    fluid_total: AggregateVolume,
}

fn run_diet_recovery_branch(
    registries: &Registries,
    branch: &DietRecoveryBranch<'_>,
    policy: DietProvisioningPolicy,
    comparison_horizon_ticks: u64,
) -> (u32, u32) {
    let mut state = branch.prepared.clone();
    let mut provisioning_elapsed_ticks = 0_u64;
    let physiology = registries.survival().physiology();
    let before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared before provisioning"));
    let selected_indices = selected_food_indices(branch.foods, policy);
    let energy_deficit = physiology
        .maximum_metabolic_energy()
        .checked_sub(before.metabolic_energy())
        .unwrap_or_else(|| panic!("diet-recovery metabolic reserve exceeded authored maximum"));
    let per_category_target = Energy::from_nanojoules(
        energy_deficit
            .nanojoules()
            .div_ceil(selected_indices.len() as u128)
            .max(1),
    );
    let desired_masses = selected_indices
        .iter()
        .map(|index| mass_for_target_energy(branch.foods[*index], per_category_target))
        .collect::<Vec<_>>();
    let selected_masses = bound_meal_masses_to_direct_limit(
        &desired_masses,
        physiology.direct_consumption().maximum_meal_mass(),
    );
    let selections = selected_indices
        .iter()
        .zip(&selected_masses)
        .map(|(index, mass)| MaterialLotSelection::new(branch.food_lots[*index], *mass))
        .collect::<Vec<_>>();
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

    let after_meal = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared after meal"));
    let drink_volume = recovery_drink_volume(
        registries,
        branch.drink,
        after_meal.hydration(),
        "diet-recovery",
    );
    if !drink_volume.is_zero() {
        let drink = validate_drink(registries, &state, branch.drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("diet-recovery drink validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("diet-recovery drink commit failed: {error}"));
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                drink.completes_at(),
            ))
            .unwrap_or_else(|| panic!("diet-recovery provisioning duration overflowed"));
    }
    let provisioned = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared after provisioning"));
    assert!(
        provisioned.metabolic_energy() >= physiology.hungry_below(),
        "one legal diet-recovery meal must lift metabolic energy above the hunger threshold before observing vitality recovery"
    );
    assert!(
        provisioned.hydration() >= physiology.thirsty_below(),
        "one legal diet-recovery drink must lift hydration above the thirst threshold before observing vitality recovery"
    );
    assert!(
        provisioning_elapsed_ticks <= comparison_horizon_ticks,
        "diet-recovery branch exceeded the policy-independent comparison horizon"
    );
    advance_idle_ticks(
        registries,
        &mut state,
        comparison_horizon_ticks - provisioning_elapsed_ticks,
        "diet-recovery matched horizon",
    );
    let restored = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared at matched observation start"));
    let diet_quality_ppm = restored.diet_quality_ppm();
    advance_idle_ticks(
        registries,
        &mut state,
        DIET_RECOVERY_OBSERVATION_TICKS,
        "diet-recovery observation",
    );
    let recovered = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("diet-recovery player disappeared during recovery"));
    assert!(
        recovered.vitality() > restored.vitality(),
        "fed and hydrated player must regain real vitality during the diet-recovery observation window"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("diet-recovery matter audit failed: {error}"))
            .total(),
        branch.matter_total,
        "diet-recovery eating must conserve represented matter"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("diet-recovery fluid audit failed: {error}"))
            .total(),
        branch.fluid_total,
        "diet-recovery drinking must conserve represented fluid"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("diet-recovery persistence audit failed: {error}"));
    (diet_quality_ppm, recovered.vitality().parts_per_million())
}

fn evaluate_diet_recovery_consequence(
    registries: &Registries,
    seed: u64,
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

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x4449_4554_5F52_4543));
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
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("diet-recovery player initialization failed: {error}"));
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
        drink: world.drink,
        drink_store,
        matter_total,
        fluid_total,
    };
    let direct = physiology.direct_consumption();
    let comparison_horizon_ticks = direct
        .meal_duration(direct.maximum_meal_mass())
        .unwrap_or_else(|| panic!("authored maximum meal must have a direct-consumption duration"))
        .value()
        .checked_add(
            direct
                .drink_duration(direct.maximum_drink_volume())
                .unwrap_or_else(|| {
                    panic!("authored maximum drink must have a direct-consumption duration")
                })
                .value(),
        )
        .unwrap_or_else(|| panic!("diet-recovery comparison horizon overflowed"));

    let (compact_diet_quality_ppm, compact_vitality_after_ppm) = run_diet_recovery_branch(
        registries,
        &branch,
        DietProvisioningPolicy::CompactCalories,
        comparison_horizon_ticks,
    );
    let (balanced_diet_quality_ppm, balanced_vitality_after_ppm) = run_diet_recovery_branch(
        registries,
        &branch,
        DietProvisioningPolicy::BalancedRecovery,
        comparison_horizon_ticks,
    );
    assert!(
        balanced_diet_quality_ppm > compact_diet_quality_ppm,
        "all-category provisioning must create stronger diet quality in the real recovery challenge"
    );
    assert!(
        balanced_vitality_after_ppm > compact_vitality_after_ppm,
        "balanced provisioning must produce more actual vitality recovery over the same physical horizon"
    );
    DietRecoveryReview {
        actionable: true,
        deprivation_ticks,
        provisioning_horizon_ticks: comparison_horizon_ticks,
        observation_ticks: DIET_RECOVERY_OBSERVATION_TICKS,
        vitality_before_ppm,
        compact_vitality_after_ppm,
        balanced_vitality_after_ppm,
        vitality_advantage_ppm: balanced_vitality_after_ppm - compact_vitality_after_ppm,
        compact_diet_quality_ppm,
        balanced_diet_quality_ppm,
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
