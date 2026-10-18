//! A goal-free opening can develop its own problem rather than stop at first provisions.

use super::*;

/// A real post-opening decision, kept separate from the initial 1.3-minute opening snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MobileColdResponse {
    pub(super) exposed_ticks: u64,
    pub(super) projected_unheated_ticks: u64,
    pub(super) cold_comfort_ppm: u32,
    pub(super) first_fueled_tick: u64,
    pub(super) recovery_ticks: u64,
    pub(super) warm_comfort_ppm: u32,
    pub(super) without_fire_comfort_ppm: u32,
    pub(super) timber_remaining_mg: u64,
    pub(super) light_level: u8,
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

pub(super) fn evaluate_mobile_cold_response(
    registries: &Registries,
    mobile_state: &AppState,
    junction: VoxelCoord,
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
    let physiology = registries.survival().physiology();
    let warning = physiology.thermal_comfort().warning_below_ppm();
    assert_eq!(
        assess_player_overhead_cover(registries, &state),
        Some(PlayerOverheadCoverAssessment::Open),
        "mobile actor's unheated runway must use the actually observed roof exposure"
    );
    let predicted_discomfort = physiology
        .thermal_comfort()
        .ticks_until_unheated_discomfort(
            initial.thermal_comfort_ppm(),
            deep_hearth::survival::ThermalExposure::Open,
        );

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
        exposed_ticks,
        predicted_discomfort.value(),
        "player-facing unheated warmth runway must match the lived onset of discomfort"
    );
    assert_eq!(cold.thermal_comfort_state(), ThermalComfortState::Chilly);
    assert!(cold.thermal_comfort_ppm() < warning);
    let cold_comfort = cold.thermal_comfort_ppm();
    let before_intervention = state.clone();
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
        &mut state,
        stone_option.position(),
        "adaptive fire stone approach",
    );
    let stone_resource = local_surface_resource(&state, stone);
    gather(
        registries,
        &mut state,
        stone_resource,
        inputs[0].mass(),
        "adaptive fire stone collection",
    );
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        junction,
        "adaptive fire stone return",
    );
    validate_set_active_selection(
        registries,
        &state,
        Some(PlayerActiveSelection::material(stone)),
    )
    .unwrap_or_else(|error| panic!("adaptive fire stone selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("adaptive fire stone selection commit failed: {error}"));
    let upward = ViewDirection::new(0, VIEW_DIRECTION_PARTS_PER_MILLION, 0)
        .unwrap_or_else(|error| panic!("adaptive fire direction failed: {error}"));
    let assembled = advance_control_tick_with_direct_input(
        registries,
        &mut state,
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
    settle_direct_action_control_interval(registries, &mut state, "after stone assembly");
    assert_eq!(
        state.energy().get_store(fire).map(|store| store.stored()),
        Some(Energy::ZERO),
        "an unfueled stone ring must not already provide heat"
    );

    let timber_option = adjacent_surface_resource(&state, timber);
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
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
        &mut state,
        timber_resource,
        CAMPFIRE_FUEL_MASS,
        "adaptive campfire fuel",
    );
    let timber_after = state
        .available_surface_resources()
        .find(|record| record.id() == timber_resource)
        .unwrap_or_else(|| panic!("adaptive fuel source disappeared after gathering"))
        .remaining_mass();
    walk_continuous_body_adjacent_and_settle(
        registries,
        &mut state,
        junction,
        "adaptive fuel return",
    );
    validate_set_active_selection(
        registries,
        &state,
        Some(PlayerActiveSelection::material(timber)),
    )
    .unwrap_or_else(|error| panic!("adaptive fire fuel selection failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("adaptive fire fuel selection commit failed: {error}"));
    let fueled = advance_control_tick_with_direct_input(
        registries,
        &mut state,
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
    settle_direct_action_control_interval(registries, &mut state, "after fueling");
    let light = assess_local_illumination(registries, &state, junction);
    assert_eq!(light.strongest_source(), Some(fire));
    assert!(light.level() > 0);
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
        timber_before.checked_sub(CAMPFIRE_FUEL_MASS),
        Some(timber_after)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("adaptive fire matter accounting failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(validate_loaded_state(registries, &state), Ok(()));
    MobileColdResponse {
        exposed_ticks,
        projected_unheated_ticks: predicted_discomfort.value(),
        cold_comfort_ppm: cold_comfort,
        first_fueled_tick: after_fuel
            .checked_duration_since(started)
            .unwrap_or_else(|| panic!("adaptive fuel time reversed"))
            .value(),
        recovery_ticks,
        warm_comfort_ppm: after.thermal_comfort_ppm(),
        without_fire_comfort_ppm: without_fire.thermal_comfort_ppm(),
        timber_remaining_mg: timber_after.milligrams(),
        light_level: light.level(),
    }
}

pub(super) fn print_mobile_cold_response(registries: &Registries, review: MobileColdResponse) {
    std::println!(
        "PLAYER ADAPTATION opening=stay-mobile initial=comfortable/no-camp forecast=[unheated:{}t conditional-on-open-cover] trigger=chilly/{}ppm after:{}t/{} response=[stone-ring:built finite-timber-fuel:{}g local-trips:2 illumination:{}] result=[recovery:{}t recovery-start:{}t warm:{}ppm matched-without-fire:{}ppm] custody=[timber-left:{}g matter-conserved:true] mode=same-generated-world+physical-time",
        review.projected_unheated_ticks,
        review.cold_comfort_ppm,
        review.exposed_ticks,
        physical_minutes_tenths(registries, review.exposed_ticks),
        CAMPFIRE_FUEL_MASS.milligrams() / 1_000,
        review.light_level,
        review.recovery_ticks,
        review.first_fueled_tick,
        review.warm_comfort_ppm,
        review.without_fire_comfort_ppm,
        review.timber_remaining_mg / 1_000,
    );
}
