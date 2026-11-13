//! Scriptable play boundary regressions against actual production simulation.

use super::*;
use deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
use deep_hearth::logistics::{HorizontalControlInput, PlayerControlInput};
use deep_hearth::survival::{ThermalComfortState, assess_survival};

fn snapshot(session: &Session) -> Vec<u8> {
    serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap()
}

#[test]
fn decimal_gather_amounts_are_exact_and_reject_unrepresentable_inputs() {
    use deep_hearth::core::quantity::Mass;

    for (text, milligrams) in [
        ("0.001", 1),
        ("0.01", 10),
        ("0.1", 100),
        ("3", 3_000),
        ("3.2", 3_200),
        ("3.020", 3_020),
        ("3.002", 3_002),
        ("18446744073709551.615", u64::MAX),
    ] {
        assert_eq!(
            command::parse_gather_grams(text),
            Ok(Mass::from_milligrams(milligrams)),
            "{text}"
        );
    }
    for text in [
        "",
        ".",
        ".001",
        "1.",
        "+1",
        "-1",
        "1e3",
        "1.0000",
        "1..0",
        "1.2.3",
        " 1",
        "18446744073709551.616",
        "18446744073709551616",
    ] {
        assert!(command::parse_gather_grams(text).is_err(), "{text}");
    }
}

#[test]
fn fractional_gathering_conserves_the_actual_requested_milligrams() {
    use deep_hearth::logistics::project_player_inventory;
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let before = calculate_matter_accounting(&session.state).unwrap().total();
    let (started, _) = session.command("gather 1 0.125").unwrap();
    assert!(started.contains("Gathering started: 0.125 g"), "{started}");
    let (completed, _) = session.command("wait work").unwrap();
    assert!(
        completed.contains("Gathered: 0.125 g collected"),
        "{completed}"
    );
    let carried = project_player_inventory(&session.registries, &session.state).unwrap();
    assert_eq!(
        carried
            .carried()
            .stacks()
            .iter()
            .map(|s| s.mass().milligrams())
            .sum::<u64>(),
        125
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        before
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("berries | 0.125 g"), "{inventory}");

    // Out-of-range and nonrepresentable input cannot spend control time or
    // mutate the physical source, even after an earlier legal collection.
    let saved = snapshot(&session);
    for input in ["gather 1 0.0001", "gather 1 -1", "gather 1 99999999"] {
        assert!(session.command(input).is_err(), "{input}");
    }
    assert_eq!(snapshot(&session), saved);
}

