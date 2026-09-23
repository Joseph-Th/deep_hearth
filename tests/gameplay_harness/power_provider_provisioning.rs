//! Canonical carried-provision recovery for long human-power projects.

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_lot, seed_stockpile};
use deep_hearth::content::{
    FLUID_WATER, FORM_FOOD, MATERIAL_GRAIN, STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST,
};
use deep_hearth::core::quantity::{Energy, Mass, Volume};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::SimulationTick;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::{
    MaterialLotSelection, StockpileId, StockpileStorageProfile, validate_build_storage_enclosure,
};
use deep_hearth::labor::PlayerWork;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::survival::{
    DrinkHydrationProjectionError, SurvivalExertion, assess_survival,
    project_minimum_drink_to_hydration_target, project_minimum_meal_to_metabolic_target,
    project_survival_resource_budget, validate_drink, validate_eat,
};

use super::super::environment::ROOM_TEMPERATURE;

#[derive(Clone, Copy)]
pub(super) struct PowerProjectProvisions {
    pub(super) food: StockpileId,
    pub(super) water: FluidStoreId,
    pub(super) food_supply_mg: u64,
    pub(super) food_preservation_ppm: u32,
    pub(super) water_supply_ul: u64,
}

pub(super) fn seed_power_project_provisions(
    registries: &Registries,
    state: &mut AppState,
) -> PowerProjectProvisions {
    // This is an onsite project cache, not an encumbrance model. Prefer the double-wall chest for
    // long mechanized campaigns: its 20 kg capacity is materially more useful than the stronger
    // but only 8 kg insulated pantry, while 3x preservation still covers the generated horizons.
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
    pub(super) meal_energy_nj: u128,
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
        self.meal_energy_nj = self
            .meal_energy_nj
            .checked_add(other.meal_energy_nj)
            .unwrap_or_else(|| panic!("power project meal energy overflowed"));
        self.attention_ticks = self
            .attention_ticks
            .checked_add(other.attention_ticks)
            .unwrap_or_else(|| panic!("power project provisioning attention overflowed"));
    }
}

fn finish_direct_consumption(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
    context: &'static str,
) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("power project {context} has no active direct consumption"));
    assert!(matches!(
        active,
        PlayerWork::Eating { .. } | PlayerWork::Drinking { .. }
    ));
    let ticks = completes_at
        .value()
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("power project {context} completion precedes now"));
    assert!(ticks > 0);
    for elapsed in 1..=ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("power project {context} tick failed: {error}"));
        assert!(
            outcome.production_availability_changes().is_empty()
                && outcome.production_completions().is_empty()
                && outcome.ready_mining_jobs().is_empty()
                && outcome.manual_power().is_none()
                && outcome.storage_enclosure_dismantling().is_none()
                && outcome.field_prospecting().is_none(),
            "power project {context} crossed unrelated observable work during provisioning"
        );
        if elapsed < ticks {
            assert_eq!(state.player_work().active(), Some(active));
        } else {
            assert_eq!(state.player_work().active(), None);
        }
    }
    ticks
}

fn recovery_drink_volume(registries: &Registries, state: &AppState, target: Volume) -> Volume {
    let current = assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project lost player before drink planning"))
        .hydration();
    let drink = *registries
        .survival()
        .get_drink(FLUID_WATER)
        .unwrap_or_else(|| panic!("power project water lost authored drink definition"));
    match project_minimum_drink_to_hydration_target(
        registries.survival().physiology(),
        drink,
        current,
        target,
    ) {
        Ok(Some(projection)) => projection.volume(),
        Ok(None) => Volume::ZERO,
        Err(DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
            maximum_drink_volume,
        }) => maximum_drink_volume,
        Err(error) => panic!("power project drink projection failed: {error}"),
    }
}

