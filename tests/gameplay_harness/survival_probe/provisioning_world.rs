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
    const AMBIENT_PRESERVATION_PPM: u64 = 1_000_000;
    let preservation = u64::from(preservation_multiplier_ppm);
    assert!(
        preservation > AMBIENT_PRESERVATION_PPM,
        "preservation witness requires a rate strictly better than ambient"
    );
    // Choose the first whole elapsed tick n for which even the conservative rounded preserved
    // age is at most n-1 ambient ticks:
    // n * ambient / preservation <= n - 1.
    preservation.div_ceil(preservation - AMBIENT_PRESERVATION_PPM)
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
            // Full-reserve starts live a full day so canonical thirst reaches the authored
            // warning boundary mid-wait. The actor must then reprovision during lived time
            // (see the lived-wait checkpoint below) instead of idling through a short wait
            // that never produces real pressure.
            let base = ticks_per_day;
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
    pub(super) selected_indices: Vec<usize>,
    pub(super) selected_masses: Vec<Mass>,
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

pub(super) fn provisioning_plan(
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
    ProvisioningPlan {
        selected_indices,
        selected_masses,
    }
}

pub(super) struct PreparedProvisioningWorld {
    pub(super) state: AppState,
    pub(super) ambient_meal: StockpileId,
    pub(super) prepared_lots: Vec<MaterialLotId>,
    pub(super) preserved_witness: MaterialLotId,
    pub(super) drink_store: FluidStoreId,
    pub(super) ambient_age: u64,
    pub(super) preserved_age: u64,
    pub(super) preservation_age_saved_ticks: u64,
    pub(super) midwait_drink_count: u64,
    pub(super) midwait_drink_volume_ul: u64,
    pub(super) matter_total: AggregateMass,
    pub(super) fluid_total: AggregateVolume,
}

pub(super) fn prepare_provisioning_world(
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
    let lived_wait = advance_lived_wait(
        registries,
        &mut state,
        world,
        drink_store,
        world.provisioning_wait_ticks - world.age_ticks,
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
        midwait_drink_count: lived_wait.drinks,
        midwait_drink_volume_ul: lived_wait.drink_volume_ul,
        matter_total,
        fluid_total,
    }
}
