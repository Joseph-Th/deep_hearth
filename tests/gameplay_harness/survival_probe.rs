//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{
    seed_fluid_store, seed_lot, seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary, seed_preexisting_world_age, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER,
};
use deep_hearth::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Volume};
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
    ManualPowerRequest, PlayerWork, ProspectingMethodId, validate_start_manual_power,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{
    DrinkDefinition, FoodCategory, FoodDefinition, FoodFreshness, assess_food_freshness,
    assess_survival, initialize_player_survival, project_food_freshness_after_storage_transition,
    validate_drink, validate_eat,
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
pub(super) use preservation::{
    PreservationInvestmentPolicy, preservation_freshness_return_threshold_ppm,
    preservation_storage_definition_for_policy_and_capacity,
};
use preservation::{preservation_candidate_for_policy, preservation_candidates};

#[path = "survival_probe/preservation_evaluation.rs"]
pub(super) mod preservation_evaluation;
use preservation_evaluation::{
    evaluate_preservation_infrastructure_definition, evaluate_preservation_infrastructure_probe,
    project_preservation_candidates, select_preservation_projection,
};

const DIET_RECOVERY_TARGET_VITALITY_PPM: u32 = 950_000;
const DIET_RECOVERY_OBSERVATION_TICKS: u64 = 1_000;

fn mass_for_target_energy(food: FoodDefinition, target: Energy) -> Mass {
    food.minimum_mass_for_dietary_energy(target)
        .unwrap_or_else(|| panic!("survival probe meal mass exceeds authoritative range"))
}

fn finish_direct_consumption(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("survival direct consumption has no active player work"));
    assert!(matches!(
        active,
        PlayerWork::Eating { .. } | PlayerWork::Drinking { .. }
    ));
    let ticks = completes_at
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("direct-consumption completion precedes current tick"));
    assert!(ticks > 0, "direct-consumption attention must occupy time");
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival direct-consumption tick failed: {error}"));
        assert!(
            outcome.production_availability_changes().is_empty()
                && outcome.production_completions().is_empty()
                && outcome.ready_mining_jobs().is_empty()
                && outcome.manual_power().is_none()
                && outcome.field_prospecting().is_none(),
            "survival direct consumption crossed an unrelated observable runtime event"
        );
        if elapsed < ticks {
            assert_eq!(state.player_work().active(), Some(active));
        } else {
            assert_eq!(state.player_work().active(), None);
        }
    }
    ticks
}

fn bound_meal_masses_to_direct_limit(masses: &[Mass], maximum: Mass) -> Vec<Mass> {
    assert!(!masses.is_empty(), "survival meal plan must select food");
    let total_milligrams = masses
        .iter()
        .try_fold(0_u128, |total, mass| {
            total.checked_add(u128::from(mass.milligrams()))
        })
        .unwrap_or_else(|| panic!("survival meal plan mass overflowed"));
    let maximum_milligrams = maximum.milligrams();
    if total_milligrams <= u128::from(maximum_milligrams) {
        return masses.to_vec();
    }

    let selected_count = u64::try_from(masses.len())
        .unwrap_or_else(|_| panic!("survival meal selection count exceeds u64"));
    assert!(
        maximum_milligrams >= selected_count,
        "authored direct meal limit must permit at least one milligram per selected category"
    );
    let mut allocated_milligrams = 0_u64;
    let mut bounded = Vec::with_capacity(masses.len());
    for (index, mass) in masses.iter().enumerate() {
        let remaining_slots = masses.len() - index - 1;
        let take = if remaining_slots == 0 {
            maximum_milligrams
                .checked_sub(allocated_milligrams)
                .unwrap_or_else(|| panic!("survival bounded meal allocation underflowed"))
        } else {
            let proportional = u128::from(mass.milligrams())
                .checked_mul(u128::from(maximum_milligrams))
                .unwrap_or_else(|| panic!("survival bounded meal scaling overflowed"))
                / total_milligrams;
            let proportional = u64::try_from(proportional)
                .unwrap_or_else(|_| panic!("survival bounded meal portion exceeds u64"))
                .max(1);
            let reserved_for_remaining = u64::try_from(remaining_slots)
                .unwrap_or_else(|_| panic!("survival meal selection count exceeds u64"));
            let maximum_here = maximum_milligrams
                .checked_sub(allocated_milligrams)
                .and_then(|remaining| remaining.checked_sub(reserved_for_remaining))
                .unwrap_or_else(|| panic!("survival bounded meal allocation exhausted early"));
            proportional.min(maximum_here)
        };
        allocated_milligrams = allocated_milligrams
            .checked_add(take)
            .unwrap_or_else(|| panic!("survival bounded meal allocation overflowed"));
        bounded.push(Mass::from_milligrams(take));
    }
    assert_eq!(allocated_milligrams, maximum_milligrams);
    bounded
}

fn food_category_count(foods: &[FoodDefinition]) -> usize {
    foods
        .iter()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len()
}

