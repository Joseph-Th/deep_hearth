//! Player-directed maintenance, next-machine fabrication, and service during shared-bank delegation.

use super::*;

pub(super) fn execute_parallel_drill_bit_maintenance(
    registries: &Registries,
    state: &mut AppState,
    powered_job: deep_hearth::production::ProductionJobId,
    delegated_ticks: u64,
    project: &mut ParallelMaintenanceProject,
) -> u64 {
    let request = select_manual_craft_request(
        registries,
        state,
        PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
        project.source,
        1,
        "settlement parallel drill-bit maintenance",
    );
    let resolution = resolve_manual_craft(registries, state, &request).unwrap_or_else(|error| {
        panic!("settlement parallel maintenance resolution failed: {error}")
    });
    let attention_ticks = resolution.duration().value();
    assert!(
        attention_ticks < delegated_ticks,
        "settlement delegated spindle-drill leg no longer leaves room for useful parallel maintenance"
    );
    let powered_completion = state
        .production()
        .get_job(powered_job)
        .map(|record| record.completes_at().value())
        .unwrap_or_else(|| panic!("settlement delegated spindle-drill job disappeared"));
    let maintenance_job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, project.destination),
    )
    .unwrap_or_else(|error| panic!("settlement parallel maintenance start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("settlement parallel maintenance commit failed: {error}"));

    let mut completed = false;
    for elapsed in 1..=attention_ticks {
        let outcome = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("settlement parallel maintenance tick failed: {error}"));
        assert_tick_events_within(
            &outcome,
            TickEventAllowance {
                production_jobs: &[powered_job, maintenance_job],
                ..TickEventAllowance::default()
            },
            "settlement parallel maintenance",
        );
        assert!(
            outcome
                .production_completions()
                .iter()
                .all(|completion| completion.job() != powered_job),
            "settlement delegated spindle-drill leg ended before the parallel maintenance task"
        );
        if outcome
            .production_completions()
            .iter()
            .any(|completion| completion.job() == maintenance_job)
        {
            assert_eq!(
                elapsed, attention_ticks,
                "settlement parallel maintenance completed before its canonical schedule"
            );
            completed = true;
        }
    }
    assert!(completed, "settlement parallel maintenance never completed");
    assert!(
        state.production().get_job(powered_job).is_some(),
        "settlement delegated spindle-drill job did not remain active through maintenance prep"
    );
    let remaining = powered_completion
        .checked_sub(state.tick().value())
        .unwrap_or_else(|| panic!("settlement powered job completion moved behind current time"));
    assert_eq!(
        remaining,
        delegated_ticks - attention_ticks,
        "settlement autonomous work did not advance while the player prepared maintenance stock"
    );
    assert_eq!(state.player_work().active(), None);
    let drill_bit = CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT);
    let expected_drill_bit_mass = resolution
        .single_output_stream()
        .unwrap_or_else(|| panic!("settlement parallel maintenance lost its single output stream"))
        .outputs()
        .iter()
        .filter(|output| output.commodity() == drill_bit)
        .map(|output| output.mass())
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("settlement parallel maintenance output mass overflowed"));
    assert!(!expected_drill_bit_mass.is_zero());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(project.destination)
            .map(|stockpile| stockpile.get_mass(drill_bit)),
        Some(expected_drill_bit_mass),
        "settlement parallel maintenance did not produce its usable service component"
    );
    project.completed = true;
    attention_ticks
}

