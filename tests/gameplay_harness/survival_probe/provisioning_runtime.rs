//! Live survival provisioning observation, planning, and decision-point preparation.

use super::*;

pub(super) struct ProvisioningPlan {
    pub(super) selections: Vec<MaterialLotSelection>,
}

pub(super) fn maximum_direct_provisioning_ticks(registries: &Registries) -> u64 {
    let direct = registries.survival().physiology().direct_consumption();
    let meal_ticks = direct
        .meal_duration(direct.maximum_meal_mass())
        .unwrap_or_else(|| panic!("authored maximum meal has no direct-consumption duration"))
        .value();
    let drink_ticks = direct
        .drink_duration(direct.maximum_drink_volume())
        .unwrap_or_else(|| panic!("authored maximum drink has no direct-consumption duration"))
        .value();
    meal_ticks
        .checked_add(drink_ticks)
        .unwrap_or_else(|| panic!("survival direct-provisioning horizon overflowed"))
}

#[derive(Clone, Copy)]
pub(super) struct ObservedProvisioningFood {
    pub(super) lot: MaterialLotId,
    pub(super) food: FoodDefinition,
    pub(super) mass: Mass,
}

pub(super) fn observed_provisioning_foods(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
) -> Vec<ObservedProvisioningFood> {
    assert!(
        state.inventory().get_stockpile(source).is_some(),
        "survival actor food source disappeared before observation"
    );
    state
        .inventory()
        .lot_ids(source)
        .filter_map(|lot| {
            let record = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| unreachable!("stockpile lot index references a live lot"));
            let food = registries
                .survival()
                .get_food(record.commodity())
                .copied()?;
            if !matches!(
                assess_food_freshness(registries, state, lot),
                Ok(FoodFreshness::Fresh { .. })
            ) {
                return None;
            }
            Some(ObservedProvisioningFood {
                lot,
                food,
                mass: record.mass(),
            })
        })
        .collect()
}

pub(super) fn observed_provisioning_drink(
    registries: &Registries,
    state: &AppState,
) -> LocalDrinkSource {
    let sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    assert_eq!(
        sources.len(),
        1,
        "survival actor requires exactly one locally observable usable drink source at the provisioning decision point"
    );
    sources[0]
}

pub(super) fn provisioning_plan(
    registries: &Registries,
    prepared: &PreparedProvisioningWorld,
    policy: DietProvisioningPolicy,
) -> ProvisioningPlan {
    let physiology = registries.survival().physiology();
    let before = assess_survival(registries, &prepared.state)
        .unwrap_or_else(|| panic!("survival provisioning plan lost the player"));
    // Reconstruct the decision set from admitted runtime state plus the live registry. The
    // generated world is setup history, not actor input.
    let available = observed_provisioning_foods(registries, &prepared.state, prepared.ambient_meal);
    let available_foods = available
        .iter()
        .map(|observed| observed.food)
        .collect::<Vec<_>>();
    let selected_available_indices = selected_food_indices(&available_foods, policy);
    assert!(!selected_available_indices.is_empty());
    assert!(
        before.metabolic_energy() <= physiology.maximum_metabolic_energy(),
        "survival provisioning energy exceeded authored maximum"
    );
    let desired_masses = desired_policy_meal_masses(
        registries,
        &prepared.state,
        &available_foods,
        &selected_available_indices,
    );
    let selections = selected_available_indices
        .iter()
        .zip(desired_masses)
        .map(|(available_index, desired)| {
            let observed = available[*available_index];
            MaterialLotSelection::new(observed.lot, desired.min(observed.mass))
        })
        .collect::<Vec<_>>();
    let selected_total = selections
        .iter()
        .try_fold(Mass::ZERO, |total, selection| {
            total.checked_add(selection.mass())
        })
        .unwrap_or_else(|| panic!("survival selected meal mass overflowed"));
    assert!(
        selected_total >= physiology.direct_consumption().minimum_meal_mass(),
        "current survival food inventory must still support one legal policy meal"
    );
    ProvisioningPlan { selections }
}

pub(super) struct PreparedProvisioningWorld {
    pub(super) state: AppState,
    pub(super) ambient_meal: StockpileId,
    pub(super) preserved_witness: MaterialLotId,
    pub(super) ambient_age: u64,
    pub(super) preserved_age: u64,
    pub(super) preservation_age_saved_ticks: u64,
    pub(super) matter_total: AggregateMass,
    pub(super) fluid_total: AggregateVolume,
}

pub(super) fn prepare_provisioning_world(
    registries: &Registries,
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

    let mut state = AppState::new();
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
    super::super::exact_local_runtime::locate_stationary_endpoints(
        &mut state,
        &[ambient_meal, preserved_reserve, enclosure_material],
        &[drink_store],
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
    super::super::exact_local_runtime::initialize_stationary_player_logistics(&mut state);
    super::super::exact_local_runtime::assert_exact_local_runtime_ready(
        registries,
        &state,
        "survival provisioning",
    );
    let observed_drink = observed_provisioning_drink(registries, &state);
    assert_eq!(
        observed_drink.fluid(),
        world.drink.fluid(),
        "survival actor must rediscover the fixture-authored drink through admitted runtime state"
    );

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
        "provisioning passive exposure",
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
        preserved_witness,
        ambient_age,
        preserved_age,
        preservation_age_saved_ticks,
        matter_total,
        fluid_total,
    }
}