#[test]
fn explicit_food_and_water_commands_use_the_observed_player_view() {
    use deep_hearth::core::quantity::{Mass, Volume};
    use deep_hearth::logistics::project_player_inventory;

    assert_eq!(view::grams(Mass::from_milligrams(1)), "0.001");
    assert_eq!(view::milliliters(Volume::from_microliters(1)), "0.001");
    let mut session = Session::new(24318).unwrap();
    let initial = snapshot(&session);
    let sources =
        deep_hearth::survival::available_local_drink_sources(&session.registries, &session.state)
            .count();
    let initial_stacks = project_player_inventory(&session.registries, &session.state)
        .unwrap()
        .carried()
        .stacks()
        .len();
    let (drink_catalog, _) = session.command("drink").unwrap();
    assert_eq!(drink_catalog.contains("No drinkable source"), sources == 0);
    for invalid in [
        "eat 0".to_owned(),
        format!("eat {}", initial_stacks + 1),
        "drink 0".to_owned(),
        format!("drink {}", sources + 1),
    ] {
        assert!(session.command(&invalid).is_err(), "{invalid}");
    }
    assert_eq!(
        snapshot(&session),
        initial,
        "listing and invalid choices are read-only"
    );

    for command in ["e 20", "wait 10", "gather 1 500"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let ticks = activity::remaining_work_control_ticks(&session.state).unwrap();
    for _ in 0..ticks {
        session
            .step(PlayerDirectControlInput::NEUTRAL, &mut String::new())
            .unwrap();
        if session.state.player_work().active().is_none() {
            break;
        }
    }
    assert!(session.state.player_work().active().is_none());
    let projected = project_player_inventory(&session.registries, &session.state).unwrap();
    let berries = projected
        .carried()
        .stacks()
        .iter()
        .enumerate()
        .find(|(_, stack)| {
            session
                .registries
                .materials()
                .commodity_name(stack.commodity())
                == Some("berries")
        })
        .map(|(index, _)| index + 1)
        .expect("ordinary gathered berries are present in carried inventory");
    let (meal, _) = session.command(&format!("eat {berries}")).unwrap();
    assert!(
        meal.contains("Eating started") || meal.contains("AlreadySatisfied"),
        "explicit food choice must produce a canonical intake result: {meal}"
    );
    assert!(!meal.contains("MaterialLotId"));
    assert!(!meal.contains("AggregateVolume"));
    assert!(!meal.contains("Energy("));
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn active_gathering_status_names_its_reserved_source() {
    let mut session = Session::new(24318).unwrap();
    assert!(
        deep_hearth::surface::active_player_surface_gathering_commodity(&session.state).is_none()
    );
    for command in ["e 20", "wait 10", "gather 1 500"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let work = session
        .state
        .player_work()
        .active()
        .expect("gathering started");
    let commodity = deep_hearth::surface::active_player_surface_gathering_commodity(&session.state)
        .expect("active gathering must expose its own material");
    assert_eq!(
        session.registries.materials().commodity_name(commodity),
        Some("berries")
    );
    let label = activity::label(&session.registries, &session.state, work);
    assert!(label.contains("berries"), "active work label: {label}");
    assert!(label.contains("500 g"), "active work label: {label}");
    let status = session.command("status").unwrap().0;
    assert!(
        status.contains(&label),
        "status must show the same work label"
    );
    let ticks = activity::remaining_work_control_ticks(&session.state).unwrap();
    for _ in 0..ticks {
        session
            .step(PlayerDirectControlInput::NEUTRAL, &mut String::new())
            .unwrap();
        if session.state.player_work().active().is_none() {
            break;
        }
    }
    assert!(session.state.player_work().active().is_none());
    assert!(
        deep_hearth::surface::active_player_surface_gathering_commodity(&session.state).is_none()
    );
}

fn stone_campfire_from_wilderness() -> Session {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "n 20",
        "wait 10",
        "e 20",
        "wait 10",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    session
}

#[test]
fn finite_campfire_fuel_warms_player_and_preserves_terminal_matter() {
    let mut empty = Session::new(24318).unwrap();
    let before = snapshot(&empty);
    let (fire, _) = empty.command("fire").unwrap();
    assert!(fire.contains("Requires 1000 g stone"));
    assert!(fire.contains("none at your current location"));
    for invalid in [
        "build-fire",
        "feed-fire 1 1",
        "feed-fire 0 1",
        "feed-fire 1 0",
    ] {
        assert!(empty.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&empty), before);

    let mut session = stone_campfire_from_wilderness();
    let (catalog, _) = session.command("fire").unwrap();
    assert!(catalog.contains("stone-ring campfire | 0.000 MJ stored"));
    assert!(catalog.contains("Fuel: 1000 g timber log per charge"));
    assert!(catalog.contains("Fuel: 500 g wood chips per charge"));
    assert!(catalog.contains("feed-fire 1 1"));
    let before_invalid = snapshot(&session);
    let bad_fuel = session.command("feed-fire 1 2").unwrap_err();
    assert!(bad_fuel.contains("no carried material at that stack index"));
    assert_eq!(snapshot(&session), before_invalid);

    let total_before = deep_hearth::matter::calculate_matter_accounting(&session.state)
        .unwrap()
        .total();
    let warmth_before = session
        .state
        .survival()
        .player()
        .unwrap()
        .thermal_comfort_ppm();
    let (burned, _) = session.command("feed-fire 1 1").unwrap();
    assert!(burned.contains("Burned 1000 g timber log in the campfire"));
    assert!(burned.contains("12.000 MJ"));
    let (status, _) = session.command("status").unwrap();
    assert!(status.contains("Local warmth: stone-ring campfire | 12.000 MJ stored"));
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        total_before,
    );
    let saved = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (first, _) = session.command("wait 7200").unwrap();
    let (replay, _) = restored.command("wait 7200").unwrap();
    assert_eq!(first, replay);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(
        session
            .state
            .survival()
            .player()
            .unwrap()
            .thermal_comfort_ppm()
            > warmth_before
    );
    let (fire_after, _) = session.command("fire").unwrap();
    assert!(fire_after.contains("10.") && fire_after.contains("MJ stored"));
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        total_before,
    );
    session.command("w 20").unwrap();
    session.command("wait 10").unwrap();
    assert!(
        session
            .command("fire")
            .unwrap()
            .0
            .contains("none at your current location")
    );
    let remote_snapshot = snapshot(&session);
    assert!(session.command("feed-fire 1 1").is_err());
    assert_eq!(snapshot(&session), remote_snapshot);
}

#[test]
fn campfire_accepts_woodworking_byproduct_as_distinct_finite_fuel() {
    let mut session = prepared_flint_and_handle();
    for command in [
        "n 20",
        "wait 10",
        "n 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("wood chips | 800 g"));
    assert!(inventory.contains("flint flakes | 80 g"));
    let before = snapshot(&session);
    let wrong_fuel = session.command("feed-fire 1 2").unwrap_err();
    assert!(wrong_fuel.contains("use 'fire' to see eligible carried stacks"));
    assert_eq!(snapshot(&session), before);
    let accounting = deep_hearth::matter::calculate_matter_accounting(&session.state)
        .unwrap()
        .total();
    let (result, _) = session.command("feed-fire 1 1").unwrap();
    assert!(result.contains("Burned 500 g wood chips"));
    assert!(result.contains("5.000 MJ"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("wood chips | 300 g"));
    let after_one_charge = snapshot(&session);
    assert!(session.command("feed-fire 1 1").is_err());
    assert_eq!(snapshot(&session), after_one_charge);
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        accounting,
    );
    assert!(
        session
            .command("fire")
            .unwrap()
            .0
            .contains("5.000 MJ stored")
    );
}

#[test]
fn angled_pointing_constructs_real_timber_post_with_local_visibility_and_replay() {
    let mut session = Session::new(24318).unwrap();
    let original = snapshot(&session);
    assert!(session.command("sprint e-down 3").is_err());
    assert!(session.command("jump s-down 10").is_err());
    let (initial, _) = session.command("look e-down").unwrap();
    assert!(initial.contains("Pointed at"));
    assert!(!initial.contains("Nearby framing"));
    assert_eq!(snapshot(&session), original);
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 3250",
        "wait work",
        "select 1",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let target = session.command("look e-down").unwrap().0;
    assert!(target.contains("Pointed at stone terrain at (11,1,9), face Top"));
    let total = deep_hearth::matter::calculate_matter_accounting(&session.state)
        .unwrap()
        .total();
    let (queued, _) = session.command("use e-down").unwrap();
    assert!(queued.contains("QueuedUntilSimulationBoundary"));
    let (admitted, _) = session.command("wait 72").unwrap();
    assert!(admitted.contains("StructuralConstructionStarted"));
    let checkpoint = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (finished, _) = session.command("wait work").unwrap();
    let (replayed, _) = restored.command("wait work").unwrap();
    assert_eq!(finished, replayed);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(finished.contains("Construction finished"));
    let (look, _) = session.command("look").unwrap();
    assert!(look.contains("Nearby framing: axial compression at (11,2,9) | Active | 3250 g"));
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        total,
    );
    session.command("w 20").unwrap();
    session.command("wait 10").unwrap();
    let (remote, _) = session.command("look").unwrap();
    assert!(!remote.contains("Nearby framing"));
}

#[test]
fn sprint_sneak_crouch_and_jump_use_authoritative_body_motion() {
    let mut session = Session::new(24318).unwrap();
    let before = snapshot(&session);
    for invalid in [
        "sprint",
        "sneak",
        "sprint up",
        "sneak down",
        "sprint e 0",
        "sneak e 241",
        "jump up",
        "jump 10",
        "jump n 241",
        "crouch 0",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), before);

    let (crouched, _) = session.command("crouch").unwrap();
    assert!(crouched.contains("Body: Crouching"));
    assert_eq!(session.state.control_tick().value(), 1);
    let (released, _) = session.command("wait 1").unwrap();
    assert!(released.contains("Body: Standing"));
    let (jump, _) = session.command("jump").unwrap();
    assert!(jump.contains("Airborne: gravity is active"));
    assert!(
        session
            .state
            .logistics()
            .player()
            .unwrap()
            .body()
            .foot_position()
            .y_nanometers()
            > 2_000_000_000
    );
    let midair = snapshot(&session);
    let mut resumed = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&midair)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (first, _) = session.command("wait 10").unwrap();
    let (second, _) = resumed.command("wait 10").unwrap();
    assert_eq!(first, second);
    assert_eq!(snapshot(&session), snapshot(&resumed));

    let mut walking = Session::new(24318).unwrap();
    let mut sprinting = Session::new(24318).unwrap();
    let east = PlayerControlInput::new(HorizontalControlInput::new(1_000_000, 0).unwrap());
    for _ in 0..12 {
        walking
            .step(PlayerDirectControlInput::new(east), &mut String::new())
            .unwrap();
        sprinting
            .step(
                PlayerDirectControlInput::new(east.with_sprint(true)),
                &mut String::new(),
            )
            .unwrap();
    }
    let walk_x = walking
        .state
        .logistics()
        .player()
        .unwrap()
        .body()
        .foot_position()
        .x_nanometers();
    let sprint_x = sprinting
        .state
        .logistics()
        .player()
        .unwrap()
        .body()
        .foot_position()
        .x_nanometers();
    assert!(
        sprint_x > walk_x,
        "sprinting must move farther through production kinematics"
    );
    assert_eq!(walking.state.control_tick(), sprinting.state.control_tick());
    let mut sneaking = Session::new(24318).unwrap();
    let (sneak, _) = sneaking.command("sneak e 3").unwrap();
    assert!(sneak.contains("Body: Crouching"));
    let (running, _) = Session::new(24318).unwrap().command("sprint e 3").unwrap();
    assert!(running.contains("Advanced 3 control ticks"));
}

