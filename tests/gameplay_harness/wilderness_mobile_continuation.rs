//! A goal-free opening can develop its own problem rather than stop at first provisions.

use super::*;
use deep_hearth::content::{
    FORM_NATIVE_METAL, MATERIAL_CLAY, MATERIAL_COPPER, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::geology::{
    FieldProspectingRequest, GeologicalEvidenceKind, player_prospecting_options,
    validate_start_field_prospecting,
};
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};

/// A real post-opening decision, kept separate from the initial 1.3-minute opening snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MobileColdResponse {
    pub(super) exposed_ticks: u64,
    pub(super) preparation_ticks: u64,
    pub(super) secured_nuts_mg: u64,
    pub(super) observed_nut_loss_mg: u64,
    pub(super) recovered_nut_mass_mg: u64,
    pub(super) recovery_route_ticks: u64,
    pub(super) nut_source_after_mg: u64,
    pub(super) planned_nut_source_after_mg: u64,
    pub(super) secured_nut_energy_nj: u128,
    pub(super) projected_unheated_ticks: u64,
    pub(super) covered_unheated_ticks: u64,
    pub(super) cold_comfort_ppm: u32,
    pub(super) first_fueled_tick: u64,
    pub(super) recovery_ticks: u64,
    pub(super) warm_comfort_ppm: u32,
    pub(super) without_fire_comfort_ppm: u32,
    pub(super) timber_remaining_mg: u64,
    pub(super) light_level: u8,
    pub(super) planned_start_tick: u64,
    pub(super) planned_fueled_tick: u64,
    pub(super) planned_comfort_ppm: u32,
    pub(super) planned_fuel_runway_ticks: u64,
    pub(super) reactive_fuel_runway_ticks: u64,
    pub(super) reconnaissance_ticks: u64,
    pub(super) reconnaissance_survey_ticks: u64,
    pub(super) reconnaissance_copper_mg: u64,
    pub(super) prepared_passive_ticks: u64,
}

/// Uses genuinely observed routes and a live field-inspection catalog while the campfire
/// works unattended. The metal is carried back; geological evidence remains acquired in the
/// same root. No pre-generated deposit position or scout schedule enters the actor's plan.
fn investigate_unattended_outcrop(
    registries: &Registries,
    state: &mut AppState,
    spawn: VoxelCoord,
    forage: VoxelCoord,
) -> (u64, u64, u64) {
    let started = state.tick();
    let copper = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    let clay = CommodityKey::new(MATERIAL_CLAY, FORM_LUMP);
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        forage,
        "reconnaissance into known forage fork",
    );
    let water = available_adjacent_drink_sources(registries, state)
        .next()
        .unwrap_or_else(|| panic!("reconnaissance cannot see the adjacent water branch"))
        .position();
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        water,
        "reconnaissance through water branch",
    );
    let outcrop = adjacent_surface_resource(state, copper).position();
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        outcrop,
        "reconnaissance toward newly seen copper",
    );
    let _ = local_surface_resource(state, clay);
    let copper_source = local_surface_resource(state, copper);
    let region = VoxelBounds::new(
        outcrop,
        VoxelCoord::new(outcrop.x() + 1, outcrop.y() + 1, outcrop.z() + 1),
    )
    .unwrap_or_else(|error| panic!("reconnaissance region failed: {error}"));
    let option = player_prospecting_options(registries, state, region)
        .unwrap_or_else(|error| panic!("reconnaissance method catalog failed: {error}"))
        .into_iter()
        .find(|candidate| candidate.method() == PROSPECTING_FIELD_INSPECTION)
        .unwrap_or_else(|| panic!("reconnaissance has no ordinary hand inspection"));
    assert!(!option.requires_equipment());
    let inspection = validate_start_field_prospecting(
        registries,
        state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("reconnaissance inspection was rejected: {error}"));
    let finishes = inspection.work().completes_at();
    let inspection_started = state.tick();
    inspection
        .commit(state)
        .unwrap_or_else(|error| panic!("reconnaissance inspection commit failed: {error}"));
    advance_to(
        registries,
        state,
        finishes,
        TickEventAllowance {
            field_prospecting: true,
            ..TickEventAllowance::default()
        },
        "unattended-camp mineral field inspection",
    );
    let survey_ticks = state.tick().value() - inspection_started.value();
    let observation = state
        .geological_knowledge()
        .observations()
        .last()
        .unwrap_or_else(|| panic!("reconnaissance produced no acquired mineral evidence"));
    assert_eq!(
        observation.evidence(),
        GeologicalEvidenceKind::SurfaceExposure
    );
    assert!(
        observation
            .finding(MATERIAL_COPPER)
            .is_some_and(|finding| finding.lower_ppm() > 0),
        "the visible native-copper float should give the player actionable local evidence"
    );
    let sample = Mass::from_milligrams(20_000);
    let copper_before = state
        .available_surface_resources()
        .find(|resource| resource.id() == copper_source)
        .unwrap_or_else(|| panic!("observed copper source disappeared"))
        .remaining_mass();
    assert!(copper_before >= sample);
    gather(
        registries,
        state,
        copper_source,
        sample,
        "reconnaissance finite copper sample",
    );
    assert_eq!(
        state
            .available_surface_resources()
            .find(|resource| resource.id() == copper_source)
            .map(|resource| resource.remaining_mass()),
        copper_before.checked_sub(sample)
    );
    for (position, label) in [
        (water, "reconnaissance return through water"),
        (forage, "reconnaissance return through forage"),
        (spawn, "reconnaissance return to protected camp route"),
    ] {
        walk_continuous_body_adjacent_and_settle(registries, state, position, label);
    }
    let carried = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(copper),
        sample
    );
    let save = serde_json::to_vec(&SaveEnvelope::new(registries, state)).unwrap();
    let restored: LoadedSaveEnvelope = serde_json::from_slice(&save).unwrap();
    assert_eq!(
        restored.into_state(registries),
        Ok(state.clone()),
        "acquired mineral evidence and the finite sample must survive reload"
    );
    (
        state.tick().value() - started.value(),
        survey_ticks,
        sample.milligrams(),
    )
}

