//! Production-backed ceramic workshop and unattended settlement regressions.

use super::*;
use deep_hearth::content::{
    FORM_FIRED_CROCK, FORM_GREENWARE_CROCK, FORM_LOG, MATERIAL_CLAY, MATERIAL_WOOD,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::material::CommodityKey;

fn workshop() -> Session {
    workshop_with_tools(false)
}

fn workshop_with_tools(mining: bool) -> Session {
    let mut session = Session::new(24318).unwrap();
    // The same visible stone, timber and clay deposits used in ordinary opening play.
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 3000",
        "wait work",
        "gather 1 3000",
        "wait work",
    ]
    .into_iter()
    .chain(
        mining
            .then_some(["gather 1 2000", "wait work"])
            .into_iter()
            .flatten(),
    )
    .chain([
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 3000",
        "wait work",
    ])
    .chain(
        mining
            .then_some(["gather 1 2000", "wait work"])
            .into_iter()
            .flatten(),
    )
    .chain([
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 3000",
        "wait work",
        "craft 101",
        "wait work",
        "craft 61 2",
        "wait work",
        "craft 14",
        "wait work",
    ])
    .chain(
        mining
            .then_some([
                "craft 7 2",
                "wait work",
                "craft 8 2",
                "wait work",
                "assemble 21",
                "take 1",
                "assemble 6",
                "take 1",
            ])
            .into_iter()
            .flatten(),
    )
    .chain(["assemble 61", "build-heat 16", "build-heat 17"])
    {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    session
}

// Select by observed physical cohort, never by a hidden material lot ID.
fn stack_index(session: &Session, key: CommodityKey) -> usize {
    project_player_inventory(&session.registries, &session.state)
        .unwrap()
        .carried()
        .stacks()
        .iter()
        .position(|stack| stack.commodity() == key)
        .unwrap()
        + 1
}

fn fueled_workshop() -> Session {
    let mut session = workshop();
    let fuel = format!(
        "feed-heat 1 {}",
        stack_index(&session, CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
    );
    session.command(&fuel).unwrap();
    session
}

#[test]
fn ambiguous_campfire_and_kiln_use_lists_both_real_fueling_commands() {
    use deep_hearth::content::{ENERGY_STONE_PIT_KILN_HEARTH, ENERGY_STONE_RING_CAMPFIRE};
    use deep_hearth::core::quantity::Energy;

    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 3000",
        "wait work",
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 3000",
        "wait work",
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "craft 61",
        "wait work",
        "build-fire",
        "build-heat 16",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let fuel = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let fire = session
        .state
        .available_local_energy_stores()
        .find(|store| store.definition() == ENERGY_STONE_RING_CAMPFIRE)
        .unwrap()
        .id();
    let hearth = session
        .state
        .available_local_energy_stores()
        .find(|store| store.definition() == ENERGY_STONE_PIT_KILN_HEARTH)
        .unwrap()
        .id();
    let stack = stack_index(&session, fuel);
    session.command(&format!("select {stack}")).unwrap();
    let report = session.command("use up").unwrap().0;
    assert!(report.contains("2 local"), "{report}");
    assert!(
        report.contains("feed-fire") && report.contains("feed-heat"),
        "{report}"
    );
    assert_eq!(
        session.state.energy().get_store(fire).unwrap().stored(),
        Energy::ZERO
    );
    assert_eq!(
        session.state.energy().get_store(hearth).unwrap().stored(),
        Energy::ZERO
    );
    assert!(session.command("fire").unwrap().0.contains("feed-fire 1"));
    assert!(
        session
            .command("thermal")
            .unwrap()
            .0
            .contains("feed-heat 1")
    );
    session
        .command(&format!("feed-fire 1 {}", stack_index(&session, fuel)))
        .unwrap();
    session
        .command(&format!("feed-heat 1 {}", stack_index(&session, fuel)))
        .unwrap();
    assert!(session.state.energy().get_store(fire).unwrap().stored() > Energy::ZERO);
    assert!(session.state.energy().get_store(hearth).unwrap().stored() > Energy::ZERO);
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn unattended_completion_does_not_hide_a_simultaneous_gathering_receipt() {
    use deep_hearth::core::time::{ControlTick, SimulationTick};
    use deep_hearth::simulation::advance_neutral_control_span;
    use std::num::NonZeroU64;

    let mut session = fueled_workshop();
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let firing = format!("heat 102 {} 1 1", stack_index(&session, green));
    session.command(&firing).unwrap();
    let due = session
        .state
        .production()
        .jobs()
        .next()
        .unwrap()
        .completes_at();
    let mass = Mass::from_milligrams(100_000);
    let option = player_surface_gathering_options(&session.registries, &session.state).unwrap()[0];
    let remaining_before = option.source_mass();
    let duration = option.duration_for_mass(mass).unwrap();
    let gather_at =
        ControlTick::from_simulation_tick(SimulationTick::new(due.value() - duration.value()))
            .unwrap();
    while session.state.control_tick() < gather_at {
        let budget =
            NonZeroU64::new(gather_at.value() - session.state.control_tick().value()).unwrap();
        let _ =
            advance_neutral_control_span(&session.registries, &mut session.state, budget).unwrap();
    }
    let report = session.command("gather 1 100 repeat 2").unwrap().0;
    assert!(report.contains("Thermal batch completed:"), "{report}");
    assert!(
        report.contains("Gather repeat: 1 batches completed."),
        "{report}"
    );
    assert!(
        report.contains("interrupted after 1 completed batches"),
        "{report}"
    );
    assert_eq!(session.state.tick(), due);
    assert!(session.state.player_work().active().is_none());
    assert_eq!(
        option
            .refresh(&session.registries, &session.state)
            .unwrap()
            .unwrap()
            .source_mass(),
        remaining_before.checked_sub(mass).unwrap()
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn unattended_stationary_output_is_announced_only_at_its_local_custody() {
    use deep_hearth::content::{ENERGY_STONE_PIT_KILN_HEARTH, PROCESS_FIRE_CLAY_CROCK};
    use deep_hearth::inventory::MaterialLotSelection;
    use deep_hearth::logistics::validate_allocate_player_ground_stockpile;
    use deep_hearth::production::validate_start_player_process;
    use deep_hearth::thermal::{SensibleHeatingRequest, resolve_sensible_heating_process};

    let mut session = fueled_workshop();
    let source = session
        .state
        .logistics()
        .player()
        .unwrap()
        .carried_stockpile();
    let destination = validate_allocate_player_ground_stockpile(
        &session.registries,
        &session.state,
        Mass::from_milligrams(10_000_000),
    )
    .unwrap()
    .commit(&mut session.state)
    .unwrap();
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let selections = session
        .state
        .inventory()
        .lot_ids(source)
        .filter_map(|id| {
            let lot = session.state.inventory().get_lot(id).unwrap();
            (lot.commodity() == green).then(|| MaterialLotSelection::new(id, lot.mass()))
        })
        .collect::<Vec<_>>();
    assert!(!selections.is_empty());
    let equipment = equipment::local_stationary(&session.state)[0];
    let energy = session
        .state
        .available_local_energy_stores()
        .find(|store| store.definition() == ENERGY_STONE_PIT_KILN_HEARTH)
        .unwrap()
        .id();
    let target = session
        .registries
        .thermal()
        .get_sensible_heating(PROCESS_FIRE_CLAY_CROCK)
        .unwrap()
        .firing_conversion()
        .unwrap()
        .minimum_temperature();
    let resolved = resolve_sensible_heating_process(
        &session.registries,
        &session.state,
        SensibleHeatingRequest::new(
            PROCESS_FIRE_CLAY_CROCK,
            source,
            &selections,
            equipment,
            energy,
            target,
        ),
    )
    .unwrap();
    let job = validate_start_player_process(
        &session.registries,
        &session.state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut session.state)
    .unwrap();
    let before_inventory = snapshot(&session);
    let projected = project_player_inventory(&session.registries, &session.state).unwrap();
    let (index, local) = projected
        .local_containers()
        .iter()
        .enumerate()
        .find(|(_, stockpile)| stockpile.stockpile() == destination)
        .unwrap();
    assert_eq!(local.stored(), Mass::ZERO);
    assert_eq!(local.reserved_inbound(), Mass::from_milligrams(1_600_000));
    let inventory = session.command("inventory").unwrap().0;
    assert!(
        inventory.contains(&format!(
            "Local storage {}: open ground stockpile | 0 g / 10000 g | 1600 g reserved inbound | 8400 g available",
            index + 1,
        )),
        "{inventory}"
    );
    assert_eq!(snapshot(&session), before_inventory);
    let storage = session.command("storage").unwrap().0;
    assert!(
        storage.contains(&format!(
            "{}. open ground stockpile | 0 g / 10000 g | 1600 g reserved inbound | 8400 g available",
            index + 1,
        )),
        "{storage}"
    );
    assert_eq!(snapshot(&session), before_inventory);
    let at = session.player_position().unwrap();
    let due = session
        .state
        .production()
        .get_job(job)
        .unwrap()
        .completes_at();
    let mut remote = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot(&session))
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    remote.command("trek w 1 steady").unwrap();
    assert_ne!(remote.player_position().unwrap(), at);
    assert!(remote.state.tick() < due);
    let wait_for_output = |session: &mut Session| {
        let mut output = String::new();
        for _ in 0..200 {
            if session.state.production().get_job(job).is_none() {
                return output;
            }
            output.push_str(&session.command("wait 7200").unwrap().0);
        }
        panic!("unattended job did not finish");
    };
    let local_report = wait_for_output(&mut session);
    let remote_report = wait_for_output(&mut remote);
    assert!(
        local_report.contains("Thermal batch completed:"),
        "{local_report}"
    );
    assert!(local_report.contains("(local storage)"), "{local_report}");
    assert_eq!(session.state.tick(), due);
    let settled = snapshot(&session);
    let storage = session.command("storage").unwrap().0;
    assert!(
        storage.contains(&format!(
            "{}. open ground stockpile | 1600 g / 10000 g | 0 g reserved inbound | 8400 g available",
            index + 1,
        )),
        "{storage}"
    );
    assert_eq!(snapshot(&session), settled);
    assert!(
        !remote_report.contains("Thermal batch completed:"),
        "{remote_report}"
    );
    assert!(
        !remote_report.contains("Received 1600 g"),
        "{remote_report}"
    );
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK);
    for state in [&session.state, &remote.state] {
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap()
                .get_mass(fired),
            Mass::from_milligrams(1_600_000)
        );
        assert_eq!(
            deep_hearth::core::state::validate_loaded_state(&session.registries, state),
            Ok(())
        );
    }
}

#[test]
fn delayed_mining_output_is_announced_only_at_its_local_custody() {
    use deep_hearth::content::{FORM_ORE, MATERIAL_COPPER, MINING_METHOD_HAND_PICK};
    use deep_hearth::core::time::{ControlTick, SimulationTick};
    use deep_hearth::logistics::validate_allocate_player_ground_stockpile;
    use deep_hearth::mining::{MiningTargetRequest, resolve_mining_target, validate_start_mining};
    use deep_hearth::simulation::advance_neutral_control_span;
    use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
    use std::num::NonZeroU64;

    let mut session = workshop_with_tools(true);
    session.command("prospect copper 2 with 1").unwrap();
    session.command("wait work").unwrap();
    session.command("wait boundary").unwrap();
    let origin = session.player_position().unwrap();
    let destination = validate_allocate_player_ground_stockpile(
        &session.registries,
        &session.state,
        Mass::from_milligrams(10_000_000),
    )
    .unwrap()
    .commit(&mut session.state)
    .unwrap();
    let target = resolve_mining_target(
        &session.state,
        MiningTargetRequest::new(
            VoxelBounds::new(
                origin,
                VoxelCoord::new(origin.x() + 1, origin.y() + 1, origin.z() + 1),
            )
            .unwrap(),
            MATERIAL_COPPER,
        ),
    )
    .unwrap();
    let tool = project_player_inventory(&session.registries, &session.state)
        .unwrap()
        .equipment()[1]
        .equipment();
    let mass = Mass::from_milligrams(200_000);
    let mut trial = serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot(&session))
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    let trial_job = validate_start_mining(
        &session.registries,
        &trial,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        tool,
        mass,
    )
    .unwrap()
    .commit(&mut trial)
    .unwrap();
    let duration = trial
        .mining()
        .get_job(trial_job)
        .unwrap()
        .completes_at()
        .value()
        - trial.tick().value();

    let fuel = format!(
        "feed-heat 1 {}",
        stack_index(&session, CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
    );
    session.command(&fuel).unwrap();
    let firing = format!(
        "heat 102 {} 1 1",
        stack_index(
            &session,
            CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK)
        )
    );
    session.command(&firing).unwrap();
    let due = session
        .state
        .production()
        .jobs()
        .next()
        .unwrap()
        .completes_at();
    let mining_at =
        ControlTick::from_simulation_tick(SimulationTick::new(due.value() - duration)).unwrap();
    assert!(
        mining_at >= session.state.control_tick(),
        "firing must outlast mining"
    );
    while session.state.control_tick() < mining_at {
        let budget =
            NonZeroU64::new(mining_at.value() - session.state.control_tick().value()).unwrap();
        let _ =
            advance_neutral_control_span(&session.registries, &mut session.state, budget).unwrap();
    }
    let job = validate_start_mining(
        &session.registries,
        &session.state,
        MINING_METHOD_HAND_PICK,
        target,
        destination,
        tool,
        mass,
    )
    .unwrap()
    .commit(&mut session.state)
    .unwrap();
    assert_eq!(
        session.state.mining().get_job(job).unwrap().completes_at(),
        due
    );
    let mut report = String::new();
    while session.state.player_work().active().is_some() {
        report.push_str(&session.command("wait work").unwrap().0);
    }
    assert!(report.contains("Thermal batch completed:"), "{report}");
    assert!(!report.contains("Mining produced:"), "{report}");
    assert!(
        session
            .state
            .mining()
            .get_job(job)
            .unwrap()
            .is_awaiting_settlement()
    );
    assert_eq!(session.state.tick(), due);

    let mut remote = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot(&session))
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let mut remote_report = remote.command("trek w 1 steady").unwrap().0;
    assert_ne!(remote.player_position().unwrap(), origin, "{remote_report}");
    let wait_for_output = |session: &mut Session| {
        let mut output = String::new();
        for _ in 0..10 {
            if session.state.mining().get_job(job).is_none() {
                return output;
            }
            output.push_str(&session.command("wait 720").unwrap().0);
        }
        panic!("deferred mining did not settle");
    };
    let local_report = wait_for_output(&mut session);
    remote_report.push_str(&wait_for_output(&mut remote));
    let ore = CommodityKey::new(MATERIAL_COPPER, FORM_ORE);
    for state in [&session.state, &remote.state] {
        assert_eq!(
            state
                .inventory()
                .get_stockpile(destination)
                .unwrap()
                .get_mass(ore),
            mass
        );
        assert_eq!(
            deep_hearth::core::state::validate_loaded_state(&session.registries, state),
            Ok(())
        );
    }
    assert!(
        local_report.contains("Mining produced: 200 g copper ore"),
        "{local_report}"
    );
    assert_eq!(session.state.tick().value(), due.value() + 1);
    assert!(
        !remote_report.contains("Mining produced:"),
        "{remote_report}"
    );
    assert!(
        remote.state.tick().value() > due.value() + 1,
        "unseen settlement must not interrupt a wait: {remote_report}"
    );
}