#[test]
fn seeded_opening_is_replayable_and_uses_authoritative_admission() {
    let first = Session::new(24318).unwrap();
    let second = Session::new(24318).unwrap();
    assert_eq!(snapshot(&first), snapshot(&second));
    assert!(first.state.survival().player().is_some());
    assert!(first.state.world().chunks().count() >= 9);
    assert!(first.opening().contains("seed 24318"));
    assert!(first.opening().contains("Condition: vitality"));
    assert!(first.opening().contains("berries 1 east at (9,2,8)"));
    assert!(
        first
            .opening()
            .contains("Visible spring water 1 east, 1 south")
    );
    assert!(first.opening().contains("Hand: empty"));
    assert!(first.opening().contains("13 C"));
}

#[test]
fn text_commands_execute_real_multirate_control_ticks() {
    let mut session = Session::new(24318).unwrap();
    let start = snapshot(&session);
    let (frame, continue_playing) = session.command("wait 72").unwrap();
    assert!(continue_playing);
    assert!(frame.contains("Advanced 72 control ticks"));
    assert_eq!(
        session.state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK
    );
    assert_eq!(session.state.tick().value(), 1);
    assert_ne!(snapshot(&session), start);

    let (frame, _) = session.command("use down").unwrap();
    assert!(frame.contains("Use:"));
    assert_eq!(
        session.state.control_tick().value(),
        CONTROL_TICKS_PER_SIMULATION_TICK + 1
    );
}

