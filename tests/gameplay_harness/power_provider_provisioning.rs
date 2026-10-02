//! Canonical carried-provision recovery for long human-power projects.

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_lot, seed_stockpile};
use deep_hearth::content::{
    FLUID_WATER, FORM_FOOD, MATERIAL_GRAIN, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
};
use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::TickSpan;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile,
    validate_build_storage_enclosure,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::survival::{
    DrinkHydrationProjectionError, DrinkStoreToTargetError, FoodFreshness,
    MealMetabolicProjectionError, SurvivalExertion, assess_food_freshness, assess_survival,
    project_minimum_meal_to_metabolic_target, project_survival_resource_budget, validate_drink,
    validate_drink_store_to_hydration_target, validate_eat, validate_eat_lot_to_metabolic_target,
};

use super::super::direct_consumption_timing::finish_direct_consumption_work;
use super::super::environment::ROOM_TEMPERATURE;

#[derive(Clone, Copy)]
pub(super) struct PowerProjectProvisions {
    pub(super) food: StockpileId,
    pub(super) enclosure_material: StockpileId,
    pub(super) water: FluidStoreId,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) food_supply_mg: u64,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) food_preservation_ppm: u32,
    #[cfg_attr(
        test,
        allow(dead_code, reason = "exploratory power-provider report telemetry")
    )]
    pub(super) water_supply_ul: u64,
}

#[derive(Clone, Copy)]
struct GrainMealPlan {
    mass: Mass,
    duration: TickSpan,
    energy_offered: Energy,
    reaches_target: bool,
}

fn plan_grain_meal(
    registries: &Registries,
    state: &AppState,
    target: Energy,
    context: &'static str,
) -> GrainMealPlan {
    let physiology = registries.survival().physiology();
    let grain = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("power project grain lost authored food definition"));
    let current = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player before meal planning"));
    match project_minimum_meal_to_metabolic_target(
        physiology,
        grain,
        current.metabolic_energy(),
        target,
    ) {
        Ok(Some(projection)) => GrainMealPlan {
            mass: projection.mass(),
            duration: projection.duration(),
            energy_offered: projection.energy_offered(),
            reaches_target: true,
        },
        Ok(None) => unreachable!("meal planning runs only below the metabolic target"),
        Err(MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit {
            maximum_meal_mass,
        }) => GrainMealPlan {
            mass: maximum_meal_mass,
            duration: physiology
                .direct_consumption()
                .meal_duration(maximum_meal_mass)
                .unwrap_or_else(|| unreachable!("authored maximum meal has a duration")),
            energy_offered: grain.dietary_energy_for_mass(maximum_meal_mass),
            reaches_target: false,
        },
        Err(error) => panic!("power project {context} meal target projection failed: {error}"),
    }
}