#[test]
fn ceramic_workshop_fires_and_cools_the_gathered_player_stack() {
    let mut session = workshop();
    let before = snapshot(&session);
    let (catalog, _) = session.command("thermal").unwrap();
    assert!(
        catalog.contains("fueled stone pit-kiln hearth"),
        "{catalog}"
    );
    assert!(catalog.contains("stone ceramic cooling bed"), "{catalog}");
    assert!(
        catalog.contains("Fuel: up to 1000 g timber log per charge"),
        "{catalog}"
    );
    assert!(
        catalog.contains(
            "fueled stone pit-kiln hearth | 0.000 MJ stored / 24.000 MJ capacity | extinguished"
        ),
        "{catalog}"
    );
    assert!(
        catalog.contains(
            "stone ceramic cooling bed | 0.000 MJ stored / 6.000 MJ capacity | heat sink"
        ),
        "{catalog}"
    );
    assert!(catalog.contains("826.85 C target"), "{catalog}");
    assert!(
        catalog.contains("unfired") || catalog.contains("Stack"),
        "{catalog}"
    );
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK);
    let firing = format!("heat 102 {} 1 1", stack_index(&session, green));
    let error = session.command(&firing).unwrap_err();
    assert!(
        error.contains("0.000 MJ available; batch requires 1.161864 MJ"),
        "{error}"
    );
    assert!(catalog.contains(&error), "{catalog}");
    assert_eq!(snapshot(&session), before);
    let fuel = format!(
        "feed-heat 1 {}",
        stack_index(&session, CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
    );
    session.command(&fuel).unwrap();
    let firing = format!("heat 102 {} 1 1", stack_index(&session, green));
    let (catalog, _) = session.command("thermal").unwrap();
    assert!(catalog.contains(&format!("{firing} | ready")), "{catalog}");
    assert!(catalog.contains("passive burn remaining"), "{catalog}");

    // A real jump on the slow boundary leaves the feet in the workshop's
    // voxel for a few control samples. Locality alone must not authorize
    // loading a batch whose output is reserved in the moving player's pack.
    let mut airborne = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot(&session))
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    while airborne.state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK
        != CONTROL_TICKS_PER_SIMULATION_TICK - 1
    {
        let _ = deep_hearth::simulation::advance_control_tick(
            &airborne.registries,
            &mut airborne.state,
        )
        .unwrap();
    }
    let origin = airborne.player_position().unwrap();
    airborne.command("jump").unwrap();
    assert!(airborne.state.control_tick().is_simulation_boundary());
    assert_eq!(airborne.player_position().unwrap(), origin);
    assert!(
        airborne
            .state
            .logistics()
            .player()
            .unwrap()
            .body()
            .vertical_velocity()
            .nanometers_per_second()
            > 0
    );
    let before = snapshot(&airborne);
    let (catalog, _) = airborne.command("thermal").unwrap();
    assert!(catalog.contains("fixed footing"), "{catalog}");
    let error = airborne.command(&firing).unwrap_err();
    assert!(error.contains("fixed footing"), "{error}");
    assert_eq!(snapshot(&airborne), before);

    session.command(&firing).unwrap();
    let before_inventory = snapshot(&session);
    let pending = project_player_inventory(&session.registries, &session.state).unwrap();
    let carried = pending.carried();
    assert_eq!(carried.reserved_inbound(), Mass::from_milligrams(1_600_000));
    let inventory = session.command("inventory").unwrap().0;
    assert!(
        inventory.contains(&format!(
            "Carried matter: {} g / {} g | 1600 g reserved inbound | {} g available",
            view::grams(carried.stored()),
            view::grams(carried.capacity()),
            view::grams(carried.available()),
        )),
        "{inventory}"
    );
    assert_eq!(snapshot(&session), before_inventory);
    let finish = |session: &mut Session| {
        let due = session
            .state
            .production()
            .jobs()
            .next()
            .unwrap()
            .completes_at();
        let mut transcript = String::new();
        for _ in 0..200 {
            let (report, _) = session.command("wait 7200").unwrap();
            transcript.push_str(&report);
            if session.state.production().jobs().next().is_none() {
                assert_eq!(session.state.tick(), due, "{report}");
                assert!(report.contains("Thermal batch completed:"), "{report}");
                assert!(report.contains("Received 1600 g"), "{report}");
                return transcript;
            }
        }
        panic!("thermal batch did not complete");
    };
    finish(&mut session);
    let settled_inventory = session.command("inventory").unwrap().0;
    let settled = project_player_inventory(&session.registries, &session.state).unwrap();
    assert_eq!(settled.carried().reserved_inbound(), Mass::ZERO);
    assert!(
        settled_inventory.contains(&format!(
            "Carried matter: {} g / {} g | 0 g reserved inbound | {} g available",
            view::grams(settled.carried().stored()),
            view::grams(settled.carried().capacity()),
            view::grams(settled.carried().available()),
        )),
        "{settled_inventory}"
    );
    let hot = project_player_inventory(&session.registries, &session.state).unwrap();
    let hot = &hot.carried().stacks()[stack_index(&session, fired) - 1];
    assert_eq!(hot.mass(), Mass::from_milligrams(1_600_000));
    assert_eq!(hot.temperature(), Temperature::from_millikelvin(1_100_000));
    session.command("wait boundary").unwrap();
    let cooling = format!("heat 104 {} 1 2", stack_index(&session, fired));
    let before_cooling = snapshot(&session);
    let (catalog, _) = session.command("thermal").unwrap();
    assert!(
        catalog.contains(&format!(
            "Stack {}: 1600 g | 826.85 C",
            stack_index(&session, fired)
        )),
        "{catalog}"
    );
    assert!(catalog.contains("20 C target"), "{catalog}");
    assert!(catalog.contains(&format!("{cooling} | ready")), "{catalog}");
    assert_eq!(snapshot(&session), before_cooling);
    session.command(&cooling).unwrap();
    let saved = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    assert_eq!(finish(&mut session), finish(&mut restored));
    assert_eq!(snapshot(&session), snapshot(&restored));
    let cooled = project_player_inventory(&session.registries, &session.state).unwrap();
    let cooled = &cooled.carried().stacks()[stack_index(&session, fired) - 1];
    assert_eq!(cooled.mass(), Mass::from_milligrams(1_600_000));
    assert_eq!(cooled.temperature(), Temperature::from_millikelvin(293_150));
    session.command("place-store 10").unwrap();
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}