#[test]
fn walking_to_berries_and_gathering_reaches_real_carried_inventory() {
    let mut session = Session::new(24318).unwrap();
    let (moving, _) = session.command("e 20").unwrap();
    assert!(moving.contains("Still moving: wait 10"));
    assert!(moving.contains("berries here"));
    let (settled, _) = session.command("wait 10").unwrap();
    assert!(!settled.contains("Still moving"));
    assert!(settled.contains("Food: Fruit, fresh; estimated spoilage in"));
    assert!(!settled.contains("TickSpan("));
    let (began, _) = session.command("gather 1").unwrap();
    assert!(began.contains("Gathering started"));
    assert!(began.contains("Work: Gathering 500 g of berries"));
    assert!(began.contains("scheduled completion in"));
    let (finished, _) = session.command("wait 1600").unwrap();
    assert!(finished.contains("Gathered:"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("berries | 500 g"));
}

#[test]
fn gathered_flint_can_be_crafted_through_ordinary_carried_recipe() {
    let mut session = Session::new(24318).unwrap();
    let before = snapshot(&session);
    let (empty, _) = session.command("recipes").unwrap();
    assert!(empty.contains("No recipes have enough carried input"));
    let (all, _) = session.command("recipes all").unwrap();
    assert!(all.contains("99. knap flint cutting head"));
    for invalid in [
        "craft 0",
        "craft 99",
        "craft 99 0",
        "craft 99 1 with 1",
        "craft 99 1 from 1",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), before);

    for command in ["e 20", "wait 10", "s 20", "wait 10", "s 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let (available, _) = session.command("gather").unwrap();
    assert!(available.contains("3. flint nodule"));
    let (started, _) = session.command("gather 3 300").unwrap();
    assert!(started.contains("Gathering started"));
    let (gathered, _) = session.command("wait work").unwrap();
    assert!(gathered.contains("Gathered:"));
    let (recipes, _) = session.command("recipes").unwrap();
    assert!(recipes.contains("99. knap flint cutting head"));
    assert!(recipes.contains("1 batch available | by hand"));
    let before_rejection = snapshot(&session);
    assert!(session.command("craft 99 2").is_err());
    assert!(session.command("craft 99 from 1").is_err());
    assert_eq!(snapshot(&session), before_rejection);
    let (started, _) = session.command("craft 99").unwrap();
    assert!(started.contains("Started knap flint cutting head"));
    assert!(started.contains("scheduled completion in 1m 48s"));
    let (finished, _) = session.command("wait work").unwrap();
    assert!(finished.contains("Craft completed: knap flint cutting head"));
    assert!(finished.contains("Received 220 g knapped flint cutting head (carried)"));
    assert!(finished.contains("Received 80 g flint flakes (carried)"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried matter: 300 g / 25000 g"));
    assert!(inventory.contains("knapped flint cutting head | 220 g"));
    assert!(inventory.contains("flint flakes | 80 g | 20 C"));
}

#[test]
fn manual_crafting_continues_deterministically_after_trusted_save_load() {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 3 300",
        "wait work",
        "craft 99",
    ] {
        session.command(command).unwrap();
    }
    let saved = snapshot(&session);
    let envelope: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let restored_state = envelope.into_state(&session.registries).unwrap();
    let mut restored = Session {
        registries: build_registries(),
        state: restored_state,
        seed: None,
    };
    assert_eq!(snapshot(&session), snapshot(&restored));
    let (original_finished, _) = session.command("wait work").unwrap();
    let (restored_finished, _) = restored.command("wait work").unwrap();
    assert_eq!(original_finished, restored_finished);
    assert!(original_finished.contains("Received 220 g knapped flint cutting head"));
    assert_eq!(snapshot(&session), snapshot(&restored));
}

fn prepared_flint_and_handle() -> Session {
    let mut session = Session::new(24318).unwrap();
    // Follow visible opening resources, not test-only material insertion.
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "craft 8",
        "wait work",
        "w 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 3 300",
        "wait work",
        "craft 99",
        "wait work",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    session
}

#[test]
fn flint_knife_assembly_local_pickup_and_disassembly_conserve_player_matter() {
    let mut session = Session::new(24318).unwrap();
    let before = snapshot(&session);
    let (all, _) = session.command("gear all").unwrap();
    assert!(all.contains("58. hafted flint woodworking knife"));
    assert!(all.contains("Requires 220 g knapped flint cutting head"));
    let (ready, _) = session.command("gear").unwrap();
    assert!(ready.contains("None available from your carried materials"));
    for invalid in [
        "assemble 58",
        "assemble 0",
        "take 1",
        "recover 1",
        "equip 1",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), before);

    let mut session = prepared_flint_and_handle();
    let before_catalog = snapshot(&session);
    let (ready, _) = session.command("gear").unwrap();
    assert!(ready.contains("58. hafted flint woodworking knife | 420 g | ready"));
    assert_eq!(snapshot(&session), before_catalog);
    let (assembled, _) = session.command("assemble 58").unwrap();
    assert!(assembled.contains("Assembled hafted flint woodworking knife"));
    assert!(session.command("equip 1").is_err());
    let (local, _) = session.command("gear").unwrap();
    assert!(local.contains("1. hafted flint woodworking knife | 420 g | condition 100%"));
    let (recovered, _) = session.command("recover 1").unwrap();
    assert!(recovered.contains("Recovered hafted flint woodworking knife into carried materials"));
    let (materials, _) = session.command("inventory").unwrap();
    assert!(materials.contains("wood handle | 200 g"));
    assert!(materials.contains("knapped flint cutting head | 220 g"));
    assert!(materials.contains("Carried matter: 1300 g / 25000 g"));
    assert!(session.command("take 1").is_err());

    session.command("assemble 58").unwrap();
    let (picked_up, _) = session.command("take 1").unwrap();
    assert!(picked_up.contains("Picked up hafted flint woodworking knife"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried equipment 1: hafted flint woodworking knife | 420 g"));
    assert!(inventory.contains("Carried matter: 880 g / 24580 g"));
    let (held, _) = session.command("equip 1").unwrap();
    assert!(held.contains("Holding hafted flint woodworking knife"));
    assert!(held.contains("Hand: hafted flint woodworking knife"));

    let saved = snapshot(&session);
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let restored = Session {
        registries: build_registries(),
        state: decoded.into_state(&session.registries).unwrap(),
        seed: None,
    };
    assert_eq!(snapshot(&restored), saved);
    assert!(
        restored
            .opening()
            .contains("Hand: hafted flint woodworking knife")
    );
    let (dropped, _) = session.command("drop").unwrap();
    assert!(dropped.contains("Drop:"));
    let (local, _) = session.command("gear").unwrap();
    assert!(local.contains("1. hafted flint woodworking knife"));
    session.command("take 1").unwrap();
    assert!(session.command("equip 1").is_ok());
}

#[test]
fn carried_flint_knife_accelerates_timed_woodwork_and_survives_work_save() {
    let mut session = prepared_flint_and_handle();
    for command in [
        "assemble 58",
        "take 1",
        "equip 1",
        "n 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let saved_before = snapshot(&session);
    let mut without_tool = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&saved_before)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (bare, _) = without_tool.command("craft 14").unwrap();
    assert!(bare.contains("Work: shape timber boards; scheduled completion in 3m 0s"));
    let (with_tool, _) = session.command("craft 14 with 1").unwrap();
    assert!(with_tool.contains("Work: shape timber boards; scheduled completion in 1m 12s"));
    let started = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&started)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (done, _) = session.command("wait work").unwrap();
    let (replayed, _) = restored.command("wait work").unwrap();
    assert_eq!(done, replayed);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(done.contains("Craft completed: shape timber boards"));
    assert!(done.contains("Received 800 g timber board (carried)"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried equipment 1: hafted flint woodworking knife"));
    assert!(inventory.contains("condition 98%"));
    assert!(inventory.contains("wood chips | 1000 g"));
    assert!(inventory.contains("timber board | 800 g"));
}

fn field_box_body_from_wilderness() -> Session {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 2000",
        "wait work",
        "craft 14 2",
        "wait work",
        "craft 27",
        "wait work",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    session
}

#[test]
fn rough_field_box_placement_and_local_quick_moves_are_production_backed() {
    let mut empty = Session::new(24318).unwrap();
    let before = snapshot(&empty);
    let (catalog, _) = empty.command("storage all").unwrap();
    assert!(catalog.contains("5. rough timber field box"));
    assert!(catalog.contains("Requires 1600 g rough timber field box body"));
    assert!(empty.command("place-store 5").is_err());
    assert!(empty.command("stash 1 1").is_err());
    assert!(empty.command("withdraw 1 1").is_err());
    assert_eq!(snapshot(&empty), before);

    let mut session = field_box_body_from_wilderness();
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("rough timber field box body | 1600 g"));
    let before = snapshot(&session);
    assert!(session.command("place-store 5").is_err());
    let (blocked, _) = session.command("storage all").unwrap();
    assert!(blocked.contains("intersects an opening or temple resident"));
    assert_eq!(snapshot(&session), before);

    for command in ["w 20", "wait 10", "s 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let (ready, _) = session.command("storage").unwrap();
    assert!(ready.contains("5. rough timber field box | 10000 g capacity | ready"));
    let before_placement = deep_hearth::matter::calculate_matter_accounting(&session.state)
        .unwrap()
        .total();
    let (placed, _) = session.command("place-store 5").unwrap();
    assert!(placed.contains("Placed rough timber field box"));
    assert!(placed.contains("1. rough timber field box | 0 g / 10000 g"));
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried matter: 400 g / 25000 g"));
    assert!(inventory.contains("Local storage 1: rough timber field box | 0 g / 10000 g"));
    let before_rejections = snapshot(&session);
    for invalid in [
        "place-store 0",
        "stash 0 1",
        "stash 1 0",
        "withdraw 2 1",
        "withdraw 1 1",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), before_rejections);

    let (stashed, _) = session.command("stash 1 1").unwrap();
    assert!(stashed.contains("Stashed 400 g wood chips in rough timber field box"));
    assert!(stashed.contains("Local storage 1: rough timber field box | 400 g / 10000 g"));
    assert!(stashed.contains("Carried matter: 0 g / 25000 g"));
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        before_placement,
    );
    let saved = snapshot(&session);
    let mut restored = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    assert_eq!(snapshot(&restored), saved);
    let (withdrawn, _) = session.command("withdraw 1 1").unwrap();
    let (replayed, _) = restored.command("withdraw 1 1").unwrap();
    assert_eq!(withdrawn, replayed);
    assert_eq!(snapshot(&session), snapshot(&restored));
    assert!(withdrawn.contains("Withdrew 400 g wood chips from rough timber field box"));
    assert!(withdrawn.contains("Carried matter: 400 g / 25000 g"));
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        before_placement,
    );
    session.command("n 20").unwrap();
    session.command("wait 10").unwrap();
    let (remote, _) = session.command("storage").unwrap();
    assert!(remote.contains("Local containers"));
    assert!(remote.contains("(none)"));
    let before_remote = snapshot(&session);
    assert!(session.command("stash 1 1").is_err());
    assert_eq!(snapshot(&session), before_remote);
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    assert!(
        session
            .command("storage")
            .unwrap()
            .0
            .contains("rough timber field box | 0 g / 10000 g")
    );
}

#[test]
fn food_selection_and_contextual_drinking_are_explained_and_authoritative() {
    let mut session = Session::new(24318).unwrap();
    session.command("e 20").unwrap();
    session.command("wait 10").unwrap();
    session.command("gather 1").unwrap();
    session.command("wait 1600").unwrap();
    let (selected, _) = session.command("select 1").unwrap();
    assert!(selected.contains("Active hand: berries"));
    let (meal, _) = session.command("use").unwrap();
    assert!(meal.contains("Eating started"));
    assert!(meal.contains("Hand: berries"));
    assert!(meal.contains("Selected food: use eats it"));
    assert!(meal.contains("Work: Eating"));
    session.command("wait 220").unwrap();
    session.command("s 20").unwrap();
    session.command("wait 10").unwrap();
    let (empty, _) = session.command("empty").unwrap();
    assert!(empty.contains("Active hand: empty"));
    let (drinking, _) = session.command("use").unwrap();
    assert!(drinking.contains("Drinking started"));
    assert!(drinking.contains("Work: Drinking"));
}

#[test]
fn text_warmth_warning_uses_authored_survival_threshold() {
    let mut session = Session::new(24318).unwrap();
    let threshold = session
        .registries
        .survival()
        .physiology()
        .thermal_comfort()
        .warning_below_ppm();
    assert!(!view::urgency(&session.registries, &session.state).3);
    let mut reached = false;
    for _ in 0..20_000 {
        let assessment = assess_survival(&session.registries, &session.state).unwrap();
        if assessment.thermal_comfort_state() != ThermalComfortState::Comfortable {
            let comfort = assessment.thermal_comfort_ppm();
            assert!(comfort < threshold);
            assert!(view::urgency(&session.registries, &session.state).3);
            assert!(
                view::status(&session.registries, &session.state)
                    .contains("Warnings: cold exposure")
            );
            reached = true;
            break;
        }
        session
            .step(PlayerDirectControlInput::NEUTRAL, &mut String::new())
            .unwrap();
    }
    assert!(
        reached,
        "the exposed opening must eventually reach the authored cold warning"
    );
}

#[test]
fn work_boundary_rejection_explains_the_required_player_action() {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 3 300",
        "wait work",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    session.command("wait 1").unwrap();
    let before = snapshot(&session);
    assert!(
        session
            .command("craft 99")
            .unwrap_err()
            .contains("Run 'wait boundary', then retry")
    );
    assert_eq!(snapshot(&session), before);
    session.command("wait boundary").unwrap();
    assert!(
        session
            .command("craft 99")
            .unwrap()
            .0
            .contains("Started knap flint cutting head")
    );
}

#[test]
fn invalid_commands_and_readouts_do_not_change_the_world() {
    let mut session = Session::new(123).unwrap();
    let before = snapshot(&session);
    assert!(session.command("n -1").is_err());
    assert!(session.command("gather 99").is_err());
    assert!(session.command("slot 0").is_err());
    assert!(session.command("select 1").is_err());
    assert!(session.command("wait work").is_err());
    assert!(session.command("respawn").is_err());
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried matter"));
    let (look, _) = session.command("look e").unwrap();
    assert!(
        look.contains("Pointed at")
            || look.contains("No targetable block")
            || look.contains("Pointing unavailable")
    );
    assert_eq!(snapshot(&session), before);
}

#[test]
fn save_envelope_roundtrips_through_current_trusted_loader() {
    let mut session = Session::new(8).unwrap();
    session.command("e 20").unwrap();
    let encoded =
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let reloaded = decoded.into_state(&session.registries).unwrap();
    assert_eq!(
        snapshot(&session),
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &reloaded)).unwrap()
    );
}

#[test]
fn quitting_does_not_advance_simulation() {
    let mut session = Session::new(8).unwrap();
    let before = snapshot(&session);
    let (notice, continue_playing) = session.command("quit").unwrap();
    assert!(!continue_playing);
    assert!(notice.contains("ended"));
    assert_eq!(before, snapshot(&session));
}
