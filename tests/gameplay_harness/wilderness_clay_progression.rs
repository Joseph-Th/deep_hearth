//! A cold-start path to a real material-backed settlement casting bed, without seeded stock.

use super::*;
use deep_hearth::content::{
    EQUIPMENT_CLAY_FACED_STONE_CASTING_BED, FORM_PACKED_CLAY_BINDER, MATERIAL_CLAY,
    PROCESS_PACK_CLAY_BINDER,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};

#[derive(Clone, Copy)]
struct CastingBedPlan {
    junction: VoxelCoord,
    carried: StockpileId,
    clay: CommodityKey,
    binder: CommodityKey,
    binder_batches: u64,
    stone: CommodityKey,
    stone_mass: Mass,
    timber: CommodityKey,
    timber_mass: Mass,
    boards: CommodityKey,
    board_batches: u64,
}

/// Both branches must execute exactly the same physically located acquisition,
/// transformation and assembly, not compare real work against a simulated wait.
fn complete_casting_bed_project(
    registries: &Registries,
    state: &mut AppState,
    plan: CastingBedPlan,
) -> deep_hearth::equipment::EquipmentId {
    let material = available_adjacent_surface_resources(state).collect::<Vec<_>>();
    assert_eq!(
        material
            .iter()
            .filter(|resource| {
                resource.commodity() == plan.stone || resource.commodity() == plan.timber
            })
            .count(),
        2
    );
    for (commodity, amount) in [
        (plan.stone, plan.stone_mass),
        (plan.timber, plan.timber_mass),
    ] {
        let source = material
            .iter()
            .find(|resource| resource.commodity() == commodity)
            .unwrap_or_else(|| panic!("casting-bed raw source not actor-visible"));
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            source.position(),
            "clay project material approach",
        );
        gather_in_batches(
            registries,
            state,
            source.resource(),
            amount,
            "casting-bed raw haul",
        );
        walk_continuous_body_adjacent_and_settle(
            registries,
            state,
            plan.junction,
            "clay project material return",
        );
    }
    craft_in_place(
        registries,
        state,
        plan.clay,
        plan.binder,
        plan.binder_batches,
        "casting-bed binder",
    );
    craft_in_place(
        registries,
        state,
        plan.timber,
        plan.boards,
        plan.board_batches,
        "casting-bed boards",
    );
    let option = player_equipment_assembly_options(registries, state, plan.carried)
        .unwrap_or_else(|error| panic!("casting-bed assembly catalog failed: {error}"))
        .into_iter()
        .find(|option| option.definition() == EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
        .unwrap_or_else(|| panic!("casting-bed assembly option unavailable"));
    assert!(
        option.is_ready(),
        "local gathered matter must make casting bed assembly ready"
    );
    let built = validate_assemble_player_equipment_option(registries, state, &option)
        .unwrap_or_else(|error| panic!("casting-bed assembly rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("casting-bed assembly commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(built)
            .map(|equipment| equipment.definition()),
        Some(EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(built)
            .unwrap()
            .embodied_mass(),
        option.embodied_mass()
    );
    assert_eq!(
        state.logistics().stationary_equipment_position(built),
        Some(plan.junction)
    );
    assert!(
        state
            .available_local_equipment()
            .any(|equipment| equipment.id() == built)
    );
    assert!(!state.logistics().is_equipment_carried(built));
    assert_eq!(
        assess_player_carrying(state)
            .unwrap_or_else(|| panic!("casting bed worksite lost body capacity"))
            .equipment(),
        Mass::ZERO,
        "stationary assembly must not encumber the actor as carried equipment"
    );
    built
}

pub(super) fn exercise_generated_clay_casting_bed(report: bool) {
    let registries = build_registries();
    let seed = WILDERNESS_WORLD_SEED;
    let mut state = AppState::new();
    let spawn = generated_wilderness_surface_spawn(&registries, &mut state, seed);
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("clay project generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("clay project generation commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("clay project admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("clay project admission commit failed: {error}"))
        .carried_stockpile();
    if let Some(demand) = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("clay project neighborhood demand failed: {error}"))
    {
        demand
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("clay project neighborhood commit failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("clay project initial matter audit failed: {error}"))
        .total();
    let initial_reserves = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("clay project initial survival assessment missing"));
    let began_at = state.tick();

    let clay = CommodityKey::new(MATERIAL_CLAY, FORM_LUMP);
    let binder = CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER);
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
        .unwrap_or_else(|| panic!("clay-faced casting bed definition missing"));
    let assembly = definition
        .assembly_profile()
        .unwrap_or_else(|| panic!("clay-faced casting bed lost material assembly"));
    let required = |commodity: CommodityKey| {
        assembly
            .inputs()
            .iter()
            .find(|spec| spec.commodity() == commodity)
            .unwrap_or_else(|| panic!("casting bed no longer requires commodity {commodity:?}"))
            .mass()
    };
    let binder_mass = required(binder);
    let stone_mass = required(stone);
    let boards_mass = required(boards);
    let binder_recipe = registries
        .crafting()
        .get_manual(PROCESS_PACK_CLAY_BINDER)
        .unwrap_or_else(|| panic!("clay packing recipe disappeared"));
    assert_eq!(
        binder_recipe.input_mass(),
        binder_recipe.outputs()[0].mass()
    );
    let binder_batches = binder_mass.milligrams() / binder_recipe.outputs()[0].mass().milligrams();
    assert_eq!(
        binder_mass.milligrams() % binder_recipe.outputs()[0].mass().milligrams(),
        0
    );
    let boards_recipe = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("bare-hand board recipe disappeared"));
    let board_yield = boards_recipe
        .outputs()
        .iter()
        .find(|output| output.commodity() == boards)
        .unwrap_or_else(|| panic!("bare-hand board shaping lost board output"))
        .mass()
        .milligrams();
    let board_batches = boards_mass.milligrams().div_ceil(board_yield);
    let timber_mass =
        Mass::from_milligrams(boards_recipe.input_mass().milligrams() * board_batches);

    // Clay is physically separate from the water voxel, so direct empty-hand drinking stays
    // unambiguous. The actor learns the clay spur only after reaching the water branch.
    let forage = adjacent_surface_resource(&state, CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage.position(),
        "clay project forage approach",
    );
    let water = adjacent_drinkable_source(&registries, &state);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        water.position(),
        "clay project waterside approach",
    );
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        1
    );
    assert_eq!(state.available_surface_resources().count(), 0);
    let outcrop = adjacent_surface_resource(&state, clay);
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        outcrop.position(),
        "clay project outcrop approach",
    );
    let clay_before = state
        .available_surface_resources()
        .find(|resource| resource.id() == outcrop.resource())
        .unwrap_or_else(|| panic!("generated outcrop missing"))
        .remaining_mass();
    assert!(clay_before >= binder_mass);
    gather_in_batches(
        &registries,
        &mut state,
        outcrop.resource(),
        binder_mass,
        "casting-bed clay",
    );
    assert_eq!(
        state
            .available_surface_resources()
            .find(|resource| resource.id() == outcrop.resource())
            .map(|resource| resource.remaining_mass()),
        clay_before.checked_sub(binder_mass)
    );

    // Return to the fork and follow observed material choices, not hidden generator coordinates.
    let water_position = water.position();
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        water_position,
        "clay project clay return",
    );
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        forage.position(),
        "clay project fork return",
    );
    let junction = state
        .available_adjacent_traversal_destinations()
        .find(|position| *position != spawn && *position != water_position)
        .unwrap_or_else(|| panic!("clay project work junction not discoverable"));
    walk_continuous_body_adjacent_and_settle(
        &registries,
        &mut state,
        junction,
        "clay project work junction",
    );
    let plan = CastingBedPlan {
        junction,
        carried,
        clay,
        binder,
        binder_batches,
        stone,
        stone_mass,
        timber,
        timber_mass,
        boards,
        board_batches,
    };
    let project_start = state.tick();
    let initial_warmth_runway = assess_player_unheated_comfort_runway(&registries, &state)
        .unwrap_or_else(|| panic!("clay project needs a visible exposure forecast"))
        .current()
        .unwrap_or_else(|| panic!("clay project lacks generated-world roof truth"))
        .value();
    let crafting_floor = binder_batches * binder_recipe.duration().value()
        + board_batches * boards_recipe.duration().value();
    // Estimate the complete minimum labor requirement from production-owned
    // gather durations and current recipe durations, before executing either
    // branch. A partial crafting-only budget conceals most of the actual
    // outdoor project, producing a false "no need for a fire" conclusion.
    let gathering_floor: u64 = [
        (SURFACE_GATHERING_HAND_COLLECT_STONE, stone_mass),
        (SURFACE_GATHERING_HAND_COLLECT_TIMBER, timber_mass),
    ]
    .into_iter()
    .map(|(method, mut remaining)| {
        let definition = registries
            .labor()
            .get_surface_gathering(method)
            .unwrap_or_else(|| panic!("casting-bed gathering definition disappeared"));
        let mut ticks = 0;
        while !remaining.is_zero() {
            let mass = remaining.min(definition.maximum_batch_mass());
            ticks += definition
                .duration_for_mass(mass)
                .unwrap_or_else(|| panic!("casting-bed gathering mass is unrepresentable"))
                .value();
            remaining = remaining.checked_sub(mass).unwrap();
        }
        ticks
    })
    .sum();
    let projected_work_floor = crafting_floor + gathering_floor;
    // Travel and interaction admission only increase this lower bound.
    let prepare_fire = projected_work_floor >= initial_warmth_runway;
    // Run the matched alternative even when another seed or retuning makes
    // pre-work fire unnecessary. The actor's *choice* is this observed
    // forecast, not an evaluator demand that every world need a fire.
    let mut prepared = state.clone();
    let prepared_fire_started = prepared.tick();
    let fire = mobile_continuation::build_scout_warding_fire(&registries, &mut prepared, junction);
    let fire_preparation_ticks = prepared.tick().value() - prepared_fire_started.value();
    let prepared_bed = complete_casting_bed_project(&registries, &mut prepared, plan);
    let prepared_finish_tick = prepared.tick();
    let built = complete_casting_bed_project(&registries, &mut state, plan);
    let finished_unprotected = state.clone();
    let bed_mass = state
        .equipment()
        .get_equipment(built)
        .unwrap()
        .embodied_mass();
    let unprotected_project_ticks = state.tick().value() - project_start.value();
    let prepared_project_ticks =
        prepared_finish_tick.value() - prepared_fire_started.value() - fire_preparation_ticks;
    assert_eq!(
        prepared_project_ticks, unprotected_project_ticks,
        "the fire comparison must perform the same full work, not an idle surrogate"
    );
    advance_to(
        &registries,
        &mut state,
        prepared_finish_tick,
        TickEventAllowance::default(),
        "matched exposed casting-bed worksite",
    );
    let exposed_comfort = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("matched exposed worker disappeared"))
        .thermal_comfort_ppm();
    let protected_comfort = assess_survival(&registries, &prepared)
        .unwrap_or_else(|| panic!("matched fire-prepared worker disappeared"))
        .thermal_comfort_ppm();
    assert!(
        protected_comfort > exposed_comfort,
        "physically fueled camp heat must protect the worker during the same long project"
    );
    assert_eq!(
        prepared
            .logistics()
            .stationary_equipment_position(prepared_bed),
        Some(junction)
    );
    assert!(
        prepared
            .energy()
            .get_store(fire)
            .is_some_and(|record| !record.stored().is_zero()),
        "the prepared branch must retain a finite and genuinely burning fire"
    );
    assert_eq!(
        calculate_matter_accounting(&prepared).unwrap().total(),
        matter_before,
        "the fire and workshop must conserve their extra physical construction and fuel"
    );
    assert_eq!(validate_loaded_state(&registries, &prepared), Ok(()));
    let prepared_fire_runway = passive_dissipation_ticks_until_empty(
        &registries,
        ENERGY_STONE_RING_CAMPFIRE,
        prepared
            .energy()
            .get_store(fire)
            .unwrap_or_else(|| panic!("prepared worksite lost its fire"))
            .stored(),
    )
    .unwrap_or_else(|error| panic!("prepared-worksite fire runway invalid: {error}"))
    .unwrap_or_else(|| panic!("prepared-worksite combustion cannot run forever"))
    .value();
    let fire_stone_mass = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .and_then(|definition| definition.assembly_profile())
        .and_then(|assembly| assembly.inputs().first())
        .unwrap_or_else(|| panic!("stone campfire has no physical frame"))
        .mass();
    let prepared_saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &prepared))
        .unwrap_or_else(|error| panic!("fire-prepared worksite save failed: {error}"));
    let prepared_loaded: LoadedSaveEnvelope = serde_json::from_slice(&prepared_saved)
        .unwrap_or_else(|error| panic!("fire-prepared worksite decode failed: {error}"));
    assert_eq!(
        prepared_loaded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("fire-prepared worksite trusted load failed: {error}")),
        prepared,
        "physically prepared fire and stationary workshop must survive replay"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("clay project final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("clay project final state invalid: {error}"));
    // The real actor follows the risk forecast. The other branch remains a
    // matched counterfactual, not the default result of an arbitrarily fixed
    // evaluator path. The initial casting-bed goal is still externally given.
    let chosen_state = if prepare_fire {
        &prepared
    } else {
        &finished_unprotected
    };
    let chosen_assessment = assess_survival(&registries, chosen_state)
        .unwrap_or_else(|| panic!("chosen casting-bed actor disappeared"));
    if prepare_fire {
        assert!(
            chosen_assessment.thermal_comfort_ppm() > exposed_comfort,
            "the selected infrastructure investment must avert the observed cold exposure"
        );
    }
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, chosen_state))
        .unwrap_or_else(|error| panic!("clay project save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved)
        .unwrap_or_else(|error| panic!("clay project decode failed: {error}"));
    assert_eq!(
        decoded
            .into_state(&registries)
            .unwrap_or_else(|error| panic!("clay project trusted load failed: {error}")),
        *chosen_state,
        "chosen fire preparation and physically built workstation must persist exactly"
    );
    if report {
        let final_reserves = assess_survival(&registries, chosen_state)
            .unwrap_or_else(|| panic!("clay project final survival assessment missing"));
        let elapsed = chosen_state.tick().value() - began_at.value();
        let energy_used = initial_reserves
            .metabolic_energy()
            .checked_sub(final_reserves.metabolic_energy())
            .unwrap_or_else(|| panic!("clay project gained unexplained metabolic energy"));
        let hydration_used = initial_reserves
            .hydration()
            .checked_sub(final_reserves.hydration())
            .unwrap_or_else(|| panic!("clay project gained unexplained hydration"));
        std::println!(
            "WILDERNESS CASTING BED seed=0x{:016X} evidence=generated-exact-local goal=specified-workstation choice=[pre-work-fire:{}] path=[forage>water>clay>fork>junction>stone>junction>timber>junction] elapsed={}t/{} survival-spent=[energy:{}nJ hydration:{}uL] reserves-left=[energy:{}nJ hydration:{}uL] pressure=[hungry:{} thirsty:{}] materials=[clay:{}mg stone:{}mg timber:{}mg] craft=[binder:{}b boards:{}b] worksite=[auto-placed-at-player:true stationary-mass:{}mg carried-mass:0mg output-produced:false] payoff=deferred-until-molten-metal clay-left={}mg matter=conserved",
            seed.value(),
            prepare_fire,
            elapsed,
            physical_minutes_tenths(&registries, elapsed),
            energy_used.nanojoules(),
            hydration_used.microliters(),
            final_reserves.metabolic_energy().nanojoules(),
            final_reserves.hydration().microliters(),
            final_reserves.metabolic_energy() <= registries.survival().physiology().hungry_below(),
            final_reserves.hydration() <= registries.survival().physiology().thirsty_below(),
            binder_mass.milligrams(),
            stone_mass.milligrams(),
            timber_mass.milligrams(),
            binder_batches,
            board_batches,
            bed_mass.milligrams(),
            clay_before
                .checked_sub(binder_mass)
                .unwrap_or_else(|| unreachable!())
                .milligrams(),
        );
        std::println!(
            "PLAYER PROJECT TRADEOFF seed=0x{:016X} episode=coverage/rested/clay-worksite goal=specified-not-discovered phase=prepare-vs-push-through observation=[remaining-warmth:{}t predicted-project-work-at-least:{}t] policy=[choose-prework-fire:{}] action=[same-acquisition-and-crafting:{}t both-branches] investment=[extra-fire-preparation:{}t stone:{}mg timber-fuel:{}mg remaining-runway:{}t] matched-at={}t consequence=[prepared-comfort:{}ppm exposed-comfort:{}ppm] benefit=[comfort-delta:+{}ppm] productive-output=[stationary-casting-bed:9.6kg metal-cast:0] next-dependency=obtain-molten-metal matter=conserved replay=exact",
            seed.value(),
            initial_warmth_runway,
            projected_work_floor,
            prepare_fire,
            unprotected_project_ticks,
            fire_preparation_ticks,
            fire_stone_mass.milligrams(),
            CAMPFIRE_FUEL_MASS.milligrams(),
            prepared_fire_runway,
            prepared_finish_tick.value(),
            protected_comfort,
            exposed_comfort,
            protected_comfort - exposed_comfort,
        );
    }
}
