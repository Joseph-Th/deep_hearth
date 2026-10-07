//! Deterministic survival provisioning world construction and decision-point preparation.

use super::*;

pub(in super::super) struct ProvisioningWorld {
    pub(in super::super) start_profile: SurvivalStartProfile,
    pub(in super::super) foods: Vec<FoodDefinition>,
    pub(in super::super) offered_masses: Vec<Mass>,
    pub(in super::super) witness_index: usize,
    pub(in super::super) preserved_reserve_mass: Mass,
    pub(in super::super) inherited_preservation_definition: StorageDefinitionId,
    pub(in super::super) inherited_preservation_multiplier_ppm: u32,
    pub(in super::super) age_ticks: u64,
    pub(in super::super) provisioning_wait_ticks: u64,
    pub(super) drink: DrinkDefinition,
}

pub(in super::super) fn minimum_visible_preservation_age_ticks(
    preservation_multiplier_ppm: u32,
) -> u64 {
    let ambient_preservation_ppm =
        u64::from(StockpileStorageProfile::unbounded_solid_only().preservation_multiplier_ppm());
    let preservation = u64::from(preservation_multiplier_ppm);
    assert!(
        preservation > ambient_preservation_ppm,
        "preservation witness requires a rate strictly better than ambient"
    );
    // Choose the first whole elapsed tick n for which even the conservative rounded preserved
    // age is at most n-1 ambient ticks:
    // n * ambient / preservation <= n - 1.
    preservation.div_ceil(preservation - ambient_preservation_ppm)
}

pub(in super::super) fn provisioning_world(
    registries: &Registries,
    seed: u64,
) -> ProvisioningWorld {
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
    // Supply margin spans genuine scarcity through oversupply so organic worlds exercise
    // both tight provisioning and comfortable reserves. The seed-derived band runs from
    // 80% (must stretch food, preservation matters) to 130% (comfortable buffer).
    let supply_margin_ppm = 800_000 + (mix64(seed ^ 0x5355_5050_4C59_4D47) % 500_001) as u32;
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
    let preservation_capacities = preserving_storage
        .iter()
        .map(|candidate| candidate.capacity.milligrams())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let maximum_preservation_capacity = Mass::from_milligrams(
        *preservation_capacities
            .last()
            .unwrap_or_else(|| unreachable!("preservation candidates are nonempty")),
    );
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
    // A four-case exploratory sample deliberately spans the current authored capacity frontier so
    // preservation is experienced both as a choice-rich investment and as genuine bulk pressure.
    // High seed entropy still varies the exact reserve inside each capacity band.
    let requested_band = usize::try_from(seed & 0b11)
        .unwrap_or_else(|_| unreachable!("two-bit preservation stratum fits usize"));
    let requested_capacity_index = requested_band
        .checked_mul(preservation_capacities.len().saturating_sub(1))
        .map(|scaled| scaled / 3)
        .unwrap_or_else(|| unreachable!("bounded preservation capacity index cannot overflow"));
    let minimum_reserve_mg = minimum_reserve_mass.milligrams();
    let capacity_index = if preservation_capacities[requested_capacity_index] >= minimum_reserve_mg
    {
        requested_capacity_index
    } else {
        preservation_capacities
            .iter()
            .position(|capacity| *capacity >= minimum_reserve_mg)
            .unwrap_or_else(|| unreachable!("witness filtering guarantees a fitting enclosure"))
    };
    let reserve_upper_mg = preservation_capacities[capacity_index];
    let reserve_lower_mg = if capacity_index == 0 {
        minimum_reserve_mg
    } else {
        preservation_capacities[capacity_index - 1]
            .checked_add(1)
            .unwrap_or_else(|| panic!("preservation capacity-band lower bound overflowed"))
            .max(minimum_reserve_mg)
    };
    assert!(reserve_lower_mg <= reserve_upper_mg);
    let reserve_span = reserve_upper_mg - reserve_lower_mg;
    let reserve_offset = if reserve_span == 0 {
        0
    } else {
        mix64(seed ^ 0x5052_4553_5253_5256) % (reserve_span + 1)
    };
    let preserved_reserve_mass = Mass::from_milligrams(reserve_lower_mg + reserve_offset);
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
            // A rested actor should experience ordinary reserve drawdown, not be forced to idle
            // all the way into an emergency. Warning-boundary worlds below cover urgent response;
            // this branch instead exercises proactive provisioning after a meaningful fraction of
            // a day. Keeping the window bounded also avoids spending most survival-report runtime
            // simulating uneventful passive ticks.
            let base = (ticks_per_day / 4).max(1);
            let jitter = (ticks_per_day / 4).max(1);
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
    let minimum_age_ticks =
        minimum_visible_preservation_age_ticks(inherited_preservation_multiplier_ppm);
    let age_limit = (witness_food.shelf_life().value() / 4)
        .max(1)
        .min(provisioning_wait_ticks.saturating_sub(1).max(1));
    assert!(
        age_limit >= minimum_age_ticks,
        "survival preservation witness has no room for a visibly different preserved age: limit={age_limit}t minimum={minimum_age_ticks}t multiplier={inherited_preservation_multiplier_ppm}ppm"
    );
    // Inherited food age spans the full freshness range: young parcels, mid-life stores,
    // and parcels near the quarter-shelf boundary, but never an age so young that fixed-point
    // rounding makes preserved and ambient exposure observationally identical.
    let age_span = age_limit - minimum_age_ticks;
    let age_ticks =
        minimum_age_ticks + mix64(seed ^ 0x4147_455F_464F_4F44) % age_span.saturating_add(1);
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
