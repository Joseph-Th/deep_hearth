//! Real generated-world clay, stone, wood and fire feed one irreversible kiln firing.

use super::*;
use deep_hearth::content::{
    ENERGY_STONE_CERAMIC_COOLING_BED, ENERGY_STONE_PIT_KILN_HEARTH, EQUIPMENT_STONE_PIT_KILN,
    FORM_FIRED_CROCK, FORM_GREENWARE_CROCK, FORM_PACKED_CLAY_BINDER, MATERIAL_CLAY,
    PROCESS_COOL_FIRED_CLAY_CROCK, PROCESS_FIRE_CLAY_CROCK, STORAGE_CARVED_STONE_PROVISIONS_CROCK,
    STORAGE_FIRED_CERAMIC_PROVISIONS_CROCK,
};
use deep_hearth::core::quantity::Temperature;
use deep_hearth::energy::{validate_assemble_energy_store, validate_combust_fuel};
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::logistics::validate_pickup_equipment;
use deep_hearth::production::validate_start_player_process;
use deep_hearth::thermal::{
    SensibleCoolingRequest, SensibleHeatingRequest, SensibleHeatingResolutionError,
    resolve_sensible_cooling_process, resolve_sensible_heating_process,
};

pub(super) fn generated_world_clay_becomes_a_fired_crock_with_wood_fuel() {
    let registries = build_registries();
    let ceramic_definition = registries
        .storage()
        .get(STORAGE_FIRED_CERAMIC_PROVISIONS_CROCK)
        .unwrap();
    let stone_definition = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap();
    let body_mass = ceramic_definition.assembly_profile().input_mass();
    assert!(body_mass < stone_definition.assembly_profile().input_mass());
    assert!(
        ceramic_definition
            .storage_profile()
            .preservation_multiplier_ppm()
            >= stone_definition
                .storage_profile()
                .preservation_multiplier_ppm()
                + 1_000_000,
        "firing must yield a real preservation advantage over hand-carved stone"
    );
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, WILDERNESS_WORLD_SEED);
    validate_generate_initial_wilderness(&registries, &state, WILDERNESS_WORLD_SEED, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state).unwrap() {
        demand.commit(&mut state).unwrap();
    }
    let initial_matter = calculate_matter_accounting(&state).unwrap().total();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let forage = adjacent_surface_resource(&state, berries).position();
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, forage, "kiln forage leg");
    let water = adjacent_drinkable_source(&registries, &state).position();
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, water, "kiln water leg");
    let clay = CommodityKey::new(MATERIAL_CLAY, FORM_LUMP);
    let clay_spur = adjacent_surface_resource(&state, clay);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        clay_spur.position(),
        "kiln clay outcrop",
    );
    gather_in_batches(
        &registries,
        &mut state,
        clay_spur.resource(),
        Mass::from_milligrams(3_000_000),
        "kiln clay harvest",
    );
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, water, "kiln water return");
    walk_continuous_body_adjacent_and_settle(&registries, &mut state, forage, "kiln fork return");
    let junction = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != spawn && *position != water)
        .unwrap();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "kiln work junction",
    );
    for (commodity, mass, label) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(6_000_000),
            "kiln stone haul",
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(3_000_000),
            "kiln fuel and frame haul",
        ),
    ] {
        let opportunity = adjacent_surface_resource(&state, commodity);
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            opportunity.position(),
            label,
        );
        gather_in_batches(&registries, &mut state, opportunity.resource(), mass, label);
        walk_continuous_body_adjacent_and_settle(
            &registries,
            &mut state,
            junction,
            "kiln workshop return",
        );
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK);
    let binder = CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER);
    craft_in_place(
        &registries,
        &mut state,
        clay,
        green,
        1,
        "shape unfired vessel",
    );
    craft_in_place(&registries, &mut state, clay, binder, 2, "pack kiln lining");
    craft_in_place(
        &registries,
        &mut state,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        1,
        "shape kiln frame",
    );
    let kiln = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PIT_KILN, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_pickup_equipment(&registries, &state, kiln)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let hearth =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_PIT_KILN_HEARTH, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    let cooling_bed = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_STONE_CERAMIC_COOLING_BED,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let log_before = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG));
    validate_combust_fuel(&registries, &state, hearth, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        log_before.checked_sub(CAMPFIRE_FUEL_MASS).unwrap()
    );
    let available_heat = state.energy().get_store(hearth).unwrap().stored();
    assert_eq!(available_heat, CAMPFIRE_FUEL_THERMAL_ENERGY);
    let green_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == carried && lot.commodity() == green)
        .unwrap()
        .id();
    let selection = [MaterialLotSelection::new(green_lot, body_mass)];
    let request = |target| {
        SensibleHeatingRequest::new(
            PROCESS_FIRE_CLAY_CROCK,
            carried,
            &selection,
            kiln,
            hearth,
            target,
        )
    };
    let before_bad_target = state.clone();
    let insufficient_target = Temperature::from_millikelvin(900_000);
    assert_eq!(
        resolve_sensible_heating_process(&registries, &state, request(insufficient_target))
            .unwrap_err(),
        SensibleHeatingResolutionError::BelowFiringTemperature {
            required: Temperature::from_millikelvin(1_100_000),
            actual: insufficient_target,
        }
    );
    assert_eq!(state, before_bad_target);
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        request(Temperature::from_millikelvin(1_100_000)),
    )
    .unwrap();
    let active = validate_start_player_process(
        &registries,
        &state,
        resolved.process_resolution(),
        carried,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let completes_at = state.production().get_job(active).unwrap().completes_at();
    advance_to(
        &registries,
        &mut state,
        completes_at,
        TickEventAllowance {
            production_jobs: &[active],
            ..TickEventAllowance::default()
        },
        "kiln ceramic firing",
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(green),
        Mass::ZERO
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(fired),
        body_mass
    );
    assert!(
        state.energy().get_store(hearth).unwrap().stored() < available_heat,
        "firing must consume reserved timber-derived heat"
    );
    let hot_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == carried && lot.commodity() == fired)
        .unwrap();
    assert_eq!(
        hot_lot.temperature(),
        Temperature::from_millikelvin(1_100_000)
    );
    let hot_selection = [MaterialLotSelection::new(hot_lot.id(), body_mass)];
    let cooling = resolve_sensible_cooling_process(
        &registries,
        &state,
        SensibleCoolingRequest::new(
            PROCESS_COOL_FIRED_CLAY_CROCK,
            carried,
            &hot_selection,
            kiln,
            cooling_bed,
        ),
    )
    .unwrap();
    let recovery_ticks = cooling.process_resolution().duration().value();
    assert!(recovery_ticks > 0);
    let cooling_energy = cooling.released_energy();
    assert!(cooling_energy.nanojoules() > 0);
    let cooled_job = validate_start_player_process(
        &registries,
        &state,
        cooling.process_resolution(),
        carried,
        carried,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let cool_end = state
        .production()
        .get_job(cooled_job)
        .unwrap()
        .completes_at();
    advance_to(
        &registries,
        &mut state,
        cool_end,
        TickEventAllowance {
            production_jobs: &[cooled_job],
            ..TickEventAllowance::default()
        },
        "kiln ceramic cooldown",
    );
    let cooled_lot = state
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == carried && lot.commodity() == fired)
        .unwrap();
    assert_eq!(cooled_lot.mass(), body_mass);
    assert_eq!(
        cooled_lot.temperature(),
        Temperature::from_millikelvin(293_150)
    );
    assert!(
        state
            .energy()
            .get_store(cooling_bed)
            .unwrap()
            .stored()
            .nanojoules()
            > 0,
        "cooling must retain thermal energy for later dissipation"
    );
    let vessel = registries
        .storage()
        .get(STORAGE_FIRED_CERAMIC_PROVISIONS_CROCK)
        .unwrap();
    assert_eq!(vessel.assembly_profile().inputs()[0].commodity(), fired);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    std::println!(
        "PLAYER CERAMICS evidence=generated-opening resource=[clay:3kg stone:6kg timber:3kg] work=[shape-greenware pack-lining assemble-kiln assemble-hearth assemble-cooling-bed burn-log fire-vessel cool-vessel] gate=[900K:rejected 1100K:accepted] payoff=[fired-body:{}mg versus-stone:{}mg sealed-shelf:{}ppm versus-stone:{}ppm] cooldown=[ticks:{recovery_ticks} heat-received-nJ:{} room-temperature:true] matter=conserved persistence=validated",
        body_mass.milligrams(),
        stone_definition
            .assembly_profile()
            .input_mass()
            .milligrams(),
        ceramic_definition
            .storage_profile()
            .preservation_multiplier_ppm(),
        stone_definition
            .storage_profile()
            .preservation_multiplier_ppm(),
        cooling_energy.nanojoules(),
    );
}