fn settle_direct_action_control_interval(
    registries: &Registries,
    state: &mut AppState,
    context: &str,
) {
    // Ordinary body-path projections require a settled slow boundary, even after a
    // successful one-shot direct action. Advance real neutral controls, not the clock.
    while !state.control_tick().is_simulation_boundary() {
        let _ = advance_control_tick(registries, state)
            .unwrap_or_else(|error| panic!("adaptive {context} control settle failed: {error}"));
    }
}

#[derive(Clone, Copy, Debug)]
struct FireActionOutcome {
    fire: EnergyStoreId,
    timber_remaining_mg: u64,
    light_level: u8,
}

fn build_campfire_from_observed_local_sources(
    registries: &Registries,
    state: &mut AppState,
    junction: VoxelCoord,
) -> FireActionOutcome {
    let stone = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let assembly = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("mobile response has no authored fire assembly"));
    let inputs = assembly.inputs();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].commodity(), stone);

    // Use only the still-visible adjacent stone and timber sources. No starter cache,
    // extra resources, teleport, or privileged search from the fixture is permitted.
    let stone_option = adjacent_surface_resource(&state, stone);
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        stone_option.position(),
        "adaptive fire stone approach",
    );
    let stone_resource = local_surface_resource(&state, stone);
    assert_eq!(
        local_gathering_option(registries, state, stone_resource, "fire stone")
            .food_value(registries, state),
        Ok(None),
        "inedible stone cannot appear as field provisions"
    );
    gather(
        registries,
        state,
        stone_resource,
        inputs[0].mass(),
        "adaptive fire stone collection",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        junction,
        "adaptive fire stone return",
    );
    validate_set_active_selection(
        registries,
        &state,
        Some(PlayerActiveSelection::material(stone)),
    )
    .unwrap_or_else(|error| panic!("adaptive fire stone selection failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("adaptive fire stone selection commit failed: {error}"));
    let upward = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
        .unwrap_or_else(|error| panic!("adaptive fire direction failed: {error}"));
    let assembled = advance_control_tick_with_direct_input(
        registries,
        state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(upward)),
    )
    .unwrap_or_else(|error| panic!("adaptive direct fire assembly failed: {error}"));
    let fire = match assembled.secondary_action() {
        Some(SecondaryActionOutcome::EnergyStoreAssembled { definition, store }) => {
            assert_eq!(*definition, ENERGY_STONE_RING_CAMPFIRE);
            *store
        }
        other => panic!("adaptive stone selection did not assemble fire: {other:?}"),
    };
    settle_direct_action_control_interval(registries, state, "after stone assembly");
    assert_eq!(
        state.energy().get_store(fire).map(|store| store.stored()),
        Some(Energy::ZERO),
        "an unfueled stone ring must not already provide heat"
    );

    let timber_option = adjacent_surface_resource(&state, timber);
    walk_continuous_body_adjacent_and_settle(
        registries,
        state,
        timber_option.position(),
        "adaptive fuel collection approach",
    );
    let timber_resource = local_surface_resource(&state, timber);
    let timber_before = state
        .available_surface_resources()
        .find(|record| record.id() == timber_resource)
        .unwrap_or_else(|| panic!("adaptive timber opportunity disappeared"))
        .remaining_mass();
    gather(
        registries,
        state,
        timber_resource,
        CAMPFIRE_FUEL_MASS,
        "adaptive campfire fuel",
    );
    let timber_after = state
        .available_surface_resources()
        .find(|record| record.id() == timber_resource)
        .unwrap_or_else(|| panic!("adaptive fuel source disappeared after gathering"))
        .remaining_mass();
    walk_continuous_body_adjacent_and_settle(registries, state, junction, "adaptive fuel return");
    validate_set_active_selection(
        registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap_or_else(|error| panic!("adaptive fire fuel selection failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("adaptive fire fuel selection commit failed: {error}"));
    let fueled = advance_control_tick_with_direct_input(
        registries,
        state,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(upward)),
    )
    .unwrap_or_else(|error| panic!("adaptive direct fueling failed: {error}"));
    assert_eq!(
        fueled.secondary_action(),
        Some(&SecondaryActionOutcome::CombustionFuelAdded {
            store: fire,
            released: CAMPFIRE_FUEL_THERMAL_ENERGY,
        })
    );
    settle_direct_action_control_interval(registries, state, "after fueling");
    let light = assess_local_illumination(registries, &state, junction);
    assert_eq!(light.strongest_source(), Some(fire));
    assert!(light.level() > 0);
    assert_eq!(
        timber_before.checked_sub(CAMPFIRE_FUEL_MASS),
        Some(timber_after)
    );
    FireActionOutcome {
        fire,
        timber_remaining_mg: timber_after.milligrams(),
        light_level: light.level(),
    }
}

/// The same real gathered-stone and timber assembly is used when a survivor
/// chooses to invest finite firewood in deterrence rather than immediate warmth.
pub(super) fn build_scout_warding_fire(
    registries: &Registries,
    state: &mut AppState,
    junction: VoxelCoord,
) -> EnergyStoreId {
    build_campfire_from_observed_local_sources(registries, state, junction).fire
}

pub(super) fn evaluate_mobile_cold_response(
    registries: &Registries,
    mobile_state: &AppState,
    junction: VoxelCoord,
    berry_position: VoxelCoord,
    spawn: VoxelCoord,
    initial_matter: deep_hearth::core::quantity::AggregateMass,
) -> MobileColdResponse {
    let mut state = mobile_state.clone();
    let started = state.tick();
    assert_eq!(
        state.logistics().player().map(|player| player.position()),
        Some(junction),
        "mobile continuation must start where the opening actor actually stopped"
    );
    let initial = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("mobile continuation lost survival state"));
    assert_eq!(
        initial.thermal_comfort_state(),
        ThermalComfortState::Comfortable
    );
    assert!(
        state.available_local_ground_stockpiles().next().is_none(),
        "the minimalist actor should not inherit an unearned camp"
    );
    let forecast = assess_player_unheated_comfort_runway(registries, &state)
        .unwrap_or_else(|| panic!("mobile actor lacks an observable warmth forecast"));
    assert_eq!(
        forecast.overhead_cover(),
        PlayerOverheadCoverAssessment::Open,
        "mobile actor's unheated runway must use the actually observed roof exposure"
    );
    let predicted_discomfort = forecast
        .current()
        .unwrap_or_else(|| panic!("generated wilderness has no current roof truth"));
    assert_eq!(forecast.if_open(), predicted_discomfort);
    assert!(
        forecast.if_covered() > predicted_discomfort,
        "the actual player-facing forecast should expose why overhead shelter is worth considering"
    );
    let warning = registries
        .survival()
        .physiology()
        .thermal_comfort()
        .warning_below_ppm();

    // Instead of standing on the junction until the scripted thermal warning, the actor
    // makes a forward-looking choice from food actually present at its feet. Nuts are
    // scarcer, calorically denser, keep longer and diversify the berry-only ration. No
    // hidden scout schedule informs this decision. Securing one normal hand-picked batch
    // creates useful expedition provisions while leaving most of the natural source.
    let nuts = CommodityKey::new(MATERIAL_WILD_NUTS, FORM_FOOD);
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let berries_food = registries
        .survival()
        .get_food(berries)
        .unwrap_or_else(|| panic!("mobile provision decision lost edible berries"));
    let nut_resource = local_surface_resource(&state, nuts);
    let nut_option = local_gathering_option(registries, &state, nut_resource, "mobile nut choice");
    let nut_value = nut_option
        .food_value(registries, &state)
        .unwrap_or_else(|error| panic!("mobile food-value assessment failed: {error}"))
        .unwrap_or_else(|| panic!("mobile gathering catalog omitted edible nut value"));
    assert!(
        matches!(
            nut_value.current_freshness(),
            deep_hearth::survival::FoodFreshness::Fresh { .. }
        ),
        "actual source history must confirm that the proposed nut ration is still edible"
    );
    assert_eq!(nut_value.category(), FoodCategory::Protein);
    assert_eq!(berries_food.category(), FoodCategory::Fruit);
    assert!(nut_value.fresh_ambient_shelf_life() > berries_food.shelf_life());
    assert_eq!(nut_value.hydration_multiplier_ppm(), 0);
    assert!(berries_food.hydration_multiplier_ppm() > 0);
    let source_before = nut_option.source_mass();
    let secured = nut_value.batch_mass();
    assert!(source_before < Mass::from_milligrams(1_500_000));
    assert!(secured <= source_before);
    let secured_nut_energy_nj = nut_value.dietary_energy().nanojoules();
    let berries_carried_energy_nj = berries_food
        .dietary_energy_for_mass(Mass::from_milligrams(500_000))
        .nanojoules();
    assert!(
        secured_nut_energy_nj > berries_carried_energy_nj,
        "one dense protein batch should meaningfully improve portable expedition energy"
    );
    gather(
        registries,
        &mut state,
        nut_resource,
        secured,
        "mobile field provisions",
    );
    assert!(
        matches!(
            nut_option.food_value(registries, &state),
            Err(deep_hearth::surface::SurfaceGatheringError::StalePlayerOption { .. })
        ),
        "already consumed gathering opportunities must not offer stale edible value"
    );
    let expected_remaining = source_before
        .checked_sub(secured)
        .unwrap_or_else(|| unreachable!("gathered batch was bounded by source"));
    assert_eq!(
        state
            .available_surface_resources()
            .find(|resource| resource.id() == nut_resource)
            .map(|resource| resource.remaining_mass()),
        Some(expected_remaining)
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        berry_position,
        "mobile field-food withdrawal",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        spawn,
        "mobile return to observed starting point",
    );
    let preparation_ticks = state
        .tick()
        .checked_duration_since(started)
        .unwrap_or_else(|| panic!("mobile provisions completion precedes opening"))
        .value();
    assert!(preparation_ticks < predicted_discomfort.value());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(state.logistics().player().unwrap().carried_stockpile())
            .map(|stockpile| stockpile.get_mass(nuts)),
        Some(secured)
    );
    let prepared_state = state.clone();

    // A new need is discovered by normal elapsed physical time. Stop on the observation
    // boundary rather than advancing an evaluator-chosen fixed number of ticks.
    let mut exposed_ticks = 0_u64;
    while assess_survival(registries, &state).is_some_and(|assessment| {
        assessment.thermal_comfort_state() == ThermalComfortState::Comfortable
    }) {
        let outcome = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("mobile exposure tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance::default(),
            "goal-free mobile exposure",
        );
        exposed_ticks += 1;
        assert!(
            exposed_ticks < registries.core().calendar().ticks_per_day(),
            "the actor did not observe changing warmth during a bounded day"
        );
    }
    let cold = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("mobile cold warning lost survival state"));
    assert_eq!(
        exposed_ticks + preparation_ticks,
        predicted_discomfort.value(),
        "player-facing unheated warmth runway must include real provision work and travel, not just idle exposure"
    );
    assert_eq!(cold.thermal_comfort_state(), ThermalComfortState::Chilly);
    assert!(cold.thermal_comfort_ppm() < warning);
    let cold_comfort = cold.thermal_comfort_ppm();
    // The pending threat and the player's acquired provisions must survive ordinary
    // persistence. This is still the same player, not a new endowed comparison fixture.
    let serialized = serde_json::to_vec(&SaveEnvelope::new(registries, &state))
        .unwrap_or_else(|error| panic!("mobile trail save encoding failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&serialized)
        .unwrap_or_else(|error| panic!("mobile trail save decoding failed: {error}"));
    let restored = loaded
        .into_state(registries)
        .unwrap_or_else(|error| panic!("mobile trail trusted continuation failed: {error}"));
    assert_eq!(
        restored, state,
        "mobile player trail state changed on save/load"
    );
    state = restored;
    let warning_at = state.tick();
    let before_intervention = state.clone();
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        berry_position,
        "mobile investigate prior provisions route",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        junction,
        "mobile revisit contested natural protein source",
    );
    let nut_source_after_mg = state
        .available_surface_resources()
        .find(|resource| resource.id() == nut_resource)
        .unwrap_or_else(|| panic!("natural nut source disappeared"))
        .remaining_mass()
        .milligrams();
    let observed_nut_loss_mg = expected_remaining
        .milligrams()
        .checked_sub(nut_source_after_mg)
        .unwrap_or_else(|| panic!("natural nut source acquired food without player intervention"));
    // The absent protein has not vanished into a counter. The actor may notice a visible
    // adjacent parcel near the scout's den, recover it through the *same automatic world
    // pickup* as any drop, and physically walk back. A missing parcel is a legitimate
    // outcome: never inspect remote kobold custody to manufacture a recovery route.
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let nuts_before_recovery = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(nuts);
    let recovery_route_started = state.tick();
    let visible_recovery =
        available_adjacent_surface_resources(&state).find(|resource| resource.commodity() == nuts);
    if let Some(visible_recovery) = visible_recovery {
        walk_continuous_body_adjacent_and_settle(
            registries,
            &mut state,
            visible_recovery.position(),
            "mobile follow visible displaced food",
        );
        walk_continuous_body_adjacent_and_settle(
            registries,
            &mut state,
            junction,
            "mobile return with reclaimed trail food",
        );
    }
    let recovered_nut_mass_mg = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .get_mass(nuts)
        .checked_sub(nuts_before_recovery)
        .unwrap_or_else(|| panic!("recovering scout food reduced carried protein"))
        .milligrams();
    let recovery_route_ticks = state
        .tick()
        .checked_duration_since(recovery_route_started)
        .unwrap_or_else(|| panic!("physical trail recovery completed before it began"))
        .value();
    assert!(
        recovered_nut_mass_mg <= observed_nut_loss_mg,
        "recovery cannot create more nuts than the unguarded source lost"
    );
    assert_eq!(
        recovered_nut_mass_mg, observed_nut_loss_mg,
        "maintained player should be able to follow and reclaim the scout's visible displaced natural food"
    );
    assert!(
        recovery_route_ticks > 0,
        "recovering visible food must occupy real travel time"
    );
    let built = build_campfire_from_observed_local_sources(registries, &mut state, junction);
    let after_fuel = state.tick();
    let mut recovery_ticks = 0_u64;
    while assess_survival(registries, &state)
        .is_some_and(|assessment| assessment.thermal_comfort_ppm() < warning)
    {
        let outcome = advance_tick(registries, &mut state)
            .unwrap_or_else(|error| panic!("adaptive fire recovery failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance::default(),
            "adaptive fire recovery",
        );
        recovery_ticks += 1;
        assert!(
            recovery_ticks < 100,
            "campfire failed to restore warmth promptly"
        );
    }
    let after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("adaptive warmed player disappeared"));
    let recovery_end = state.tick();
    let mut declined = before_intervention;
    advance_to(
        registries,
        &mut declined,
        recovery_end,
        TickEventAllowance::default(),
        "matched minimalist continuation without campfire",
    );
    let without_fire = assess_survival(registries, &declined)
        .unwrap_or_else(|| panic!("matched unfueled player disappeared"));
    assert!(after.thermal_comfort_ppm() > without_fire.thermal_comfort_ppm());
    assert!(after.thermal_comfort_ppm() >= warning);
    assert_eq!(declined.tick(), state.tick());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("adaptive fire matter accounting failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));

    // A forward-looking actor uses the original warmth forecast and the physically discovered
    // stone/timber for an early, long-lived field fire. Building while still comfortable costs
    // extra fuel but can let the player leave provisions unattended. This policy is deliberately
    // independent of the hidden scout clock: prepare now because a full fire lasts longer than
    // the projected unheated exposure, not because the evaluator knows a patrol arrival tick.
    let stone_mass = registries
        .energy()
        .get_store(ENERGY_STONE_RING_CAMPFIRE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("planned fire recipe disappeared"))
        .inputs()[0]
        .mass();
    let planned_stone_ticks = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_STONE)
        .unwrap_or_else(|| panic!("planned stone gathering method disappeared"))
        .duration_for_mass(stone_mass)
        .unwrap_or_else(|| panic!("planned stone input cannot be gathered"))
        .value();
    let planned_timber_ticks = registries
        .labor()
        .get_surface_gathering(SURFACE_GATHERING_HAND_COLLECT_TIMBER)
        .unwrap_or_else(|| panic!("planned timber gathering method disappeared"))
        .duration_for_mass(CAMPFIRE_FUEL_MASS)
        .unwrap_or_else(|| panic!("planned fire fuel cannot be gathered"))
        .value();
    let projected_fire_response_ticks = planned_stone_ticks
        .checked_add(planned_timber_ticks)
        .and_then(|ticks| ticks.checked_add(6 * SETTLED_ADJACENT_LEG_SLOW_TICKS))
        .and_then(|ticks| ticks.checked_add(3))
        .unwrap_or_else(|| panic!("planned fire-response budget overflowed"));
    assert!(
        preparation_ticks + projected_fire_response_ticks < predicted_discomfort.value(),
        "early fire should fit inside the observed current warmth runway"
    );
    let planned_start_tick = preparation_ticks;
    let mut planned = prepared_state;
    let planned_start = started
        .checked_add_span(TickSpan::new(planned_start_tick))
        .unwrap_or_else(|| panic!("planned response start overflowed"));
    advance_to(
        registries,
        &mut planned,
        planned_start,
        TickEventAllowance::default(),
        "early fire chosen from warmth and fieldwork economics",
    );
    assert_eq!(
        assess_survival(registries, &planned).map(|assessment| assessment.thermal_comfort_state()),
        Some(ThermalComfortState::Comfortable),
        "a forecast-driven player should still be comfortable when preparing the fire"
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        berry_position,
        "forecast-driven return through forage",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        junction,
        "forecast-driven return to fire materials",
    );
    let planned_built =
        build_campfire_from_observed_local_sources(registries, &mut planned, junction);
    let planned_fueled_tick = planned
        .tick()
        .checked_duration_since(started)
        .unwrap_or_else(|| panic!("planned fire fuel time reversed"))
        .value();
    assert!(
        planned_fueled_tick < preparation_ticks + exposed_ticks,
        "a conservative ordinary-action fire budget should beat the forecasted warning: planned fuel at {planned_fueled_tick}, projected warning at {}",
        preparation_ticks + exposed_ticks
    );
    assert!(
        assess_survival(registries, &planned).is_some_and(|assessment| assessment
            .thermal_comfort_state()
            == ThermalComfortState::Comfortable),
        "using the forecast should prevent becoming chilly instead of waiting for a warning"
    );
    // Withdraw completely, preserving an unattended world experiment. The scout's first
    // approach must happen with no player at the worksite: otherwise player fear, not light,
    // could explain the protected natural food. Only physical traversal is used.
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        berry_position,
        "leave prepared light unattended via forage",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        spawn,
        "leave prepared light unattended from start",
    );
    assert_eq!(
        planned.logistics().player().map(|player| player.position()),
        Some(spawn)
    );
    // Keep the fire-only comparator at the same departure instant. Unlike the
    // passive branch, this actor now converts available safe time into information
    // and a finite sample from an *observed* side route, not an invented job.
    let mut idle_with_fire = planned.clone();
    let (reconnaissance_ticks, reconnaissance_survey_ticks, reconnaissance_copper_mg) =
        investigate_unattended_outcrop(registries, &mut planned, spawn, berry_position);
    assert!(
        planned.tick() < warning_at,
        "physical reconnaissance must finish before the forecasted discomfort window"
    );
    // Keep the fire unattended throughout the scout's real foraging window. A distant
    // fire does not warm the player; return to the actual worksite when the same player
    // would otherwise become chilly, then measure the consequence of using the already
    // constructed refuge instead of starting a new fire while cold.
    let prepared_passive_ticks = warning_at.value() - planned.tick().value();
    advance_to(
        registries,
        &mut planned,
        warning_at,
        TickEventAllowance::default(),
        "prepared fire left unattended during scavenger patrol",
    );
    advance_to(
        registries,
        &mut idle_with_fire,
        warning_at,
        TickEventAllowance::default(),
        "matched passive fire-only patrol comparator",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        berry_position,
        "inspect independently protected field fire approach",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut planned,
        junction,
        "inspect independently protected protein source",
    );
    let planned_nut_source_after_mg = planned
        .available_surface_resources()
        .find(|resource| resource.id() == nut_resource)
        .unwrap_or_else(|| panic!("planned camp lost local natural nuts"))
        .remaining_mass()
        .milligrams();
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut idle_with_fire,
        berry_position,
        "fire-only counterfactual source approach",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut idle_with_fire,
        junction,
        "fire-only counterfactual source inspection",
    );
    assert_eq!(
        idle_with_fire
            .available_surface_resources()
            .find(|resource| resource.id() == nut_resource)
            .unwrap()
            .remaining_mass()
            .milligrams(),
        planned_nut_source_after_mg,
        "scout deterrence must be due to the fire, not the actor's mineral reconnaissance"
    );
    assert_eq!(
        planned_nut_source_after_mg,
        expected_remaining.milligrams(),
        "finite firelight should defend unattended food without the player standing guard"
    );
    advance_to(
        registries,
        &mut planned,
        recovery_end,
        TickEventAllowance::default(),
        "matched return to usable prepared warmth",
    );
    let planned_assessment = assess_survival(registries, &planned)
        .unwrap_or_else(|| panic!("planned warm player disappeared"));
    assert!(
        planned_assessment.thermal_comfort_ppm() > after.thermal_comfort_ppm(),
        "anticipatory fire preparation should leave the player warmer than late reaction"
    );
    let remaining_fire_ticks = |state: &AppState, fire: EnergyStoreId| {
        let remaining = state
            .energy()
            .get_store(fire)
            .unwrap_or_else(|| panic!("planned comparison lost the campfire"))
            .stored();
        passive_dissipation_ticks_until_empty(registries, ENERGY_STONE_RING_CAMPFIRE, remaining)
            .unwrap_or_else(|error| panic!("fire comparison runway failed: {error}"))
            .unwrap_or_else(|| panic!("fire comparison needs finite dissipation"))
            .value()
    };
    let reactive_fuel_runway_ticks = remaining_fire_ticks(&state, built.fire);
    let planned_fuel_runway_ticks = remaining_fire_ticks(&planned, planned_built.fire);
    assert!(
        planned_fuel_runway_ticks < reactive_fuel_runway_ticks,
        "an early fire must consume more of the same finite fuel by the matched endpoint"
    );
    assert_eq!(
        calculate_matter_accounting(&planned)
            .unwrap_or_else(|error| panic!("planned fire matter accounting failed: {error}"))
            .total(),
        initial_matter
    );
    validate_loaded_state(registries, &planned)
        .unwrap_or_else(|error| panic!("planned fire state invalid: {error}"));
    MobileColdResponse {
        exposed_ticks,
        preparation_ticks,
        secured_nuts_mg: secured.milligrams(),
        observed_nut_loss_mg,
        recovered_nut_mass_mg,
        recovery_route_ticks,
        nut_source_after_mg,
        planned_nut_source_after_mg,
        secured_nut_energy_nj,
        projected_unheated_ticks: predicted_discomfort.value(),
        covered_unheated_ticks: forecast.if_covered().value(),
        cold_comfort_ppm: cold_comfort,
        first_fueled_tick: after_fuel
            .checked_duration_since(started)
            .unwrap_or_else(|| panic!("adaptive fuel time reversed"))
            .value(),
        recovery_ticks,
        warm_comfort_ppm: after.thermal_comfort_ppm(),
        without_fire_comfort_ppm: without_fire.thermal_comfort_ppm(),
        timber_remaining_mg: built.timber_remaining_mg,
        light_level: built.light_level,
        planned_start_tick,
        planned_fueled_tick,
        planned_comfort_ppm: planned_assessment.thermal_comfort_ppm(),
        planned_fuel_runway_ticks,
        reactive_fuel_runway_ticks,
        reconnaissance_ticks,
        reconnaissance_survey_ticks,
        reconnaissance_copper_mg,
        prepared_passive_ticks,
    }
}