/// Prepare the next actual machine's precision die while the previous machine is producing.
/// Both jobs use normal scheduling and local material custody. The next drawbench cannot be
/// assembled before the player has finished this work.
pub(super) fn prepare_next_machine_during_delegation(
    registries: &Registries,
    state: &mut AppState,
    powered_job: deep_hearth::production::ProductionJobId,
    project: &mut ParallelMachineProject,
) -> u64 {
    assert!(
        !project.completed,
        "next-machine preparation must execute only once"
    );
    let request = select_manual_craft_request(
        registries,
        state,
        PROCESS_SHAPE_STONE_DRAWPLATE,
        project.source,
        1,
        "parallel next-machine drawplate",
    );
    let resolution = resolve_manual_craft(registries, state, &request)
        .unwrap_or_else(|error| panic!("next-machine manual work resolution failed: {error}"));
    let duration = resolution.duration().value();
    let completes_at = state
        .production()
        .get_job(powered_job)
        .unwrap_or_else(|| panic!("next-machine preparation lost delegated machine job"))
        .completes_at()
        .value();
    assert!(
        state
            .tick()
            .value()
            .checked_add(duration)
            .is_some_and(|end| end < completes_at),
        "first delegated production leg must leave genuine time to prepare the next machine"
    );
    let manual_job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, project.components),
    )
    .unwrap_or_else(|error| panic!("next-machine drawing-die craft failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("next-machine die craft commit failed: {error}"));
    let mut completed = false;
    for elapsed in 1..=duration {
        let tick = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("next-machine fabrication tick failed: {error}"));
        assert_tick_events_within(
            &tick,
            TickEventAllowance {
                production_jobs: &[powered_job, manual_job],
                ..TickEventAllowance::default()
            },
            "parallel next-machine fabrication",
        );
        assert!(
            tick.production_completions()
                .iter()
                .all(|event| event.job() != powered_job),
            "first machine must still be working when the next machine's die is ready"
        );
        if tick
            .production_completions()
            .iter()
            .any(|event| event.job() == manual_job)
        {
            assert_eq!(elapsed, duration);
            completed = true;
        }
    }
    assert!(
        completed,
        "next-machine die did not finish during delegated work"
    );
    let made = state
        .inventory()
        .get_stockpile(project.components)
        .unwrap_or_else(|| panic!("drawbench part stockpile vanished"))
        .get_mass(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE));
    let required = registries
        .equipment()
        .get_equipment(EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("next-machine assembly disappeared"))
        .inputs()
        .iter()
        .filter(|input| input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))
        .map(|input| input.mass())
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("next-machine die mass overflowed"));
    assert_eq!(
        made, required,
        "delegated time must produce precisely the needed machine component"
    );
    assert_eq!(state.player_work().active(), None);
    project.completed = true;
    duration
}

/// Execute a matched service alternative, using the bit prepared while the machine ran.
/// The actual player can keep the bit for future demand instead of servicing a healthy idle
/// machine. This branch uses the ordinary pickup, service catalog, and timed service owner.
pub(super) fn service_spindle_from_parallel_bit(
    registries: &Registries,
    state: &mut AppState,
    spindle: EquipmentId,
    project: &ParallelMaintenanceProject,
    outcome: &mut LivedPortfolioOutcome,
) {
    assert!(
        project.completed,
        "spindle service requires the parallel-prepared replacement"
    );
    let drill_bit = CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT);
    let stock = project_material_stacks(registries, state, project.destination)
        .unwrap_or_else(|error| panic!("parallel service bit catalog failed: {error}"));
    let bit = stock
        .iter()
        .find(|stack| stack.commodity() == drill_bit)
        .unwrap_or_else(|| panic!("parallel-prepared bit cannot be picked up"));
    let bit_mass = bit.mass();
    validate_pickup_stack_from_ground(registries, state, project.destination, bit, bit_mass)
        .unwrap_or_else(|error| panic!("parallel bit pickup failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("parallel bit pickup commit failed: {error}"));
    let service_option = player_equipment_maintenance_options(registries, state)
        .unwrap_or_else(|error| panic!("parallel service catalog failed: {error}"))
        .into_iter()
        .find(|option| option.equipment() == spindle)
        .unwrap_or_else(|| {
            panic!("spindle wear did not produce a player-visible service opportunity")
        });
    assert!(
        service_option.replacement_ready(),
        "parallel-prepared drill bit must fund actual spindle service"
    );
    assert_eq!(service_option.replacement(), drill_bit);
    assert!(service_option.required_replacement_mass() <= bit_mass);
    let service = validate_start_player_equipment_maintenance(registries, state, spindle)
        .unwrap_or_else(|error| panic!("parallel-funded spindle service rejected: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("parallel-funded spindle service commit failed: {error}"));
    assert_eq!(
        service.condition_before(),
        service_option.condition_before()
    );
    assert_eq!(
        service.target_condition(),
        service_option.target_condition()
    );
    let started = state.tick().value();
    let mut completion = None;
    while state.tick() < service.completes_at() {
        let tick = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("parallel-funded service tick failed: {error}"));
        assert_tick_events_within(
            &tick,
            TickEventAllowance {
                equipment_maintenance: true,
                ..TickEventAllowance::default()
            },
            "parallel-funded spindle service",
        );
        if let Some(done) = tick.equipment_maintenance() {
            assert!(
                completion.replace(done).is_none(),
                "spindle service completed repeatedly"
            );
        }
    }
    let completion =
        completion.unwrap_or_else(|| panic!("parallel-funded spindle service never completed"));
    assert_eq!(completion.equipment(), spindle);
    assert_eq!(
        completion.condition_after(),
        service_option.target_condition()
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(spindle)
            .map(|record| record.condition()),
        Some(service_option.target_condition())
    );
    let restored = state.tick().value() - started;
    assert_eq!(restored, service_option.duration().value());
    outcome.service_attention_ticks = restored;
    outcome.serviced_spindle_condition_before_ppm =
        service_option.condition_before().parts_per_million();
    outcome.serviced_spindle_condition_after_ppm =
        service_option.target_condition().parts_per_million();
    outcome.service_used_parallel_bit = true;
}