fn grain_selection(state: &AppState, stockpile: StockpileId, mass: Mass) -> MaterialLotSelection {
    let grain = CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD);
    let lot = state
        .inventory()
        .lot_ids(stockpile)
        .find(|lot| {
            state
                .inventory()
                .get_lot(*lot)
                .is_some_and(|record| record.commodity() == grain && record.mass() >= mass)
        })
        .unwrap_or_else(|| {
            let remaining = state
                .inventory()
                .get_stockpile(stockpile)
                .map(|record| record.get_mass(grain).milligrams())
                .unwrap_or(0);
            panic!(
                "power project provisions lack {}mg of grain at tick {} with {}mg remaining",
                mass.milligrams(),
                state.tick().value(),
                remaining,
            )
        });
    MaterialLotSelection::new(lot, mass)
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
        let volume = recovery_drink_volume(registries, state, target);
        assert!(!volume.is_zero());
        let current_hydration = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project {context} lost player before drinking"))
            .hydration();
        let drank = validate_drink(registries, state, provisions.water, volume)
            .unwrap_or_else(|error| {
                let current = assess_survival(registries, state)
                    .unwrap_or_else(|| panic!("power project lost player before failed drink"));
                panic!(
                    "power project {context} drink validation failed at tick {}: {error}; target={}uL current={}uL requested={}uL reserve=[energy:{}nJ hydration:{}uL vitality:{}ppm]",
                    state.tick().value(),
                    target.microliters(),
                    current_hydration.microliters(),
                    volume.microliters(),
                    current.metabolic_energy().nanojoules(),
                    current.hydration().microliters(),
                    current.vitality().parts_per_million(),
                )
            })
            .commit(state)
            .unwrap_or_else(|error| panic!("power project drink commit failed: {error}"));
        outcome.attention_ticks = outcome
            .attention_ticks
            .checked_add(finish_direct_consumption(
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

    let grain = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("power project grain lost authored food definition"));
    if assess_survival(registries, state)
        .unwrap_or_else(|| panic!("power project {context} lost player while eating"))
        .metabolic_energy()
        < metabolic_target
    {
        let current = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project lost player before meal planning"));
        let mut meal_projection = project_minimum_meal_to_metabolic_target(
            physiology,
            grain,
            current.metabolic_energy(),
            metabolic_target,
        )
        .unwrap_or_else(|error| {
            panic!("power project {context} meal target projection failed: {error}")
        })
        .unwrap_or_else(|| unreachable!("meal planning runs only below the metabolic target"));
        loop {
            let meal_hydration = project_survival_resource_budget(
                physiology,
                SurvivalExertion::REST,
                meal_projection.duration(),
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
            let revised = project_minimum_meal_to_metabolic_target(
                physiology,
                grain,
                current_after_drink.metabolic_energy(),
                metabolic_target,
            )
            .unwrap_or_else(|error| {
                panic!("power project {context} revised meal projection failed: {error}")
            })
            .unwrap_or_else(|| {
                unreachable!("pre-meal drinking cannot increase metabolic energy to target")
            });
            let duration_changed = revised.duration() != meal_projection.duration();
            meal_projection = revised;
            if !duration_changed {
                break;
            }
        }
        let mass = meal_projection.mass();
        let selection = grain_selection(state, provisions.food, mass);
        let meal = validate_eat(registries, state, provisions.food, &[selection])
            .unwrap_or_else(|error| {
                let current = assess_survival(registries, state)
                    .unwrap_or_else(|| panic!("power project lost player before failed meal"));
                panic!(
                    "power project {context} meal validation failed: {error}; reserve=[energy:{}nJ hydration:{}uL vitality:{}ppm]",
                    current.metabolic_energy().nanojoules(),
                    current.hydration().microliters(),
                    current.vitality().parts_per_million(),
                )
            })
            .commit(state)
            .unwrap_or_else(|error| panic!("power project meal commit failed: {error}"));
        assert_eq!(
            meal.energy_offered(),
            meal_projection.energy_offered(),
            "power project {context} meal execution diverged from pre-action metabolic projection"
        );
        outcome.attention_ticks = outcome
            .attention_ticks
            .checked_add(finish_direct_consumption(
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
        outcome.meal_energy_nj = outcome
            .meal_energy_nj
            .checked_add(meal.energy_offered().nanojoules())
            .unwrap_or_else(|| panic!("power project meal energy overflowed"));
        let after_meal = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("power project {context} lost player during planned meal"));
        assert!(
            after_meal.metabolic_energy() >= metabolic_target,
            "power project {context} projected meal missed metabolic target: {}nJ < {}nJ",
            after_meal.metabolic_energy().nanojoules(),
            metabolic_target.nanojoules(),
        );
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