pub(super) fn print_mobile_cold_response(registries: &Registries, review: MobileColdResponse) {
    std::println!(
        "PLAYER ADAPTATION opening=stay-mobile forecast=[unheated-open:{}t roof-if-built:{}t no-local-heat] self-chosen-provisions:{}t passive-exposure={} trigger=chilly/{}ppm after:{}t/{} reactive=[stone-ring:built fuel:{}g trips:2 light:{} recovered:{}t fueled:{}t comfort:{}ppm no-fire:{}ppm] planned=[start:{}t fueled:{}t comfort:{}ppm fire-runway:{}t vs-reactive:{}t] recon=[active:{}t survey:{}t copper:{}mg idle-after:{}t actual-evidence:true fire-only-control:matched] custody=[timber-left:{}g matter-conserved:true] mode=same-generated-world",
        review.projected_unheated_ticks,
        review.covered_unheated_ticks,
        review.preparation_ticks,
        physical_minutes_tenths(registries, review.exposed_ticks),
        review.cold_comfort_ppm,
        review.exposed_ticks + review.preparation_ticks,
        physical_minutes_tenths(registries, review.exposed_ticks + review.preparation_ticks),
        CAMPFIRE_FUEL_MASS.milligrams() / 1_000,
        review.light_level,
        review.recovery_ticks,
        review.first_fueled_tick,
        review.warm_comfort_ppm,
        review.without_fire_comfort_ppm,
        review.planned_start_tick,
        review.planned_fueled_tick,
        review.planned_comfort_ppm,
        review.planned_fuel_runway_ticks,
        review.reactive_fuel_runway_ticks,
        review.reconnaissance_ticks,
        review.reconnaissance_survey_ticks,
        review.reconnaissance_copper_mg,
        review.prepared_passive_ticks,
        review.timber_remaining_mg / 1_000,
    );
}