fn food_option_summary(registries: &Registries, foods: &[FoodDefinition]) -> String {
    foods
        .iter()
        .map(|food| {
            let material = registries
                .materials()
                .get_material(food.commodity().material())
                .unwrap_or_else(|| unreachable!("validated food option has a material"));
            format!(
                "{}:{}:{:?}:{}nJ/mg:{}uL/mg:{}t",
                food.commodity().value(),
                material.name(),
                food.category(),
                food.dietary_energy().nanojoules_per_milligram(),
                food.hydration_microliters_per_milligram(),
                food.shelf_life().value(),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

const fn category_salt(category: FoodCategory) -> u64 {
    match category {
        FoodCategory::Grain => 0x4752_4149_4E00_0001,
        FoodCategory::Fruit => 0x4652_5549_5400_0002,
        FoodCategory::Protein => 0x5052_4F54_4549_4E03,
    }
}

fn normalized_deficit_priority(
    energy_deficit_ppm: u32,
    hydration_deficit_ppm: u32,
) -> ProvisioningPriority {
    match hydration_deficit_ppm.cmp(&energy_deficit_ppm) {
        std::cmp::Ordering::Greater => ProvisioningPriority::Hydration,
        std::cmp::Ordering::Less => ProvisioningPriority::MetabolicEnergy,
        std::cmp::Ordering::Equal => ProvisioningPriority::Balanced,
    }
}

fn provisioning_drink_supply(registries: &Registries, world: &ProvisioningWorld) -> Volume {
    // Provision the world with enough finite drink to recover from any legal player reserve state.
    // The acting plan below sizes the actual drink from the authoritative decision-point assessment,
    // so setup does not need to predict passive or exertion losses.
    world
        .drink
        .minimum_volume_for_hydration(registries.survival().physiology().maximum_hydration())
        .unwrap_or_else(|| panic!("survival probe drink supply exceeds authoritative range"))
}

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

fn selected_food_indices(foods: &[FoodDefinition], policy: DietProvisioningPolicy) -> Vec<usize> {
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
    let hydration_deficit = physiology
        .maximum_hydration()
        .checked_sub(after_meal.hydration())
        .unwrap_or_else(|| panic!("diet-recovery hydration exceeded authored maximum"));
    if !hydration_deficit.is_zero() {
        let required_drink_volume = branch
            .drink
            .minimum_volume_for_hydration(hydration_deficit)
            .unwrap_or_else(|| panic!("diet-recovery drink volume exceeds authoritative range"));
        let drink_volume =
            required_drink_volume.min(physiology.direct_consumption().maximum_drink_volume());
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

pub(super) struct ProvisioningWorld {
    pub(super) start_profile: SurvivalStartProfile,
    pub(super) foods: Vec<FoodDefinition>,
    pub(super) offered_masses: Vec<Mass>,
    pub(super) witness_index: usize,
    pub(super) preserved_reserve_mass: Mass,
    pub(super) inherited_preservation_definition: StorageDefinitionId,
    pub(super) inherited_preservation_multiplier_ppm: u32,
    pub(super) age_ticks: u64,
    pub(super) provisioning_wait_ticks: u64,
    drink: DrinkDefinition,
}

pub(super) fn provisioning_world(registries: &Registries, seed: u64) -> ProvisioningWorld {
    let physiology = registries.survival().physiology();
    let mut foods_by_category = BTreeMap::<FoodCategory, Vec<FoodDefinition>>::new();
    for food in registries.survival().foods().copied() {
        foods_by_category
            .entry(food.category())
            .or_default()
            .push(food);
    }
    for options in foods_by_category.values_mut() {
        options.sort_by_key(|food| food.commodity());
    }
    assert!(
        !foods_by_category.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored edible food"
    );
    let mut foods = foods_by_category
        .iter()
        .enumerate()
        .map(|(index, (category, options))| {
            let choice = usize::try_from(
                mix64(seed ^ category_salt(*category) ^ index as u64) % options.len() as u64,
            )
            .unwrap_or_else(|_| unreachable!("food option index fits usize"));
            options[choice]
        })
        .collect::<Vec<_>>();
    let available_count = if foods.len() <= 2 {
        foods.len()
    } else {
        2 + usize::try_from(mix64(seed ^ 0x4341_5445_474F_5259) % (foods.len() - 1) as u64)
            .unwrap_or_else(|_| unreachable!("bounded survival category count fits usize"))
    };
    let rotation = usize::try_from(mix64(seed ^ 0x464F_4F44_5F52_4F54) % foods.len() as u64)
        .unwrap_or_else(|_| unreachable!("survival food rotation fits usize"));
    foods.rotate_left(rotation);
    foods.truncate(available_count);
    foods.sort_by_key(|food| food.category());
    let start_profile = match mix64(seed ^ 0x5354_4152_5450_5246) % 3 {
        0 => SurvivalStartProfile::FullReserve,
        1 => SurvivalStartProfile::HungerWarningBoundary,
        _ => SurvivalStartProfile::HydrationWarningBoundary,
    };
    let compact_indices = selected_food_indices(&foods, DietProvisioningPolicy::CompactCalories);
    let balanced_indices = selected_food_indices(&foods, DietProvisioningPolicy::BalancedRecovery);
    let maximum_absorbed_energy = physiology.maximum_metabolic_energy().nanojoules();
    let compact_target = Energy::from_nanojoules(
        maximum_absorbed_energy
            .div_ceil(compact_indices.len() as u128)
            .max(1),
    );
    let balanced_target = Energy::from_nanojoules(
        maximum_absorbed_energy
            .div_ceil(balanced_indices.len() as u128)
            .max(1),
    );
    let supply_margin_ppm = 1_000_000 + (mix64(seed ^ 0x5355_5050_4C59_4D47) % 300_001) as u32;
    let offered_masses = foods
        .iter()
        .enumerate()
        .map(|(index, food)| {
            let balanced = mass_for_target_energy(*food, balanced_target);
            let required = if compact_indices.contains(&index) {
                balanced.max(mass_for_target_energy(*food, compact_target))
            } else {
                balanced
            };
            let scaled = u128::from(required.milligrams())
                .checked_mul(u128::from(supply_margin_ppm))
                .map(|value| value.div_ceil(1_000_000))
                .unwrap_or_else(|| panic!("survival offered-food margin overflowed"));
            Mass::from_milligrams(
                u64::try_from(scaled)
                    .unwrap_or_else(|_| panic!("survival offered-food mass exceeds range")),
            )
        })
        .collect::<Vec<_>>();
    let preserving_storage = preservation_candidates(registries);
    let maximum_preservation_capacity = preserving_storage
        .iter()
        .map(|candidate| candidate.capacity)
        .max()
        .unwrap_or_else(|| unreachable!("preservation candidates are nonempty"));
    let witness_options = compact_indices
        .iter()
        .copied()
        .filter(|witness_index| offered_masses[*witness_index] <= maximum_preservation_capacity)
        .collect::<Vec<_>>();
    assert!(
        !witness_options.is_empty(),
        "no authored preservation enclosure can hold any generated compact-calorie reserve parcel"
    );
    let witness_option_index =
        usize::try_from(mix64(seed ^ 0x5052_4553_5749_544E) % witness_options.len() as u64)
            .unwrap_or_else(|_| {
                unreachable!("bounded preservation witness option index fits usize")
            });
    let witness_index = witness_options[witness_option_index];
    let witness_food = foods[witness_index];
    let minimum_reserve_mass = offered_masses[witness_index];
    // Reserve demand is a world need, not a property of whichever container happens to exist.
    // Generate several meal-equivalents first, then choose inherited storage from the authored
    // enclosures capable of holding that reserve. This keeps capacity strategically relevant without
    // coupling the desired stockpile size to container identity.
    let reserve_servings = 4 + mix64(seed ^ 0x5052_4553_5253_5256) % 21;
    let requested_reserve_mg = minimum_reserve_mass
        .milligrams()
        .checked_mul(reserve_servings)
        .unwrap_or_else(|| panic!("survival preserved-reserve demand overflowed"));
    let preserved_reserve_mass =
        Mass::from_milligrams(requested_reserve_mg.min(maximum_preservation_capacity.milligrams()));
    let inherited_options = preserving_storage
        .iter()
        .filter(|candidate| candidate.capacity >= preserved_reserve_mass)
        .collect::<Vec<_>>();
    assert!(
        !inherited_options.is_empty(),
        "generated preserved reserve has no authored enclosure capacity"
    );
    let inherited_index =
        usize::try_from(mix64(seed ^ 0x494E_4845_5249_5445) % inherited_options.len() as u64)
            .unwrap_or_else(|_| unreachable!("bounded inherited-preservation index fits usize"));
    let inherited_preservation = inherited_options[inherited_index];
    let inherited_preservation_multiplier_ppm = inherited_preservation.preservation_multiplier_ppm;
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let provisioning_wait_ticks = match start_profile {
        SurvivalStartProfile::FullReserve => {
            let base = ticks_per_day
                .checked_mul(2)
                .map(|ticks| ticks / 3)
                .unwrap_or_else(|| panic!("survival probe provisioning horizon overflowed"));
            let jitter = (ticks_per_day / 12).max(1);
            base.checked_add(mix64(seed ^ 0x4441_5946_5241_4354) % jitter)
                .unwrap_or_else(|| panic!("survival probe provisioning wait overflowed"))
        }
        SurvivalStartProfile::HungerWarningBoundary
        | SurvivalStartProfile::HydrationWarningBoundary => {
            let base = (ticks_per_day / 24).max(1);
            base.checked_add(mix64(seed ^ 0x5052_4553_5355_5245) % base)
                .unwrap_or_else(|| panic!("survival pressure-world wait overflowed"))
        }
    };
    let age_limit = (witness_food.shelf_life().value() / 4)
        .max(1)
        .min(provisioning_wait_ticks.saturating_sub(1).max(1));
    let age_ticks = (256 + mix64(seed ^ 0x4147_455F_464F_4F44) % 512).min(age_limit);
    assert!(provisioning_wait_ticks > age_ticks);
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    assert!(
        !drinks.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored drinkable fluid"
    );
    let drink_index = usize::try_from(mix64(seed ^ 0x4452_494E_4B00_0001) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("drink index fits usize"));

    ProvisioningWorld {
        start_profile,
        foods,
        offered_masses,
        witness_index,
        preserved_reserve_mass,
        inherited_preservation_definition: inherited_preservation.definition,
        inherited_preservation_multiplier_ppm,
        age_ticks,
        provisioning_wait_ticks,
        drink: drinks[drink_index],
    }
}

struct ProvisioningPlan {
    selected_indices: Vec<usize>,
    selected_masses: Vec<Mass>,
    drink_volume: Volume,
}

fn provisioning_plan_duration(registries: &Registries, plan: &ProvisioningPlan) -> u64 {
    let direct = registries.survival().physiology().direct_consumption();
    let meal_mass = plan
        .selected_masses
        .iter()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("survival provisioning plan meal mass overflowed"));
    let meal_ticks = direct
        .meal_duration(meal_mass)
        .unwrap_or_else(|| panic!("survival provisioning plan has an invalid direct meal mass"))
        .value();
    let drink_ticks = if plan.drink_volume.is_zero() {
        0
    } else {
        direct
            .drink_duration(plan.drink_volume)
            .unwrap_or_else(|| {
                panic!("survival provisioning plan has an invalid direct drink volume")
            })
            .value()
    };
    meal_ticks
        .checked_add(drink_ticks)
        .unwrap_or_else(|| panic!("survival provisioning plan duration overflowed"))
}

fn provisioning_plan(
    registries: &Registries,
    world: &ProvisioningWorld,
    prepared: &PreparedProvisioningWorld,
    policy: DietProvisioningPolicy,
) -> ProvisioningPlan {
    let foods = world.foods.as_slice();
    let physiology = registries.survival().physiology();
    let before = assess_survival(registries, &prepared.state)
        .unwrap_or_else(|| panic!("survival provisioning plan lost the player"));
    let selected_indices = selected_food_indices(foods, policy);
    assert!(!selected_indices.is_empty());
    let energy_deficit = physiology
        .maximum_metabolic_energy()
        .checked_sub(before.metabolic_energy())
        .unwrap_or_else(|| panic!("survival provisioning energy exceeded authored maximum"));
    let category_target = Energy::from_nanojoules(
        energy_deficit
            .nanojoules()
            .div_ceil(selected_indices.len() as u128)
            .max(1),
    );
    let desired_masses = selected_indices
        .iter()
        .map(|index| mass_for_target_energy(foods[*index], category_target))
        .collect::<Vec<_>>();
    let selected_masses = bound_meal_masses_to_direct_limit(
        &desired_masses,
        physiology.direct_consumption().maximum_meal_mass(),
    );
    for (index, selected_mass) in selected_indices.iter().zip(&selected_masses) {
        assert!(
            *selected_mass <= world.offered_masses[*index],
            "survival probe offered food must cover every matched-policy portion"
        );
    }
    let offered_food_hydration = selected_indices
        .iter()
        .zip(&selected_masses)
        .try_fold(0_u128, |total, (index, mass)| {
            let contribution = u128::from(mass.milligrams()).checked_mul(u128::from(
                foods[*index].hydration_microliters_per_milligram(),
            ))?;
            total.checked_add(contribution)
        })
        .unwrap_or_else(|| panic!("survival probe food hydration overflowed"));
    let hydration_deficit = physiology
        .maximum_hydration()
        .checked_sub(before.hydration())
        .unwrap_or_else(|| panic!("survival provisioning hydration exceeded authored maximum"));
    let target_drink_gain = u64::try_from(
        u128::from(hydration_deficit.microliters()).saturating_sub(offered_food_hydration),
    )
    .unwrap_or_else(|_| panic!("survival probe hydration target exceeds represented range"));
    let drink_volume = if target_drink_gain == 0 {
        Volume::ZERO
    } else {
        let required = world
            .drink
            .minimum_volume_for_hydration(Volume::from_microliters(target_drink_gain))
            .unwrap_or_else(|| panic!("survival probe drink volume exceeds authoritative range"));
        required.min(physiology.direct_consumption().maximum_drink_volume())
    };
    ProvisioningPlan {
        selected_indices,
        selected_masses,
        drink_volume,
    }
}

struct PreparedProvisioningWorld {
    state: AppState,
    ambient_meal: StockpileId,
    prepared_lots: Vec<MaterialLotId>,
    preserved_witness: MaterialLotId,
    drink_store: FluidStoreId,
    ambient_age: u64,
    preserved_age: u64,
    preservation_age_saved_ticks: u64,
    matter_total: AggregateMass,
    fluid_total: AggregateVolume,
}

fn prepare_provisioning_world(
    registries: &Registries,
    seed: u64,
    world: &ProvisioningWorld,
    drink_supply: Volume,
) -> PreparedProvisioningWorld {
    let foods = world.foods.as_slice();
    let offered_masses = world.offered_masses.as_slice();
    let witness_food = foods[world.witness_index];
    let witness_mass = world.preserved_reserve_mass;
    let preservation_definition = registries
        .storage()
        .get(world.inherited_preservation_definition)
        .unwrap_or_else(|| panic!("survival provisioning references a missing storage definition"));
    assert_eq!(
        world.inherited_preservation_multiplier_ppm,
        preservation_definition
            .storage_profile()
            .preservation_multiplier_ppm(),
        "survival provisioning must report the inherited storage definition's actual preservation strength"
    );
    assert!(
        witness_mass <= preservation_definition.maximum_stockpile_capacity(),
        "controlled preserved reserve must fit inside the selected authored storage definition"
    );
    let ambient_capacity = offered_masses
        .iter()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("survival probe offered-food capacity overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    let ambient_meal = seed_stockpile(
        &mut state,
        ambient_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let preserved_reserve = seed_stockpile(
        &mut state,
        witness_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let enclosure_material = seed_stockpile(
        &mut state,
        preservation_definition.assembly_profile().input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in preservation_definition.assembly_profile().inputs() {
        seed_lot(
            registries,
            &mut state,
            enclosure_material,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_build_storage_enclosure(
        registries,
        &state,
        world.inherited_preservation_definition,
        preserved_reserve,
        enclosure_material,
    )
    .unwrap_or_else(|error| panic!("survival provisioning enclosure bootstrap failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("survival provisioning enclosure bootstrap commit failed: {error}")
    });
    assert_eq!(
        state
            .inventory()
            .get_stockpile(preserved_reserve)
            .map(|stockpile| stockpile.storage_profile()),
        Some(preservation_definition.storage_profile()),
        "preexisting preserved reserve must be backed by the selected physical enclosure"
    );
    let prepared_lots = foods
        .iter()
        .zip(offered_masses)
        .map(|(food, mass)| {
            seed_lot(
                registries,
                &mut state,
                ambient_meal,
                food.commodity(),
                *mass,
                ROOM_TEMPERATURE,
            )
        })
        .collect::<Vec<_>>();
    let ambient_witness = prepared_lots[world.witness_index];
    let preserved_witness = seed_lot(
        registries,
        &mut state,
        preserved_reserve,
        witness_food.commodity(),
        witness_mass,
        ROOM_TEMPERATURE,
    );
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        drink_supply,
        world.drink.fluid(),
        drink_supply,
        ROOM_TEMPERATURE,
    );

    // Actor admission follows all fixture-only mutations; canonical ticks create subsequent
    // survival pressure.
    match world.start_profile {
        SurvivalStartProfile::FullReserve => initialize_player_survival(registries, &mut state)
            .unwrap_or_else(|error| panic!("survival probe player initialization failed: {error}")),
        SurvivalStartProfile::HungerWarningBoundary => {
            seed_player_survival_at_hunger_warning_boundary(registries, &mut state)
        }
        SurvivalStartProfile::HydrationWarningBoundary => {
            seed_player_survival_at_hydration_warning_boundary(registries, &mut state)
        }
    }

    advance_idle_ticks(
        registries,
        &mut state,
        world.age_ticks,
        "provisioning world aging",
    );
    let ambient_age = fresh_age(registries, &state, ambient_witness);
    let preserved_age = fresh_age(registries, &state, preserved_witness);
    assert!(
        preserved_age < ambient_age,
        "authored preservation must slow future food spoilage relative to ambient storage"
    );
    let preservation_age_saved_ticks = ambient_age - preserved_age;
    advance_idle_ticks(
        registries,
        &mut state,
        world.provisioning_wait_ticks - world.age_ticks,
        "provisioning decision wait",
    );
    validate_loaded_state(registries, &state).unwrap_or_else(|error| {
        panic!("survival probe decision-point state audit failed: {error}")
    });
    let matter_total = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial matter audit failed: {error}"))
        .total();
    let fluid_total = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial fluid audit failed: {error}"))
        .total();

    PreparedProvisioningWorld {
        state,
        ambient_meal,
        prepared_lots,
        preserved_witness,
        drink_store,
        ambient_age,
        preserved_age,
        preservation_age_saved_ticks,
        matter_total,
        fluid_total,
    }
}

fn run_provisioning_case(
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
    let drink_volume = plan.drink_volume;
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
    let mut provisioning_elapsed_ticks = 0_u64;
    let mut drank_volume = Volume::ZERO;
    let mut hydration_offered = Volume::ZERO;
    let meal;
    let action_order;
    if drink_first && !drink_volume.is_zero() {
        let drank = validate_drink(registries, &state, prepared.drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("survival probe drinking validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
        drank_volume = drank.volume();
        hydration_offered = drank.hydration_offered();
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                drank.completes_at(),
            ))
            .unwrap_or_else(|| panic!("survival provisioning attention duration overflowed"));
        meal = validate_eat(registries, &state, prepared.ambient_meal, &selections)
            .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                meal.completes_at(),
            ))
            .unwrap_or_else(|| panic!("survival provisioning attention duration overflowed"));
        action_order = "drink->eat";
    } else {
        meal = validate_eat(registries, &state, prepared.ambient_meal, &selections)
            .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
        provisioning_elapsed_ticks = provisioning_elapsed_ticks
            .checked_add(finish_direct_consumption(
                registries,
                &mut state,
                meal.completes_at(),
            ))
            .unwrap_or_else(|| panic!("survival provisioning attention duration overflowed"));
        if drink_volume.is_zero() {
            action_order = "eat-only";
        } else {
            let drank = validate_drink(registries, &state, prepared.drink_store, drink_volume)
                .unwrap_or_else(|error| {
                    panic!("survival probe drinking validation failed: {error}")
                })
                .commit(&mut state)
                .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
            drank_volume = drank.volume();
            hydration_offered = drank.hydration_offered();
            provisioning_elapsed_ticks = provisioning_elapsed_ticks
                .checked_add(finish_direct_consumption(
                    registries,
                    &mut state,
                    drank.completes_at(),
                ))
                .unwrap_or_else(|| panic!("survival provisioning attention duration overflowed"));
            action_order = "eat->drink";
        }
    }
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
    if !drink_volume.is_zero() {
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
            "PLAYABLE SURVIVAL behavior=0x{behavior_seed:016X} mode=matched-policy policy={} catalog=registry-derived world-bootstrap=[reserve-profile:{},authored-food,authored-drink,storage-profile] player-present-from=t0 available-categories={available_categories} selected-categories={selected_categories} food-rotation=[witness:{} elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient-age:{ambient_age}t preserved-age:{preserved_age}t age-saved:{preservation_age_saved_ticks}t consume:older-ambient retain-preserved:{}mg] wait={provisioning_wait_ticks}t provisioning=[priority:{} action-order:{action_order}] meal=[mass:{}mg energy-offered:{}nJ nutrition-offered:{}ppm diet-quality:{}->{}ppm recovery-rate:{}->{}ppm/t] drink=[fluid:{} volume:{}uL hydration-offered:{}uL] reserves=improved matter=conserved fluid=conserved tick={}",
            policy.label(),
            world.start_profile.label(),
            witness_food.commodity().value(),
            witness_mass.milligrams(),
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

fn evaluate_survival_pressure_response_probe(registries: &Registries, seed: u64) {
    let mut dry_foods = registries
        .survival()
        .foods()
        .copied()
        .filter(|food| food.hydration_microliters_per_milligram() == 0)
        .collect::<Vec<_>>();
    dry_foods.sort_by_key(|food| food.commodity());
    assert!(
        !dry_foods.is_empty(),
        "survival pressure probe requires one authored dry food so hunger and thirst actions remain physically distinct"
    );
    let dry_food =
        dry_foods[usize::try_from(mix64(seed ^ 0x5052_4553_5355_5245) % dry_foods.len() as u64)
            .unwrap_or_else(|_| unreachable!("dry-food index fits usize"))];
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    assert!(
        !drinks.is_empty(),
        "survival pressure probe requires one authored drink"
    );
    let drink = drinks[usize::try_from(mix64(seed ^ 0x5052_4553_4452_494E) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("pressure-probe drink index fits usize"))];
    let food_mass = Mass::from_milligrams(1);
    let drink_volume = Volume::from_microliters(1);
    let physiology = registries.survival().physiology();

    let mut hunger = AppState::new(WorldSeed::new(seed ^ 0x4855_4E47_4552_0001));
    let hunger_food_store = seed_stockpile(
        &mut hunger,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let hunger_food = seed_lot(
        registries,
        &mut hunger,
        hunger_food_store,
        dry_food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    let hunger_drink_store = seed_fluid_store(
        registries,
        &mut hunger,
        drink_volume,
        drink.fluid(),
        drink_volume,
        ROOM_TEMPERATURE,
    );
    seed_player_survival_at_hunger_warning_boundary(registries, &mut hunger);
    let hunger_before = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared"));
    let hunger_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        hunger_before.metabolic_energy(),
        physiology.maximum_hydration(),
        hunger_before.hydration(),
    );
    assert_eq!(hunger_priority, ProvisioningPriority::MetabolicEnergy);
    let _ = validate_drink(registries, &hunger, hunger_drink_store, drink_volume)
        .unwrap_or_else(|error| {
            panic!(
                "timed drinking at full hydration should remain useful because basal loss creates capacity during the action: {error}"
            )
        });
    let mut hunger_baseline = hunger.clone();
    let hunger_meal = validate_eat(
        registries,
        &hunger,
        hunger_food_store,
        &[MaterialLotSelection::new(hunger_food, food_mass)],
    )
    .unwrap_or_else(|error| panic!("hunger-pressure dry food should be useful: {error}"))
    .commit(&mut hunger)
    .unwrap_or_else(|error| panic!("hunger-pressure meal commit failed: {error}"));
    assert!(!hunger_meal.energy_offered().is_zero());
    let hunger_ticks =
        finish_direct_consumption(registries, &mut hunger, hunger_meal.completes_at());
    advance_idle_ticks(
        registries,
        &mut hunger_baseline,
        hunger_ticks,
        "hunger-pressure no-meal baseline",
    );
    let hunger_after = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared after eating"));
    let hunger_baseline_after = assess_survival(registries, &hunger_baseline)
        .unwrap_or_else(|| panic!("hunger-pressure baseline player disappeared"));
    assert!(hunger_after.metabolic_energy() > hunger_baseline_after.metabolic_energy());

    let mut thirst = AppState::new(WorldSeed::new(seed ^ 0x5448_4952_5354_0002));
    let thirst_food_store = seed_stockpile(
        &mut thirst,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let thirst_food = seed_lot(
        registries,
        &mut thirst,
        thirst_food_store,
        dry_food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    let thirst_drink_store = seed_fluid_store(
        registries,
        &mut thirst,
        drink_volume,
        drink.fluid(),
        drink_volume,
        ROOM_TEMPERATURE,
    );
    seed_player_survival_at_hydration_warning_boundary(registries, &mut thirst);
    let thirst_before = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared"));
    let thirst_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        thirst_before.metabolic_energy(),
        physiology.maximum_hydration(),
        thirst_before.hydration(),
    );
    assert_eq!(thirst_priority, ProvisioningPriority::Hydration);
    let _ = validate_eat(
        registries,
        &thirst,
        thirst_food_store,
        &[MaterialLotSelection::new(thirst_food, food_mass)],
    )
    .unwrap_or_else(|error| {
        panic!(
            "timed eating at full metabolic reserves should remain useful because basal cost creates capacity during the action: {error}"
        )
    });
    let mut thirst_baseline = thirst.clone();
    let thirst_drink = validate_drink(registries, &thirst, thirst_drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("thirst-pressure drink should be useful: {error}"))
        .commit(&mut thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure drink commit failed: {error}"));
    assert!(!thirst_drink.hydration_offered().is_zero());
    let thirst_ticks =
        finish_direct_consumption(registries, &mut thirst, thirst_drink.completes_at());
    advance_idle_ticks(
        registries,
        &mut thirst_baseline,
        thirst_ticks,
        "thirst-pressure no-drink baseline",
    );
    let thirst_after = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared after drinking"));
    let thirst_baseline_after = assess_survival(registries, &thirst_baseline)
        .unwrap_or_else(|| panic!("thirst-pressure baseline player disappeared"));
    assert!(thirst_after.hydration() > thirst_baseline_after.hydration());
    validate_loaded_state(registries, &hunger)
        .unwrap_or_else(|error| panic!("hunger-pressure state audit failed: {error}"));
    validate_loaded_state(registries, &thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        reviewln!(
            "SURVIVAL PRESSURE seed=0x{seed:016X} matched-warning-boundary-worlds=[hunger:[priority:{} eat:targeted drink:legal-nontarget] thirst:[priority:{} drink:targeted dry-food:legal-nontarget]] response=pressure-sensitive counterfactual-benefit=true canonical-actions=true",
            hunger_priority.label(),
            thirst_priority.label(),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurvivalWorkPressureReview {
    prospecting_method: ProspectingMethodId,
    prospecting_region_voxels: u128,
    prospecting_ticks: u64,
    prospecting_energy_deficit_ppm: u32,
    prospecting_hydration_deficit_ppm: u32,
    manual_power_ticks: u64,
    manual_power_energy_deficit_ppm: u32,
    manual_power_hydration_deficit_ppm: u32,
    stored_work_nj: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntegratedSurvivalWorkReview {
    initial_drink_ticks: u64,
    prospecting_ticks: u64,
    reprovisioned_after_prospecting: bool,
    reprovision_ticks: u64,
    manual_power_ticks: u64,
    stored_work_nj: u128,
    energy_deficit_ppm: u32,
    hydration_deficit_ppm: u32,
    hydration_warning_safe: bool,
}

pub(super) fn prospecting_method_for_work_pressure(
    registries: &Registries,
    seed: u64,
) -> ProspectingMethodId {
    let mut methods = registries
        .labor()
        .prospecting_definitions()
        .filter(|definition| definition.equipment().is_none())
        .map(|definition| definition.id())
        .collect::<Vec<_>>();
    methods.sort_unstable();
    assert!(
        !methods.is_empty(),
        "survival work-pressure probe requires an authored prospecting method"
    );
    let index = usize::try_from(mix64(seed ^ 0x5052_4F53_4D45_5448) % methods.len() as u64)
        .unwrap_or_else(|_| unreachable!("prospecting method index fits usize"));
    methods[index]
}

fn evaluate_survival_work_pressure_probe(
    registries: &Registries,
    seed: u64,
) -> SurvivalWorkPressureReview {
    let physiology = registries.survival().physiology();

    let mut prospecting = AppState::new(WorldSeed::new(seed ^ 0x5052_4F53_5045_4354));
    initialize_player_survival(registries, &mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting survival setup failed: {error}"));
    let prospecting_method = prospecting_method_for_work_pressure(registries, seed);
    let prospecting_definition = registries
        .labor()
        .get_prospecting(prospecting_method)
        .copied()
        .unwrap_or_else(|| panic!("selected work-pressure prospecting method disappeared"));
    let region_width = i64::try_from(prospecting_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| unreachable!("bounded prospecting footprint fits i64"));
    let region = VoxelBounds::new(
        VoxelCoord::new(24, -1, 0),
        VoxelCoord::new(24 + region_width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("work-pressure prospecting bounds failed: {error}"));
    let prospecting_region_voxels = region
        .voxel_count()
        .unwrap_or_else(|| panic!("work-pressure prospecting region volume overflowed"));
    let prospecting_before = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared"));
    let prospecting_start = validate_start_field_prospecting(
        registries,
        &prospecting,
        FieldProspectingRequest::new(prospecting_method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("work-pressure prospecting start failed: {error}"));
    let prospecting_work = prospecting_start.work();
    let prospecting_ticks = prospecting_work
        .completes_at()
        .value()
        .checked_sub(prospecting_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated prospecting completes after it starts"));
    prospecting_start
        .commit(&mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting commit failed: {error}"));
    let mut prospecting_completion = None;
    for elapsed in 1..=prospecting_ticks {
        let outcome = advance_tick(registries, &mut prospecting)
            .unwrap_or_else(|error| panic!("work-pressure prospecting tick failed: {error}"));
        let completion = outcome.field_prospecting();
        if elapsed < prospecting_ticks {
            assert_eq!(
                completion, None,
                "work-pressure prospecting completed before its validated schedule"
            );
        } else {
            prospecting_completion = completion;
        }
    }
    let prospecting_completion = prospecting_completion
        .unwrap_or_else(|| panic!("work-pressure prospecting produced no completion outcome"));
    assert_eq!(prospecting_completion.method(), prospecting_method);
    assert_eq!(prospecting_completion.region(), region);
    assert_eq!(prospecting_completion.material(), MATERIAL_COPPER);
    assert!(
        prospecting
            .geological_knowledge()
            .get_observation(prospecting_completion.observation())
            .is_some(),
        "work-pressure prospecting completion must identify its persisted observation"
    );
    let prospecting_after = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared after work"));
    let prospecting_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        prospecting_after.metabolic_energy(),
    );
    let prospecting_hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        prospecting_after.hydration(),
    );
    assert_eq!(
        prospecting_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(
        prospecting_before.hydration(),
        physiology.maximum_hydration()
    );
    assert!(prospecting_energy_deficit_ppm > 0);
    assert!(prospecting_hydration_deficit_ppm > 0);

    let mut power = AppState::new(WorldSeed::new(seed ^ 0x504F_5745_5257_4F52));
    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("work-pressure primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut power,
        component_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut power,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &power,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &power,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly commit failed: {error}"));
    initialize_player_survival(registries, &mut power).unwrap_or_else(|error| {
        panic!("work-pressure manual-power survival setup failed: {error}")
    });
    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel definition disappeared"));
    let power_before = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared"));
    let power_start = validate_start_manual_power(
        registries,
        &power,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy),
    )
    .unwrap_or_else(|error| panic!("work-pressure manual-power start failed: {error}"));
    let power_work = power_start.work();
    let power_ticks = power_work
        .completes_at()
        .value()
        .checked_sub(power_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated manual power completes after it starts"));
    power_start
        .commit(&mut power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(
            registries,
            &mut power,
            power_work,
            "work-pressure manual power"
        ),
        power_ticks
    );
    assert_eq!(
        power.energy().get_store(drive).map(|store| store.stored()),
        Some(requested_energy),
        "matched work-pressure manual labor must create the requested finite stored work"
    );
    let power_after = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared after work"));
    let power_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        power_after.metabolic_energy(),
    );
    let power_hydration_deficit_ppm =
        normalized_hydration_deficit_ppm(physiology.maximum_hydration(), power_after.hydration());
    assert_eq!(
        power_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(power_before.hydration(), physiology.maximum_hydration());
    assert!(power_energy_deficit_ppm > 0);
    assert!(power_hydration_deficit_ppm > 0);

    let prospecting_priority = normalized_deficit_priority(
        prospecting_energy_deficit_ppm,
        prospecting_hydration_deficit_ppm,
    );
    let power_priority =
        normalized_deficit_priority(power_energy_deficit_ppm, power_hydration_deficit_ppm);
    let activity_changes_dominant_pressure = prospecting_priority != power_priority;

    validate_loaded_state(registries, &prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting state audit failed: {error}"));
    validate_loaded_state(registries, &power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        reviewln!(
            "SURVIVAL WORK PRESSURE seed=0x{seed:016X} matched-full-reserve-work=[prospecting:[method:{} region:{}vox {}t energy:{}ppm hydration:{}ppm dominant:{}] manual-power:[{}t energy:{}ppm hydration:{}ppm dominant:{} stored-work:{}nJ]] activity-changes-dominant-pressure:{} canonical-actions=true",
            prospecting_method.value(),
            prospecting_region_voxels,
            prospecting_ticks,
            prospecting_energy_deficit_ppm,
            prospecting_hydration_deficit_ppm,
            prospecting_priority.label(),
            power_ticks,
            power_energy_deficit_ppm,
            power_hydration_deficit_ppm,
            power_priority.label(),
            requested_energy.nanojoules(),
            activity_changes_dominant_pressure,
        );
    }
    SurvivalWorkPressureReview {
        prospecting_method,
        prospecting_region_voxels,
        prospecting_ticks,
        prospecting_energy_deficit_ppm,
        prospecting_hydration_deficit_ppm,
        manual_power_ticks: power_ticks,
        manual_power_energy_deficit_ppm: power_energy_deficit_ppm,
        manual_power_hydration_deficit_ppm: power_hydration_deficit_ppm,
        stored_work_nj: requested_energy.nanojoules(),
    }
}

fn evaluate_integrated_survival_work_loop(
    registries: &Registries,
    seed: u64,
) -> IntegratedSurvivalWorkReview {
    let physiology = registries.survival().physiology();
    let direct = physiology.direct_consumption();
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    let drink = drinks
        .get(
            usize::try_from(mix64(seed ^ 0x494E_5445_4752_4452) % drinks.len().max(1) as u64)
                .unwrap_or_else(|_| unreachable!("integrated survival drink index fits usize")),
        )
        .copied()
        .unwrap_or_else(|| panic!("integrated survival work loop requires one authored drink"));
    let drink_volume = direct.maximum_drink_volume();
    let mut state = AppState::new(WorldSeed::new(seed ^ 0x494E_5445_4752_4154));
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        drink_volume
            .checked_add(drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink capacity overflowed")),
        drink.fluid(),
        drink_volume
            .checked_add(drink_volume)
            .unwrap_or_else(|| panic!("integrated survival drink supply overflowed")),
        ROOM_TEMPERATURE,
    );

    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("integrated survival stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("integrated survival primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut state,
        component_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut state,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &state,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival crank assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &state,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("integrated survival flywheel assembly commit failed: {error}"));
    seed_player_survival_at_hydration_warning_boundary(registries, &mut state);

    let start = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared at start"));
    assert_eq!(start.hydration(), physiology.thirsty_below());
    let first_drink = validate_drink(registries, &state, drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("integrated survival initial drink failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival initial drink commit failed: {error}"));
    let initial_drink_ticks =
        finish_direct_consumption(registries, &mut state, first_drink.completes_at());
    let after_drink = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after drinking"));
    assert!(after_drink.hydration() > physiology.thirsty_below());

    let prospecting_method = prospecting_method_for_work_pressure(registries, seed);
    let prospecting_definition = registries
        .labor()
        .get_prospecting(prospecting_method)
        .copied()
        .unwrap_or_else(|| panic!("integrated survival prospecting method disappeared"));
    let region_width = i64::try_from(prospecting_definition.maximum_region_voxels().min(4))
        .unwrap_or_else(|_| unreachable!("bounded integrated prospecting footprint fits i64"));
    let region = VoxelBounds::new(
        VoxelCoord::new(40, -1, 0),
        VoxelCoord::new(40 + region_width, 0, 1),
    )
    .unwrap_or_else(|error| panic!("integrated survival prospecting bounds failed: {error}"));
    let prospecting = validate_start_field_prospecting(
        registries,
        &state,
        FieldProspectingRequest::new(prospecting_method, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("integrated survival prospecting start failed: {error}"));
    let prospecting_work = prospecting.work();
    let prospecting_ticks = prospecting_work.completes_at().value() - state.tick().value();
    prospecting
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival prospecting commit failed: {error}"));
    let mut observation = None;
    for _ in 0..prospecting_ticks {
        observation = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("integrated survival prospecting tick failed: {error}"))
            .field_prospecting();
    }
    assert!(
        observation.is_some(),
        "integrated survival prospecting produced no observation"
    );

    let after_prospecting = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after prospecting"));
    let reprovisioned_after_prospecting =
        after_prospecting.hydration() < physiology.thirsty_below();
    let reprovision_ticks = if reprovisioned_after_prospecting {
        let drink = validate_drink(registries, &state, drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("integrated survival follow-up drink failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("integrated survival follow-up drink commit failed: {error}")
            });
        finish_direct_consumption(registries, &mut state, drink.completes_at())
    } else {
        0
    };

    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("integrated survival flywheel definition disappeared"));
    let power = validate_start_manual_power(
        registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy),
    )
    .unwrap_or_else(|error| panic!("integrated survival manual-power start failed: {error}"));
    let work = power.work();
    let manual_power_ticks = work.completes_at().value() - state.tick().value();
    power
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("integrated survival manual-power commit failed: {error}"));
    assert_eq!(
        finish_manual_power_work(
            registries,
            &mut state,
            work,
            "integrated survival manual power"
        ),
        manual_power_ticks
    );
    assert_eq!(
        state.energy().get_store(drive).map(|store| store.stored()),
        Some(requested_energy)
    );
    let final_survival = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("integrated survival player disappeared after work loop"));
    let energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        final_survival.metabolic_energy(),
    );
    let hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        final_survival.hydration(),
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("integrated survival work-loop audit failed: {error}"));

    IntegratedSurvivalWorkReview {
        initial_drink_ticks,
        prospecting_ticks,
        reprovisioned_after_prospecting,
        reprovision_ticks,
        manual_power_ticks,
        stored_work_nj: requested_energy.nanojoules(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        hydration_warning_safe: final_survival.hydration() >= physiology.thirsty_below(),
    }
}

fn evaluate_survival_provisioning_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let sample = focused_probe_role_label(case.role());
    let behavior_seed = case
        .behavior_seed()
        .unwrap_or_else(|| panic!("survival probe is missing its actor behavior seed"));
    let world = provisioning_world(registries, seed);
    let protected_food = world.foods[world.witness_index];
    let protected_reserve_mass = world.preserved_reserve_mass;
    let attention_investment = evaluate_preservation_infrastructure_probe(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        PreservationInvestmentPolicy::AttentionEfficient,
    );
    let protection_investment = evaluate_preservation_infrastructure_probe(
        registries,
        seed,
        protected_food,
        protected_reserve_mass,
        PreservationInvestmentPolicy::MaximumProtection,
    );
    assert_eq!(
        attention_investment.food_commodity, protection_investment.food_commodity,
        "matched preservation choices must protect the same food"
    );
    assert_eq!(
        attention_investment.bootstrap_age_ticks, protection_investment.bootstrap_age_ticks,
        "matched preservation choices must begin from the same food age"
    );
    assert_eq!(
        attention_investment.ambient_age_after_ticks, protection_investment.ambient_age_after_ticks,
        "matched preservation choices must be judged at the same wall-clock endpoint"
    );
    let protection_attention_delta_ticks = protection_investment
        .production_ticks
        .checked_sub(attention_investment.production_ticks)
        .unwrap_or_else(|| unreachable!("maximum protection is not cheaper to construct"));
    let protection_raw_delta_mg = protection_investment
        .raw_material_mass_mg
        .checked_sub(attention_investment.raw_material_mass_mg)
        .unwrap_or_else(|| unreachable!("maximum protection does not use less raw matter"));
    let protection_freshness_delta_ticks =
        i128::from(attention_investment.enclosed_age_after_ticks)
            - i128::from(protection_investment.enclosed_age_after_ticks);
    let protection_remaining_fresh_delta_ticks =
        i128::from(protection_investment.enclosed_remaining_fresh_ticks)
            - i128::from(attention_investment.enclosed_remaining_fresh_ticks);
    let protection_remaining_fresh_gain_ticks =
        u64::try_from(protection_remaining_fresh_delta_ticks.max(0))
            .unwrap_or_else(|_| panic!("bounded preservation benefit exceeds u64"));
    let preservation_return_ppm = if protection_attention_delta_ticks == 0 {
        0
    } else {
        u32::try_from(
            u128::from(protection_remaining_fresh_gain_ticks) * 1_000_000
                / u128::from(protection_attention_delta_ticks),
        )
        .unwrap_or(u32::MAX)
    };
    let preservation_return_threshold_ppm =
        preservation_freshness_return_threshold_ppm(behavior_seed);
    let projected_candidates =
        project_preservation_candidates(registries, seed, protected_food, protected_reserve_mass);
    let selected_projection = select_preservation_projection(behavior_seed, &projected_candidates);
    let preservation_infrastructure =
        if selected_projection.definition == attention_investment.storage_definition {
            attention_investment
        } else if selected_projection.definition == protection_investment.storage_definition {
            protection_investment
        } else {
            evaluate_preservation_infrastructure_definition(
                registries,
                seed,
                protected_food,
                protected_reserve_mass,
                selected_projection.definition,
            )
        };
    assert_eq!(
        preservation_infrastructure.enclosed_remaining_fresh_ticks,
        selected_projection.remaining_fresh_ticks,
        "executed preservation choice must match the canonical projected frontier consequence"
    );
    let protection_metabolic_delta_nj = protection_investment
        .metabolic_cost_nj
        .checked_sub(attention_investment.metabolic_cost_nj)
        .unwrap_or_else(|| unreachable!("maximum protection does not cost less manual exertion"));
    let protection_hydration_delta_ul = protection_investment
        .hydration_cost_ul
        .checked_sub(attention_investment.hydration_cost_ul)
        .unwrap_or_else(|| unreachable!("maximum protection does not cost less hydration"));
    let attention_investment_time =
        format_physical_duration(registries, attention_investment.production_ticks);
    let protection_investment_time =
        format_physical_duration(registries, protection_investment.production_ticks);
    let selected_investment_time =
        format_physical_duration(registries, preservation_infrastructure.production_ticks);
    let protection_attention_delta_time =
        format_physical_duration(registries, protection_attention_delta_ticks);
    let protection_freshness_delta_magnitude =
        u64::try_from(protection_freshness_delta_ticks.unsigned_abs())
            .unwrap_or_else(|_| panic!("bounded preservation freshness delta exceeds u64"));
    let protection_freshness_delta_time = format!(
        "{}{}",
        if protection_freshness_delta_ticks >= 0 {
            "+"
        } else {
            "-"
        },
        format_physical_duration(registries, protection_freshness_delta_magnitude)
    );
    let protection_remaining_delta_magnitude =
        u64::try_from(protection_remaining_fresh_delta_ticks.unsigned_abs())
            .unwrap_or_else(|_| panic!("bounded remaining freshness delta exceeds u64"));
    let protection_remaining_delta_time = format!(
        "{}{}",
        if protection_remaining_fresh_delta_ticks >= 0 {
            "+"
        } else {
            "-"
        },
        format_physical_duration(registries, protection_remaining_delta_magnitude)
    );
    evaluate_survival_pressure_response_probe(registries, seed);
    let work_pressure = evaluate_survival_work_pressure_probe(registries, seed);
    let integrated_work = evaluate_integrated_survival_work_loop(registries, seed);
    let prospecting_pressure = normalized_deficit_priority(
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
    );
    let manual_power_pressure = normalized_deficit_priority(
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
    );
    let foods = world.foods.as_slice();
    let available_category_count = food_category_count(foods);
    let authored_category_count = registries
        .survival()
        .foods()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len();
    let provisioning_wait_ticks = world.provisioning_wait_ticks;
    let drink_supply = provisioning_drink_supply(registries, &world);
    let prepared = prepare_provisioning_world(registries, seed, &world, drink_supply);
    let compact_plan = provisioning_plan(
        registries,
        &world,
        &prepared,
        DietProvisioningPolicy::CompactCalories,
    );
    let balanced_plan = provisioning_plan(
        registries,
        &world,
        &prepared,
        DietProvisioningPolicy::BalancedRecovery,
    );
    assert!(compact_plan.drink_volume <= drink_supply);
    assert!(balanced_plan.drink_volume <= drink_supply);
    let compact_provisioning_ticks = provisioning_plan_duration(registries, &compact_plan);
    let balanced_provisioning_ticks = provisioning_plan_duration(registries, &balanced_plan);
    let comparison_horizon_ticks = compact_provisioning_ticks.max(balanced_provisioning_ticks);

    let compact = run_provisioning_case(
        registries,
        behavior_seed,
        &world,
        &prepared,
        &compact_plan,
        DietProvisioningPolicy::CompactCalories,
        comparison_horizon_ticks,
    );
    let balanced = run_provisioning_case(
        registries,
        behavior_seed,
        &world,
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
    assert_eq!(
        compact.comparison_horizon_ticks,
        balanced.comparison_horizon_ticks
    );
    assert_eq!(compact.comparison_horizon_ticks, comparison_horizon_ticks);
    assert_eq!(
        compact.provisioning_elapsed_ticks,
        compact_provisioning_ticks
    );
    assert_eq!(
        balanced.provisioning_elapsed_ticks,
        balanced_provisioning_ticks
    );
    assert_eq!(
        compact.preservation_age_saved_ticks,
        balanced.preservation_age_saved_ticks
    );
    assert_eq!(
        compact.retained_preserved_mass_mg,
        balanced.retained_preserved_mass_mg
    );
    assert!(compact.reserve_recovered && balanced.reserve_recovered);
    let natural_policy = diet_provisioning_policy_for_behavior_seed(behavior_seed);
    let natural = match natural_policy {
        DietProvisioningPolicy::CompactCalories => compact,
        DietProvisioningPolicy::BalancedRecovery => balanced,
    };
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
    let diet_quality_delta_ppm =
        i64::from(balanced.diet_quality_after_ppm) - i64::from(compact.diet_quality_after_ppm);
    let recovery_rate_delta_ppm_per_tick = i64::from(balanced.recovery_rate_after_ppm_per_tick)
        - i64::from(compact.recovery_rate_after_ppm_per_tick);
    let meal_mass_delta_mg = i128::from(balanced.meal_mass_mg) - i128::from(compact.meal_mass_mg);
    let water_saved_delta_ul =
        i128::from(compact.drink_volume_ul) - i128::from(balanced.drink_volume_ul);
    let reserve_recovered = compact.reserve_recovered && balanced.reserve_recovered;
    let diet_recovery = evaluate_diet_recovery_consequence(registries, seed, &world);
    let recovery_consequence = if diet_recovery.actionable {
        format!(
            "choice:actionable deprivation:{}t provisioning-horizon:{}t observe:{}t vitality:{}->[compact:{} balanced:{} delta:+{}ppm] diet:[compact:{} balanced:{}ppm]",
            diet_recovery.deprivation_ticks,
            diet_recovery.provisioning_horizon_ticks,
            diet_recovery.observation_ticks,
            diet_recovery.vitality_before_ppm,
            diet_recovery.compact_vitality_after_ppm,
            diet_recovery.balanced_vitality_after_ppm,
            diet_recovery.vitality_advantage_ppm,
            diet_recovery.compact_diet_quality_ppm,
            diet_recovery.balanced_diet_quality_ppm,
        )
    } else {
        "choice:supply-collapsed reason=available-categories-below-authored-diet-set".to_string()
    };
    let choice_state = if available_category_count == authored_category_count {
        "policy-sensitive"
    } else {
        "supply-constrained"
    };
    let food_options = food_option_summary(registries, foods);
    let preservation_capacity_utilization_ppm = u32::try_from(
        u128::from(protected_reserve_mass.milligrams()) * 1_000_000
            / u128::from(preservation_infrastructure.capacity_mass_mg),
    )
    .unwrap_or_else(|_| panic!("preservation capacity utilization exceeded normalized range"));
    reviewln!(
        "SURVIVAL EXPERIENCE seed=0x{seed:016X} sample={sample} start={} supply=[foods:{} categories:{}] pressure={} choice=[state:{choice_state} diet:{} meal:{}mg drink:{}uL] inherited-reserve=[storage:{} preservation:{}ppm rotation:consume-ambient-first retained:{}mg age-saved:{}t] current-investment=[protected-reserve:{}mg storage-policy:{} value=[strongest-return:{}ppm attention-value:{}ppm] selected:{} preservation:{}ppm candidates:{} fastest:{}:{}t/{}:{}ppm strongest:{}:{}t/{}:{}ppm build:{}t/{} raw:{}mg embodied:{}mg capacity:{}mg utilization:{}ppm dismantle=[{}t body:{}nJ/{}uL returned:{}mg] stronger-tradeoff=[attention:+{}t/+{} raw:+{}mg body:+{}nJ/+{}uL matched-age:{:+}t/{} remaining-edible:{:+}t/{}]] consequence=[reserve-improved:{} diet-delta:{:+}ppm recovery-delta:{:+}ppm/t horizon:{}t] work-interlock=[prospecting:{}t cost:{}ppmE/{}ppmH dominant:{} manual-power:{}t cost:{}ppmE/{}ppmH dominant:{} integrated=[drink:{}t prospect:{}t reprovision:{}:{}t power:{}t stored:{}nJ final-reserve:{}ppmE/{}ppmH warning-safe:{}]]",
        world.start_profile.label(),
        foods.len(),
        available_category_count,
        compact.provisioning_priority.label(),
        natural_policy.label(),
        natural.meal_mass_mg,
        natural.drink_volume_ul,
        world.inherited_preservation_definition.value(),
        world.inherited_preservation_multiplier_ppm,
        compact.retained_preserved_mass_mg,
        compact.preservation_age_saved_ticks,
        protected_reserve_mass.milligrams(),
        preservation_infrastructure.selection_kind.label(),
        preservation_return_ppm,
        preservation_return_threshold_ppm,
        preservation_infrastructure.storage_definition.value(),
        preservation_infrastructure.preservation_multiplier_ppm,
        preservation_infrastructure.candidate_count,
        preservation_infrastructure.fastest_definition.value(),
        preservation_infrastructure.fastest_ticks,
        attention_investment_time,
        preservation_infrastructure.fastest_preservation_multiplier_ppm,
        preservation_infrastructure.strongest_definition.value(),
        preservation_infrastructure.strongest_ticks,
        protection_investment_time,
        preservation_infrastructure.strongest_preservation_multiplier_ppm,
        preservation_infrastructure.production_ticks,
        selected_investment_time,
        preservation_infrastructure.raw_material_mass_mg,
        preservation_infrastructure.embodied_mass_mg,
        preservation_infrastructure.capacity_mass_mg,
        preservation_capacity_utilization_ppm,
        preservation_infrastructure.dismantle_ticks,
        preservation_infrastructure.dismantle_metabolic_cost_nj,
        preservation_infrastructure.dismantle_hydration_cost_ul,
        preservation_infrastructure.recovered_enclosure_mass_mg,
        protection_attention_delta_ticks,
        protection_attention_delta_time,
        protection_raw_delta_mg,
        protection_metabolic_delta_nj,
        protection_hydration_delta_ul,
        protection_freshness_delta_ticks,
        protection_freshness_delta_time,
        protection_remaining_fresh_delta_ticks,
        protection_remaining_delta_time,
        reserve_recovered,
        diet_quality_delta_ppm,
        recovery_rate_delta_ppm_per_tick,
        comparison_horizon_ticks,
        work_pressure.prospecting_ticks,
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
        prospecting_pressure.label(),
        work_pressure.manual_power_ticks,
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
        manual_power_pressure.label(),
        integrated_work.initial_drink_ticks,
        integrated_work.prospecting_ticks,
        integrated_work.reprovisioned_after_prospecting,
        integrated_work.reprovision_ticks,
        integrated_work.manual_power_ticks,
        integrated_work.stored_work_nj,
        1_000_000 - integrated_work.energy_deficit_ppm,
        1_000_000 - integrated_work.hydration_deficit_ppm,
        integrated_work.hydration_warning_safe,
    );
    reviewln!(
        "SURVIVAL REVIEW seed=0x{seed:016X} behavior=0x{behavior_seed:016X} sample={sample} role=runtime-experience-after-disclosed-bootstrap fantasy=prepare+provision episode=[start:{} wait:{provisioning_wait_ticks}t available:[foods:{} categories:{} options:{food_options}]] preservation-choice=[policy:{} value=[strongest-return:{}ppm attention-value:{}ppm] candidates:{} fastest=[storage:{} attention:{}t multiplier:{}ppm remaining:{}t] strongest=[storage:{} attention:{}t multiplier:{}ppm remaining:{}t] selected:{} stronger-tradeoff=[attention:+{}t raw:+{}mg metabolic:+{}nJ hydration:+{}uL matched-age:{:+}t remaining-edible:{:+}t]] preservation-infrastructure=[food:{} stages:{} route=shared-raw-opportunity->manual-production-forest->enclosure production:{}t observation:{}t raw:{}mg embodied:{}mg residual:{}mg capacity:{}mg multiplier:{}ppm witness=[bootstrap-age:{}t ambient:{}:{}t enclosed:{}:{}t remaining:{}t saved:{}t] survival-cost:{}nJ+{}uL dismantle=[{}t body:{}nJ/{}uL returned:{}mg]] activity-pressure=[prospecting:[method:{} region:{}vox {}t] energy:{}ppm hydration:{}ppm dominant:{}; manual-power:{}t energy:{}ppm hydration:{}ppm dominant:{} stored-work:{}nJ; contrast:{}] integrated-work-loop=[start:hydration-warning provision:{}t prospect:{}t reprovision:{}:{}t generate:{}t stored:{}nJ final-reserve:{}ppmE/{}ppmH warning-safe:{}] actor-choice=[diet-policy:{} selected:{} meal:{}mg drink:{}uL] matched-counterfactual=[horizon:{}t compact-calories:[action:{}t selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t] balanced:[action:{}t selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t]] tradeoff=[meal-mass-delta:{:+}mg water-saved-delta:{:+}uL diet-quality-delta:{:+}ppm recovery-delta:{:+}ppm/t] recovery-consequence=[{recovery_consequence}] decision-pressure=[energy:{}ppm hydration:{}ppm dominant:{}] inherited-preservation=[definition:{} age-saved:{}t retained:{}mg] reserve-recovered:{}",
        world.start_profile.label(),
        foods.len(),
        available_category_count,
        preservation_infrastructure.selection_kind.label(),
        preservation_return_ppm,
        preservation_return_threshold_ppm,
        preservation_infrastructure.candidate_count,
        preservation_infrastructure.fastest_definition.value(),
        preservation_infrastructure.fastest_ticks,
        preservation_infrastructure.fastest_preservation_multiplier_ppm,
        attention_investment.enclosed_remaining_fresh_ticks,
        preservation_infrastructure.strongest_definition.value(),
        preservation_infrastructure.strongest_ticks,
        preservation_infrastructure.strongest_preservation_multiplier_ppm,
        protection_investment.enclosed_remaining_fresh_ticks,
        preservation_infrastructure.storage_definition.value(),
        protection_attention_delta_ticks,
        protection_raw_delta_mg,
        protection_metabolic_delta_nj,
        protection_hydration_delta_ul,
        protection_freshness_delta_ticks,
        protection_remaining_fresh_delta_ticks,
        preservation_infrastructure.food_commodity.value(),
        preservation_infrastructure.construction_stages,
        preservation_infrastructure.production_ticks,
        preservation_infrastructure.observation_ticks,
        preservation_infrastructure.raw_material_mass_mg,
        preservation_infrastructure.embodied_mass_mg,
        preservation_infrastructure.residual_mass_mg,
        preservation_infrastructure.capacity_mass_mg,
        preservation_infrastructure.preservation_multiplier_ppm,
        preservation_infrastructure.bootstrap_age_ticks,
        if preservation_infrastructure.ambient_spoiled {
            "spoiled"
        } else {
            "fresh"
        },
        preservation_infrastructure.ambient_age_after_ticks,
        if preservation_infrastructure.enclosed_fresh {
            "fresh"
        } else {
            "spoiled"
        },
        preservation_infrastructure.enclosed_age_after_ticks,
        preservation_infrastructure.enclosed_remaining_fresh_ticks,
        preservation_infrastructure.age_saved_ticks,
        preservation_infrastructure.metabolic_cost_nj,
        preservation_infrastructure.hydration_cost_ul,
        preservation_infrastructure.dismantle_ticks,
        preservation_infrastructure.dismantle_metabolic_cost_nj,
        preservation_infrastructure.dismantle_hydration_cost_ul,
        preservation_infrastructure.recovered_enclosure_mass_mg,
        work_pressure.prospecting_method.value(),
        work_pressure.prospecting_region_voxels,
        work_pressure.prospecting_ticks,
        work_pressure.prospecting_energy_deficit_ppm,
        work_pressure.prospecting_hydration_deficit_ppm,
        prospecting_pressure.label(),
        work_pressure.manual_power_ticks,
        work_pressure.manual_power_energy_deficit_ppm,
        work_pressure.manual_power_hydration_deficit_ppm,
        manual_power_pressure.label(),
        work_pressure.stored_work_nj,
        prospecting_pressure != manual_power_pressure,
        integrated_work.initial_drink_ticks,
        integrated_work.prospecting_ticks,
        integrated_work.reprovisioned_after_prospecting,
        integrated_work.reprovision_ticks,
        integrated_work.manual_power_ticks,
        integrated_work.stored_work_nj,
        1_000_000 - integrated_work.energy_deficit_ppm,
        1_000_000 - integrated_work.hydration_deficit_ppm,
        integrated_work.hydration_warning_safe,
        natural_policy.label(),
        natural.selected_category_count,
        natural.meal_mass_mg,
        natural.drink_volume_ul,
        comparison_horizon_ticks,
        compact.provisioning_elapsed_ticks,
        compact.selected_category_count,
        compact.meal_mass_mg,
        compact.drink_volume_ul,
        compact.diet_quality_before_ppm,
        compact.diet_quality_after_ppm,
        compact.recovery_rate_before_ppm_per_tick,
        compact.recovery_rate_after_ppm_per_tick,
        balanced.provisioning_elapsed_ticks,
        balanced.selected_category_count,
        balanced.meal_mass_mg,
        balanced.drink_volume_ul,
        balanced.diet_quality_before_ppm,
        balanced.diet_quality_after_ppm,
        balanced.recovery_rate_before_ppm_per_tick,
        balanced.recovery_rate_after_ppm_per_tick,
        meal_mass_delta_mg,
        water_saved_delta_ul,
        diet_quality_delta_ppm,
        recovery_rate_delta_ppm_per_tick,
        compact.energy_deficit_ppm,
        compact.hydration_deficit_ppm,
        compact.provisioning_priority.label(),
        world.inherited_preservation_definition.value(),
        compact.preservation_age_saved_ticks,
        compact.retained_preserved_mass_mg,
        reserve_recovered,
    );
}

pub(super) fn run_survival_provisioning_probe(registries: &Registries, case: FocusedProbeCase) {
    evaluate_survival_provisioning_probe(registries, case);
}