pub(super) fn seed_power_project_provisions(
    registries: &Registries,
    state: &mut AppState,
) -> PowerProjectProvisions {
    // This is an onsite project cache, not an encumbrance model. The double-wall chest favors
    // campaign capacity over maximum preservation while still covering the generated horizons.
    let physiology = registries.survival().physiology();
    let cache = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("power project double-wall provisions chest disappeared"));
    let food_supply = cache.maximum_stockpile_capacity();
    let food_supply_mg = food_supply.milligrams();
    let food = seed_stockpile(
        state,
        food_supply,
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let enclosure_material = seed_stockpile(
        state,
        cache.assembly_profile().input_mass(),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    for input in cache.assembly_profile().inputs() {
        seed_lot(
            registries,
            state,
            enclosure_material,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_build_storage_enclosure(
        registries,
        state,
        STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
        food,
        enclosure_material,
    )
    .unwrap_or_else(|error| panic!("power project food-cache construction failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("power project food-cache construction commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(food)
            .map(|stockpile| stockpile.storage_profile()),
        Some(cache.storage_profile()),
        "power project food cache must be backed by the authored enclosure"
    );
    seed_lot(
        registries,
        state,
        food,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        food_supply,
        ROOM_TEMPERATURE,
    );

    const PROJECT_WATER_RESERVE_MULTIPLIER: u64 = 8;
    let water_supply_ul = physiology
        .maximum_hydration()
        .microliters()
        .checked_mul(PROJECT_WATER_RESERVE_MULTIPLIER)
        .unwrap_or_else(|| panic!("power project carried water supply overflowed"));
    let water_supply = Volume::from_microliters(water_supply_ul);
    let water = seed_fluid_store(
        registries,
        state,
        water_supply,
        FLUID_WATER,
        water_supply,
        ROOM_TEMPERATURE,
    );
    PowerProjectProvisions {
        food,
        enclosure_material,
        water,
        food_supply_mg,
        food_preservation_ppm: cache.storage_profile().preservation_multiplier_ppm(),
        water_supply_ul,
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct ProvisioningOutcome {
    pub(super) stops: u64,
    pub(super) drink_actions: u64,
    pub(super) drink_volume_ul: u64,
    pub(super) meal_actions: u64,
    pub(super) meal_mass_mg: u64,
    pub(super) attention_ticks: u64,
}

impl ProvisioningOutcome {
    pub(super) fn add(&mut self, other: Self) {
        self.stops = self
            .stops
            .checked_add(other.stops)
            .unwrap_or_else(|| panic!("power project provisioning stop count overflowed"));
        self.drink_actions = self
            .drink_actions
            .checked_add(other.drink_actions)
            .unwrap_or_else(|| panic!("power project drink count overflowed"));
        self.drink_volume_ul = self
            .drink_volume_ul
            .checked_add(other.drink_volume_ul)
            .unwrap_or_else(|| panic!("power project drink volume overflowed"));
        self.meal_actions = self
            .meal_actions
            .checked_add(other.meal_actions)
            .unwrap_or_else(|| panic!("power project meal count overflowed"));
        self.meal_mass_mg = self
            .meal_mass_mg
            .checked_add(other.meal_mass_mg)
            .unwrap_or_else(|| panic!("power project meal mass overflowed"));
        self.attention_ticks = self
            .attention_ticks
            .checked_add(other.attention_ticks)
            .unwrap_or_else(|| panic!("power project provisioning attention overflowed"));
    }
}

fn grain_lot(registries: &Registries, state: &AppState, stockpile: StockpileId) -> MaterialLotId {
    let grain = CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD);
    state
        .inventory()
        .lot_ids(stockpile)
        .filter_map(|lot| {
            let record = state.inventory().get_lot(lot)?;
            if record.commodity() != grain {
                return None;
            }
            let remaining = match assess_food_freshness(registries, state, lot)
                .unwrap_or_else(|error| panic!("power project grain freshness failed: {error:?}"))
            {
                FoodFreshness::Fresh { remaining, .. } => remaining,
                FoodFreshness::Spoiled { .. } => return None,
            };
            Some((remaining.value(), lot))
        })
        .min_by_key(|(remaining, lot)| (*remaining, *lot))
        .map(|(_, lot)| lot)
        .unwrap_or_else(|| {
            let remaining = state
                .inventory()
                .get_stockpile(stockpile)
                .map(|record| record.get_mass(grain).milligrams())
                .unwrap_or(0);
            panic!(
                "power project provisions lack a grain stack at tick {} with {}mg remaining",
                state.tick().value(),
                remaining,
            )
        })
}

fn drink_to_target(
    registries: &Registries,
    state: &mut AppState,
    provisions: PowerProjectProvisions,
    target: Volume,
    context: &'static str,
    outcome: &mut ProvisioningOutcome,
) {
    while assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player while drinking"))
        .hydration()
        < target
    {
        let current_hydration = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project {context} lost player before drinking"))
            .hydration();
        let drink = match validate_drink_store_to_hydration_target(
            registries,
            state,
            provisions.water,
            target,
        ) {
            Ok(Some(drink)) => drink,
            Ok(None) => break,
            Err(DrinkStoreToTargetError::Projection(
                DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                    maximum_drink_volume,
                },
            )) => validate_drink(registries, state, provisions.water, maximum_drink_volume)
                .unwrap_or_else(|error| {
                    panic!("power project {context} maximum legal drink failed: {error}")
                }),
            Err(error) => panic!(
                "power project {context} drink-to-target failed at tick {}: {error}; target={}uL current={}uL",
                state.tick().value(),
                target.microliters(),
                current_hydration.microliters(),
            ),
        };
        let drank = drink
            .commit(state)
            .unwrap_or_else(|error| panic!("power project drink commit failed: {error}"));
        outcome.attention_ticks = outcome
            .attention_ticks
            .checked_add(finish_direct_consumption_work(
                registries,
                state,
                drank.completes_at(),
                "working-reserve drink",
            ))
            .unwrap_or_else(|| panic!("power project drink attention overflowed"));
        outcome.drink_actions += 1;
        outcome.drink_volume_ul = outcome
            .drink_volume_ul
            .checked_add(drank.volume().microliters())
            .unwrap_or_else(|| panic!("power project drink volume overflowed"));
    }
}

/// Restores enough reserve for the next known project leg plus an ordinary working buffer.
pub(super) fn provision_for_project_leg(
    registries: &Registries,
    state: &mut AppState,
    provisions: PowerProjectProvisions,
    required_energy: Energy,
    required_hydration: Volume,
    context: &'static str,
) -> ProvisioningOutcome {
    let physiology = registries.survival().physiology();
    let maximum_task_energy = physiology
        .maximum_metabolic_energy()
        .checked_sub(physiology.hungry_below())
        .unwrap_or_else(|| unreachable!("hunger warning is below maximum metabolic reserve"));
    let maximum_task_hydration = physiology
        .maximum_hydration()
        .checked_sub(physiology.thirsty_below())
        .unwrap_or_else(|| unreachable!("thirst warning is below maximum hydration reserve"));
    assert!(
        required_energy <= maximum_task_energy,
        "power project {context} consumes too much metabolic reserve to preserve the authored hunger warning floor"
    );
    assert!(
        required_hydration <= maximum_task_hydration,
        "power project {context} consumes too much hydration reserve to preserve the authored thirst warning floor"
    );
    let half_energy =
        Energy::from_nanojoules(physiology.maximum_metabolic_energy().nanojoules() / 2);
    let half_hydration = Volume::from_microliters(physiology.maximum_hydration().microliters() / 2);
    let recovery_energy = Energy::from_nanojoules(
        physiology
            .maximum_metabolic_energy()
            .nanojoules()
            .checked_mul(3)
            .unwrap_or_else(|| unreachable!("bounded metabolic reserve times three fits u128"))
            / 4,
    );
    let recovery_hydration = Volume::from_microliters(
        physiology
            .maximum_hydration()
            .microliters()
            .checked_mul(3)
            .unwrap_or_else(|| unreachable!("bounded hydration reserve times three fits u64"))
            / 4,
    );
    let task_energy_target = physiology
        .hungry_below()
        .checked_add(required_energy)
        .unwrap_or_else(|| unreachable!("bounded project energy plus hunger warning fits reserve"));
    let task_hydration_target = physiology
        .thirsty_below()
        .checked_add(required_hydration)
        .unwrap_or_else(|| {
            unreachable!("bounded project hydration plus thirst warning fits reserve")
        });
    let metabolic_trigger = std::cmp::max(half_energy, task_energy_target);
    let hydration_trigger = std::cmp::max(half_hydration, task_hydration_target);
    let metabolic_target = std::cmp::max(recovery_energy, task_energy_target);
    let hydration_target = std::cmp::max(recovery_hydration, task_hydration_target);
    let before = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player before provisioning"));
    assert!(
        before.vitality().parts_per_million() != 0,
        "power project {context} reached provisioning after player death; reserve=[energy:{}nJ hydration:{}uL]",
        before.metabolic_energy().nanojoules(),
        before.hydration().microliters(),
    );
    if before.metabolic_energy() >= metabolic_trigger && before.hydration() >= hydration_trigger {
        return ProvisioningOutcome::default();
    }

    let mut outcome = ProvisioningOutcome {
        stops: 1,
        ..ProvisioningOutcome::default()
    };
    drink_to_target(
        registries,
        state,
        provisions,
        hydration_target,
        context,
        &mut outcome,
    );

    while assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player while eating"))
        .metabolic_energy()
        < metabolic_target
    {
        let mut meal_plan = plan_grain_meal(registries, state, metabolic_target, context);
        loop {
            let meal_hydration = project_survival_resource_budget(
                physiology,
                SurvivalExertion::REST,
                meal_plan.duration,
            )
            .unwrap_or_else(|error| {
                panic!("power project {context} meal survival projection failed: {error:?}")
            })
            .hydration();
            let pre_meal_hydration_target = hydration_target
                .checked_add(meal_hydration)
                .unwrap_or(physiology.maximum_hydration())
                .min(physiology.maximum_hydration());
            drink_to_target(
                registries,
                state,
                provisions,
                pre_meal_hydration_target,
                context,
                &mut outcome,
            );
            let current_after_drink = assess_survival(registries, state)
                .unwrap_or_else(|| panic!("power project lost player after pre-meal drinking"));
            assert!(
                current_after_drink.metabolic_energy() < metabolic_target,
                "pre-meal drinking cannot increase metabolic energy to target"
            );
            let revised = plan_grain_meal(registries, state, metabolic_target, context);
            let duration_changed = revised.duration != meal_plan.duration;
            meal_plan = revised;
            if !duration_changed {
                break;
            }
        }
        let lot = grain_lot(registries, state, provisions.food);
        let before_meal = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project {context} lost player before eating"));
        let validated = if meal_plan.reaches_target {
            validate_eat_lot_to_metabolic_target(registries, state, lot, metabolic_target)
                .unwrap_or_else(|error| {
                    panic!("power project {context} selected-stack meal validation failed: {error}")
                })
                .unwrap_or_else(|| unreachable!("meal target remains unmet before validation"))
        } else {
            validate_eat(
                registries,
                state,
                provisions.food,
                &[MaterialLotSelection::new(lot, meal_plan.mass)],
            )
            .unwrap_or_else(|error| {
                panic!("power project {context} maximum legal meal failed: {error}")
            })
        };
        let meal = validated
            .commit(state)
            .unwrap_or_else(|error| panic!("power project meal commit failed: {error}"));
        assert_eq!(
            meal.energy_offered(),
            meal_plan.energy_offered,
            "power project {context} meal execution diverged from pre-action metabolic projection"
        );
        outcome.attention_ticks = outcome
            .attention_ticks
            .checked_add(finish_direct_consumption_work(
                registries,
                state,
                meal.completes_at(),
                "working-reserve meal",
            ))
            .unwrap_or_else(|| panic!("power project meal attention overflowed"));
        outcome.meal_actions += 1;
        outcome.meal_mass_mg = outcome
            .meal_mass_mg
            .checked_add(meal.total_mass().milligrams())
            .unwrap_or_else(|| panic!("power project meal mass overflowed"));
        let after_meal = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project {context} lost player during planned meal"));
        if meal_plan.reaches_target {
            assert!(
                after_meal.metabolic_energy() >= metabolic_target,
                "power project {context} projected meal missed metabolic target: {}nJ < {}nJ",
                after_meal.metabolic_energy().nanojoules(),
                metabolic_target.nanojoules(),
            );
        } else {
            assert!(
                after_meal.metabolic_energy() > before_meal.metabolic_energy(),
                "power project {context} maximum legal meal made no reserve progress"
            );
        }
    }
    drink_to_target(
        registries,
        state,
        provisions,
        hydration_target,
        context,
        &mut outcome,
    );
    let after = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player during provisioning"));
    assert!(
        after.vitality().parts_per_million() != 0,
        "power project {context} provisioning must finish with a live player"
    );
    assert!(
        after.metabolic_energy() >= metabolic_target,
        "power project {context} provisioning missed metabolic target: {}nJ < {}nJ",
        after.metabolic_energy().nanojoules(),
        metabolic_target.nanojoules(),
    );
    assert!(
        after.hydration() >= hydration_target,
        "power project {context} provisioning missed hydration target: {}uL < {}uL",
        after.hydration().microliters(),
        hydration_target.microliters(),
    );
    outcome
}
