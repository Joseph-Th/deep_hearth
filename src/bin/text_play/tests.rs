//! Scriptable play boundary regressions against actual production simulation.

use super::*;
use deep_hearth::core::time::CONTROL_TICKS_PER_SIMULATION_TICK;
use deep_hearth::logistics::{HorizontalControlInput, PlayerControlInput};
use deep_hearth::survival::{ThermalComfortState, assess_survival};

fn snapshot(session: &Session) -> Vec<u8> {
    serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap()
}

#[test]
fn gathering_catalog_matches_the_reserve_limited_default_command() {
    use deep_hearth::core::quantity::Mass;
    use deep_hearth::labor::PlayerWorkStartError;
    use deep_hearth::surface::{
        SurfaceGatheringError, assess_affordable_player_gathering_mass,
        assess_player_surface_gathering_option_admission,
    };

    let mut session = Session::new(24318).unwrap();
    for _ in 0..3 {
        session.command("e 20").unwrap();
        session.command("wait 10").unwrap();
    }
    let option = player_surface_gathering_options(&session.registries, &session.state).unwrap()[0];
    let ceiling = option.suggested_mass_at(&session.state);
    let mut saved: serde_json::Value = serde_json::from_slice(&snapshot(&session)).unwrap();
    let player = &mut saved["state"]["systems"]["survival"]["player"];
    player["metabolic_energy"] = serde_json::json!(0);
    let depleted = serde_json::from_value::<LoadedSaveEnvelope>(saved.clone())
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    let Err(SurfaceGatheringError::PlayerWork(PlayerWorkStartError::InsufficientMetabolicEnergy {
        required,
        ..
    })) = assess_player_surface_gathering_option_admission(
        &session.registries,
        &depleted,
        option,
        ceiling,
    )
    else {
        panic!("stone gathering should require metabolic reserves");
    };
    saved["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(required.nanojoules() / 2);
    session.state = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    let expected = assess_affordable_player_gathering_mass(
        &session.registries,
        &session.state,
        option,
        ceiling,
    )
    .unwrap();
    assert!(expected > Mass::ZERO && expected < ceiling);
    let before = snapshot(&session);
    let catalog = session.command("gather").unwrap().0;
    assert!(
        catalog.contains(&format!("default {} g", view::grams(expected))),
        "{catalog}"
    );
    assert!(
        !catalog.contains("Default batch currently blocked"),
        "{catalog}"
    );
    assert_eq!(snapshot(&session), before);
    let report = session.command("gather 1").unwrap().0;
    let Some(PlayerWork::SurfaceGathering { work }) = session.state.player_work().active() else {
        panic!("{report}");
    };
    assert_eq!(work.gathered_mass(), expected);
}

#[test]
fn eating_at_slow_boundary_refreshes_the_exact_selected_cohort() {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "gather 1 500",
        "wait work",
        "wait boundary",
    ] {
        session.command(command).unwrap();
    }
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK - 1 {
        session
            .step(PlayerDirectControlInput::NEUTRAL, &mut String::new())
            .unwrap();
    }
    assert!(
        session
            .state
            .control_tick()
            .checked_next()
            .unwrap()
            .is_simulation_boundary()
    );
    let (report, _) = session.command("eat 1").unwrap();
    assert!(report.contains("Eating started:"), "{report}");
    assert!(!report.contains("became stale"), "{report}");
    assert!(matches!(
        session.state.player_work().active(),
        Some(PlayerWork::Eating { .. })
    ));
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn named_food_selection_matches_numbered_cohort_without_guessing_a_different_material() {
    let mut by_name = Session::new(24318).unwrap();
    let mut by_row = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "gather 1 500", "wait work"] {
        by_name.command(command).unwrap();
        by_row.command(command).unwrap();
    }
    let before = snapshot(&by_name);
    assert_eq!(snapshot(&by_row), before);
    assert!(
        by_name.command("eat crushed stone").is_err(),
        "a missing named material must not select an unrelated food"
    );
    assert_eq!(snapshot(&by_name), before);
    let (named, _) = by_name.command("eat BERRIES").unwrap();
    let (numbered, _) = by_row.command("eat 1").unwrap();
    assert!(named.contains("Eating started:"), "{named}");
    assert!(numbered.contains("Eating started:"), "{numbered}");
    assert_eq!(snapshot(&by_name), snapshot(&by_row));
    by_name.command("wait finish").unwrap();
    by_row.command("wait finish").unwrap();
    assert_eq!(snapshot(&by_name), snapshot(&by_row));
}

#[test]
fn repeated_gathering_matches_explicit_authoritative_batches() {
    let mut repeated = Session::new(24318).unwrap();
    let mut manual = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10"] {
        repeated.command(command).unwrap();
        manual.command(command).unwrap();
    }
    let (output, _) = repeated.command("gather 1 100 repeat 3").unwrap();
    assert!(
        output.contains("Gather repeat: 3 batches completed."),
        "{output}"
    );
    assert_eq!(
        output.matches("Gathered: 100 g collected").count(),
        3,
        "{output}"
    );
    assert_eq!(
        output.matches("Visible surface resources").count(),
        1,
        "repeating an ordinary batch must not print a full look for every batch"
    );
    for _ in 0..3 {
        manual.command("gather 1 100").unwrap();
        manual.command("wait work").unwrap();
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&repeated.registries, &repeated.state),
        Ok(())
    );
}

#[test]
fn explicit_gathering_above_default_matches_repeated_batches() {
    let mut repeated = Session::new(24318).unwrap();
    let mut manual = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "e 20", "wait 10", "e 20", "wait 10"] {
        repeated.command(command).unwrap();
        manual.command(command).unwrap();
    }
    let option =
        player_surface_gathering_options(&repeated.registries, &repeated.state).unwrap()[0];
    let mass = option
        .direct_batch_mass()
        .checked_add(option.direct_batch_mass())
        .unwrap();
    assert!(mass <= option.maximum_mass_at(&repeated.state));
    let grams = view::grams(mass);
    let (output, _) = repeated
        .command(&format!("gather 1 {grams} repeat 2"))
        .unwrap();
    assert!(
        output.contains("Gather repeat: 2 batches completed."),
        "{output}"
    );
    for _ in 0..2 {
        manual.command(&format!("gather 1 {grams}")).unwrap();
        manual.command("wait work").unwrap();
    }
    assert_eq!(snapshot(&repeated), snapshot(&manual));
}

#[test]
fn repeated_gathering_stops_without_retargeting_a_depleted_source() {
    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "e 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let options = player_surface_gathering_options(&session.registries, &session.state).unwrap();
    let original = options[0];
    let (output, _) = session.command("gather 1 repeat 32").unwrap();
    assert!(
        output.contains("Gather repeat: source depleted after "),
        "{output}"
    );
    let current = player_surface_gathering_options(&session.registries, &session.state).unwrap();
    assert!(
        current
            .iter()
            .all(|option| option.resource() != original.resource()),
        "repeat must stop on the original source rather than using a new row"
    );
}

#[test]
fn repeated_gathering_stays_warm_during_paid_exertion_and_stops_at_finite_supply() {
    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let (output, _) = session.command("gather 1 500 repeat 32").unwrap();
    assert!(
        output.contains("Gather repeat: insufficient remaining source or carrying space")
            && output.contains("11 batches completed")
            && !output.contains("Warnings: cold exposure"),
        "{output}"
    );
    assert!(session.state.player_work().active().is_none());
    let runway = deep_hearth::survival::assess_player_unheated_comfort_runway(
        &session.registries,
        &session.state,
    )
    .unwrap();
    assert!(
        runway.current().is_some(),
        "work-produced warmth should not erase future rest-time cold risk"
    );
}

#[test]
fn gathering_reprojects_the_same_rooted_source_after_automatic_loose_pickup() {
    use deep_hearth::core::quantity::Mass;
    use deep_hearth::core::state::validate_loaded_state;
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "gather 1 0.125", "wait work", "select 1"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let (dropped, _) = session.command("drop").unwrap();
    assert!(dropped.contains("Dropped 0.125 g berries"), "{dropped}");
    // Player drops have a real 40-control-tick grace interval. Advance to
    // one tick before reacquisition so that the next gather input encounters
    // a same-frame automatic pickup and the source revision genuinely shifts.
    let (grace, _) = session.command("wait 39").unwrap();
    assert!(!grace.contains("Picked up:"), "{grace}");
    let options = player_surface_gathering_options(&session.registries, &session.state).unwrap();
    let original_index = options
        .iter()
        .position(|option| option.source_mass() > Mass::from_milligrams(100_000))
        .unwrap_or_else(|| panic!("rooted berries disappeared after the drop"))
        + 1;
    let matter_before = calculate_matter_accounting(&session.state).unwrap().total();
    let original = options[original_index - 1];
    let mass = original.maximum_batch_mass();
    assert!(mass > original.direct_batch_mass());
    let grams = view::grams(mass);
    let (begun, _) = session
        .command(&format!("gather {original_index} {grams}"))
        .unwrap();
    assert!(begun.contains("Picked up: 0.125 g berries"), "{begun}");
    assert!(
        begun.contains(&format!("Gathering started: {grams} g")),
        "{begun}"
    );
    assert!(!begun.contains("surface revision"), "{begun}");
    for _ in 0..20 {
        if session.state.player_work().active().is_none() {
            break;
        }
        session.command("wait work").unwrap();
    }
    assert!(session.state.player_work().active().is_none());
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        matter_before
    );
    assert_eq!(
        validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
    let saved = snapshot(&session);
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &replay)).unwrap(),
        saved
    );
}

#[test]
fn shallow_aim_completes_an_overhead_timber_cover_on_the_played_seed() {
    use deep_hearth::core::state::validate_loaded_state;
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(80705).unwrap();
    let original_matter = calculate_matter_accounting(&session.state).unwrap().total();
    for command in [
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "w 20",
        "wait 10",
        "gather 1 1000 repeat 14",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
        "feed-fire 1 1",
        "select 1",
        "use e-down",
        "wait boundary",
        "wait work",
        "craft 14 9",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    for _ in 0..8 {
        if session.state.player_work().active().is_none() {
            break;
        }
        session.command("wait work").unwrap();
    }
    assert!(session.state.player_work().active().is_none());
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("timber board | 7200 g"), "{inventory}");
    session.command("select 3").unwrap();
    let before_preview = snapshot(&session);
    let (preview, _) = session.command("look e-shallow").unwrap();
    assert!(
        preview.contains("Cover preview: panel would occupy (9,0,11)"),
        "a player must see the real panel destination before committing materials: {preview}"
    );
    assert!(
        preview.contains("(9,-2,11)"),
        "the preview must describe the standing-height space below the panel: {preview}"
    );
    assert_eq!(
        snapshot(&session),
        before_preview,
        "preview must be read-only"
    );
    let (started, _) = session.command("use e-shallow").unwrap();
    assert!(started.contains("Action queued"), "{started}");
    assert!(started.contains("Queued interaction:"), "{started}");
    let before_wait = snapshot(&session);
    let mut reference = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before_wait)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (admission, _) = reference.command("wait work").unwrap();
    assert!(
        admission.contains("Structural construction started"),
        "{admission}"
    );
    assert!(
        admission.contains("Nearby panel: axial compression at (9,0,11) | Planned | 6500 g"),
        "{admission}"
    );
    assert!(
        reference.state.player_work().active().is_some(),
        "wait work must stop after admission, before construction finishes"
    );
    let (completed_in_one, _) = session.command("wait finish").unwrap();
    let (completed_in_two, _) = reference.command("wait finish").unwrap();
    assert!(
        completed_in_one.contains("Structural construction started"),
        "{completed_in_one}"
    );
    assert!(
        completed_in_one.contains("Construction finished"),
        "{completed_in_one}"
    );
    assert!(
        completed_in_two.contains("Construction finished"),
        "{completed_in_two}"
    );
    assert!(
        completed_in_one.contains("The built member is now load-bearing."),
        "{completed_in_one}"
    );
    assert_eq!(
        snapshot(&session),
        snapshot(&reference),
        "one wait finish must settle exactly the same canonical ticks as admission plus completion"
    );
    let (look, _) = session.command("look").unwrap();
    assert!(
        look.contains("Nearby panel: axial compression at (9,0,11) | Active | 6500 g"),
        "{look}"
    );
    assert!(
        look.contains("Nearby roof does not cover your standing position"),
        "the completed panel must not be mistaken for usable shelter: {look}"
    );
    assert!(
        deep_hearth::logistics::project_player_standing_space(
            &session.registries,
            &session.state,
            VoxelCoord::new(9, 1, 11),
        ),
        "the completed panel is a physical floor above which the player can stand"
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        original_matter
    );
    assert_eq!(
        validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn upward_aim_extends_a_real_walkable_shelter_over_lower_ground() {
    use deep_hearth::logistics::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(80705).unwrap();
    let original_matter = calculate_matter_accounting(&session.state).unwrap().total();
    // Build a post and a cap on the high side of the ledge, using ordinary
    // finite gathering, crafting, fuel and construction, not fixture placement.
    for command in [
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "w 20",
        "wait 10",
        "gather 1 1000 repeat 22",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
        "feed-fire 1 1",
        "craft 14 17",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let checkpoint = snapshot(&session);
    let mut reference = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (finished, _) = session.command("wait finish").unwrap();
    assert!(finished.contains("Craft completed"), "{finished}");
    for _ in 0..9 {
        reference.command("wait work").unwrap();
    }
    assert_eq!(
        snapshot(&session),
        snapshot(&reference),
        "long wait must settle the same work and every physical tick"
    );
    assert!(session.state.player_work().active().is_none());
    for command in [
        "n 20",
        "wait 10",
        "n 20",
        "wait 10",
        "select 1",
        "use n-down",
        "wait boundary",
        "wait work",
        "select 3",
        "use n-shallow",
        "wait boundary",
        "wait work",
        // Approach the far side of the ledge; retain actual continuous motion.
        "w 20",
        "wait 10",
        "w 20",
        "wait 10",
        "n 20",
        "wait 10",
        "n 20",
        "wait 10",
        "n 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    assert_eq!(
        session.player_position().unwrap(),
        VoxelCoord::new(8, -2, 5)
    );
    let before = snapshot(&session);
    let (preview, _) = session.command("look s-up").unwrap();
    assert!(preview.contains("panel would occupy (8,0,7)"), "{preview}");
    assert!(
        preview.contains("Potential standing space at (8,-2,7)"),
        "{preview}"
    );
    assert_eq!(snapshot(&session), before);
    for command in [
        "use s-up",
        "wait boundary",
        "wait work",
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "center",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    assert_eq!(
        session.player_position().unwrap(),
        VoxelCoord::new(8, -2, 7)
    );
    assert_eq!(
        assess_player_overhead_cover(&session.registries, &session.state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    assert!(
        session
            .command("look")
            .unwrap()
            .0
            .contains("cover: Covered")
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        original_matter
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn exact_supported_placement_builds_usable_shelter_on_level_ground() {
    use deep_hearth::logistics::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
    use deep_hearth::matter::calculate_matter_accounting;
    let mut session = Session::new(80705).unwrap();
    let original = calculate_matter_accounting(&session.state).unwrap().total();
    for command in [
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "w 20",
        "wait 10",
        "gather 1 1000 repeat 22",
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 1 1000",
        "wait work",
        "build-fire",
        "feed-fire 1 1",
        "select 1",
        "use e-down",
        "wait boundary",
        "wait finish",
        "craft 14 9",
        "wait finish",
        "center",
        "wait boundary",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let here = VoxelCoord::new(8, -1, 11);
    assert_eq!(session.player_position().unwrap(), here);
    let before = snapshot(&session);
    for rejected in [
        "build-at 1 7 1 11",
        "build-at 1 8 -1 11",
        "build-at 1 8 -2 11",
        "build-at 1 8000 0 11",
        "build-at 2 9 0 11",
        "look at 9223372036854775807 0 0",
    ] {
        assert!(session.command(rejected).is_err(), "{rejected}");
        assert_eq!(snapshot(&session), before);
    }
    for (plan, build) in [
        ("plan-at 1 9 0 11", "build-at 1 9 0 11"),
        ("plan-at 1 9 1 11", "build-at 1 9 1 11"),
        ("plan-at 3 8 1 11", "build-at 3 8 1 11"),
    ] {
        let before = snapshot(&session);
        let (preview, _) = session.command(plan).unwrap();
        assert!(preview.contains("Ready:"), "{preview}");
        if plan == "plan-at 3 8 1 11" {
            assert!(
                preview.contains("above the entire current standing footprint"),
                "the supported panel must be previewed as a prospective roof: {preview}"
            );
            assert!(preview.contains("wind from the sides"), "{preview}");
        }
        assert_eq!(snapshot(&session), before, "preview is read-only");
        assert!(
            session
                .command(build)
                .unwrap()
                .0
                .contains("Structural construction started")
        );
        assert_eq!(
            assess_player_overhead_cover(&session.registries, &session.state),
            Some(PlayerOverheadCoverAssessment::Open),
            "unfinished pieces grant no cover"
        );
        assert!(
            session
                .command("wait finish")
                .unwrap()
                .0
                .contains("Construction finished")
        );
        assert_eq!(session.player_position().unwrap(), here);
    }
    assert_eq!(
        assess_player_overhead_cover(&session.registries, &session.state),
        Some(PlayerOverheadCoverAssessment::Covered)
    );
    let before = snapshot(&session);
    let (pointed, _) = session.command("look at 8 1 11").unwrap();
    assert!(
        pointed.contains("Break target: panel at (8,1,11)"),
        "{pointed}"
    );
    assert_eq!(snapshot(&session), before);
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        original
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn begin_scheduled_work_aligns_the_real_clock_and_preserves_normal_admission() {
    fn prepared() -> Session {
        let mut session = Session::new(24318).unwrap();
        for command in [
            "e 20",
            "wait 10",
            "s 20",
            "wait 10",
            "s 20",
            "wait 10",
            "gather 2 300",
            "wait work",
            "wait 1",
        ] {
            session
                .command(command)
                .unwrap_or_else(|error| panic!("{command}: {error}"));
        }
        assert_ne!(
            session.state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK,
            0
        );
        session
    }

    let mut manual = prepared();
    let mut aligned = prepared();
    let before = snapshot(&aligned);
    for invalid in ["begin", "begin wait boundary", "begin quit", "begin craft"] {
        assert!(aligned.command(invalid).is_err(), "{invalid}");
        assert_eq!(snapshot(&aligned), before, "{invalid} advanced time");
    }
    manual.command("wait boundary").unwrap();
    let ordinary = manual.command("craft 99").unwrap().0;
    let (convenient, _) = aligned.command("begin craft 99").unwrap();
    assert!(
        ordinary.contains("Started knap flint cutting head"),
        "{ordinary}"
    );
    assert!(convenient.contains("Begin: advanced"), "{convenient}");
    assert!(
        convenient.contains("Started knap flint cutting head"),
        "{convenient}"
    );
    assert_eq!(snapshot(&manual), snapshot(&aligned));
    let busy = snapshot(&aligned);
    assert!(aligned.command("begin craft 99").is_err());
    assert_eq!(
        snapshot(&aligned),
        busy,
        "busy work must not advance the clock"
    );

    let mut rejected = Session::new(24318).unwrap();
    rejected.command("wait 1").unwrap();
    let error = rejected.command("begin craft 999").unwrap_err();
    assert!(
        error.contains("rejected after advancing 71 control ticks"),
        "{error}"
    );
    assert_eq!(rejected.state.control_tick().value(), 72);
    assert!(rejected.state.player_work().active().is_none());
}

#[test]
fn eat_rejects_shifted_non_food_inventory_rows_without_spending_time() {
    use deep_hearth::content::{FORM_WINTER_PROVISIONS, MATERIAL_WILD_NUTS};
    use deep_hearth::material::CommodityKey;

    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "s 20",
        "wait 10",
        "s 20",
        "wait 10",
        "gather 2 300",
        "wait work",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let inventory = project_player_inventory(&session.registries, &session.state).unwrap();
    let (index, stack) = inventory
        .carried()
        .stacks()
        .iter()
        .enumerate()
        .find(|(_, stack)| {
            session
                .registries
                .survival()
                .get_food(stack.commodity())
                .is_none()
        })
        .expect("opening stone/flint should not be edible");
    let name = session
        .registries
        .materials()
        .commodity_name(stack.commodity())
        .unwrap()
        .to_owned();
    let before = snapshot(&session);
    let error = session.command(&format!("eat {}", index + 1)).unwrap_err();
    assert!(
        error.contains(&format!("({name}) is not edible")),
        "{error}"
    );
    assert_eq!(snapshot(&session), before);

    // Edibility is defined by the survival registry, not by the FORM_FOOD
    // label alone: preserved winter provisions must remain valid food.
    assert!(
        session
            .registries
            .survival()
            .get_food(CommodityKey::new(
                MATERIAL_WILD_NUTS,
                FORM_WINTER_PROVISIONS
            ))
            .is_some()
    );
}

#[test]
fn centering_consumes_real_control_ticks_and_keeps_the_current_cell() {
    use deep_hearth::spatial::{HorizontalVelocity, WorldPoint};
    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10"] {
        session.command(command).unwrap();
    }
    let here = session.player_position().unwrap();
    let target = WorldPoint::centered_on_voxel_floor(here).unwrap();
    let before = session.state.control_tick();
    let (report, _) = session.command("center").unwrap();
    assert!(report.contains("Centred in the current cell"), "{report}");
    assert!(session.state.control_tick() > before);
    assert_eq!(session.player_position().unwrap(), here);
    let body = session.state.logistics().player().unwrap().body();
    assert!(
        body.foot_position()
            .x_nanometers()
            .abs_diff(target.x_nanometers())
            <= 30_000_000
    );
    assert!(
        body.foot_position()
            .z_nanometers()
            .abs_diff(target.z_nanometers())
            <= 30_000_000
    );
    assert_eq!(body.horizontal_velocity(), HorizontalVelocity::ZERO);
    let settled = snapshot(&session);
    session.command("center").unwrap();
    assert_eq!(snapshot(&session), settled);
}

#[test]
fn directional_jump_can_regain_a_one_voxel_ledge_on_the_played_seed() {
    let mut session = Session::new(80705).unwrap();
    for command in [
        "s 20", "wait 10", "w 20", "wait 10", "w 20", "wait 10", "w 20", "wait 10",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    assert_eq!(
        session.player_position().unwrap(),
        VoxelCoord::new(5, -2, 9)
    );
    let (jump, _) = session.command("jump e 40").unwrap();
    assert_eq!(
        session.player_position().unwrap(),
        VoxelCoord::new(6, -1, 9),
        "directional jump should clear the one-voxel rise toward the flint outcrop: {jump}"
    );
}

#[test]
fn directional_jump_rebuilds_velocity_after_blocked_launch_and_matches_one_impulse() {
    let mut played = Session::new(24318).unwrap();
    let mut reference = Session::new(24318).unwrap();
    for _ in 0..7 {
        played.command("n 240").unwrap();
        reference.command("n 240").unwrap();
    }
    let before = played.player_position().unwrap();
    let (report, _) = played.command("jump n 240").unwrap();
    let after = played.player_position().unwrap();
    assert_eq!(
        after.z(),
        before.z() - 1,
        "the jump must cross the previously blocked ledge: {report}"
    );
    assert_eq!(
        after.y(),
        before.y() + 1,
        "the ledge must really be cleared, not tunneled through: {report}"
    );
    let elapsed: u64 = report
        .lines()
        .find_map(|line| line.strip_prefix("Advanced "))
        .and_then(|line| line.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(elapsed > 1 && elapsed < 240, "{report}");
    let north = HorizontalControlInput::new(0, -1_000_000).unwrap();
    for step in 0..elapsed {
        reference
            .step(
                PlayerDirectControlInput::new(PlayerControlInput::new(north).with_jump(step == 0)),
                &mut String::new(),
            )
            .unwrap();
    }
    assert_eq!(snapshot(&played), snapshot(&reference));
}

#[test]
fn fire_command_consumes_the_exact_gathered_stack_and_conserves_matter() {
    use deep_hearth::content::{FORM_LOG, MATERIAL_WOOD};
    use deep_hearth::core::quantity::Mass;
    use deep_hearth::core::state::validate_loaded_state;
    use deep_hearth::material::CommodityKey;
    use deep_hearth::matter::calculate_matter_accounting;

    fn finish_work(session: &mut Session) {
        for _ in 0..20 {
            if session.state.player_work().active().is_none() {
                break;
            }
            session.command("wait work").unwrap();
        }
        assert!(session.state.player_work().active().is_none());
    }

    let mut session = Session::new(24318).unwrap();
    // Reach the stone source along the real opening trail and construct a fire.
    for command in [
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "e 20",
        "wait 10",
        "gather 1 1000",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    finish_work(&mut session);
    let (built, _) = session.command("build-fire").unwrap();
    assert!(built.contains("Built a stone-ring campfire"), "{built}");

    // Bring back a half charge and burn it without waiting for a full log.
    for command in ["w 20", "wait 10", "s 20", "wait 10", "gather 1 500"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    finish_work(&mut session);
    for command in ["n 20", "wait 10", "e 20", "wait 10"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let projected = project_player_inventory(&session.registries, &session.state).unwrap();
    let source = projected.carried().stockpile();
    let wood = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let short_index = projected
        .carried()
        .stacks()
        .iter()
        .position(|stack| stack.commodity() == wood)
        .unwrap_or_else(|| panic!("ordinary gathered timber must be carried"))
        + 1;
    let catalog = session.command("fire").unwrap().0;
    assert!(
        catalog.contains(&format!("feed-fire 1 {short_index} (500 g, 6.000 MJ)")),
        "{catalog}"
    );
    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    let (first, _) = session
        .command(&format!("feed-fire 1 {short_index}"))
        .unwrap();
    assert!(first.contains("Burned 500 g timber log"), "{first}");
    assert!(first.contains("6.000 MJ"), "{first}");
    assert_eq!(
        session
            .state
            .inventory()
            .get_stockpile(source)
            .unwrap()
            .get_mass(wood),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );

    // The next half charge is gathered rather than borrowed from another cohort.
    for command in ["w 20", "wait 10", "s 20", "wait 10", "gather 1 500"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    finish_work(&mut session);
    for command in ["n 20", "wait 10", "e 20", "wait 10"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let projection = project_player_inventory(&session.registries, &session.state).unwrap();
    let index = projection
        .carried()
        .stacks()
        .iter()
        .position(|stack| {
            stack.commodity() == wood && stack.mass() == Mass::from_milligrams(500_000)
        })
        .unwrap_or_else(|| panic!("the second half charge must be carried"))
        + 1;
    let catalog = session.command("fire").unwrap().0;
    assert!(
        catalog.contains(&format!("feed-fire 1 {index}")),
        "{catalog}"
    );
    let conserved = calculate_matter_accounting(&session.state).unwrap().total();
    let (receipt, _) = session.command(&format!("feed-fire 1 {index}")).unwrap();
    assert!(receipt.contains("Burned 500 g timber log"), "{receipt}");
    // The first charge has dissipated some heat during the second gathering trip.
    assert!(receipt.contains("thermal store now has "), "{receipt}");
    assert_eq!(
        session
            .state
            .inventory()
            .get_stockpile(source)
            .unwrap()
            .get_mass(wood),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        conserved
    );
    assert_eq!(
        validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
    let saved = snapshot(&session);
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &replay)).unwrap(),
        saved
    );
}

#[test]
fn dropped_material_is_visibly_distinct_from_natural_supply_and_conserves_matter() {
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "gather 1 0.125", "wait work", "select 1"] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let before = calculate_matter_accounting(&session.state).unwrap().total();
    let (drop, _) = session.command("drop").unwrap();
    assert!(
        drop.contains("Drop: Dropped 0.125 g berries at (9,2,8)."),
        "{drop}"
    );
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        before
    );
    let (look, _) = session.command("look").unwrap();
    assert!(look.contains("  berries here at (9,2,8)"), "{look}");
    assert!(
        look.contains("  berries (loose drop) here at (9,2,8)"),
        "{look}"
    );

    // This is the genuine delayed control-rate pickup, not a synthesized
    // transcript. It must stop the wait at the first eligible frame and say
    // what arrived, not expose surface or inventory lot identities.
    let (picked, _) = session.command("wait 60").unwrap();
    assert!(
        picked.contains("Picked up: 0.125 g berries entered your pack."),
        "{picked}"
    );
    assert!(picked.contains("Advanced 40 control ticks"), "{picked}");
    for private_type in [
        "SurfaceDropPickupOutcome",
        "MaterialLotId",
        "SurfaceResourceId",
    ] {
        assert!(!picked.contains(private_type), "{picked}");
    }
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        before
    );
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("berries | 0.125 g"), "{inventory}");
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn pickup_receipt_preserves_commodity_when_same_frame_drop_removes_its_lot() {
    use deep_hearth::matter::calculate_matter_accounting;

    let mut session = Session::new(24318).unwrap();
    for command in ["e 20", "wait 10", "gather 1 0.125", "wait work", "select 1"] {
        session.command(command).unwrap();
    }
    let before = calculate_matter_accounting(&session.state).unwrap().total();
    session.command("drop").unwrap();
    let (almost, _) = session.command("wait 39").unwrap();
    assert!(!almost.contains("Picked up:"), "{almost}");

    // The pickup precedes direct actions in the same canonical control frame.
    // Its material name must survive the immediate re-drop of the new lot.
    let (both, _) = session.command("drop").unwrap();
    assert!(
        both.contains("Picked up: 0.125 g berries entered your pack."),
        "{both}"
    );
    assert!(both.contains("Drop: Dropped 0.125 g berries"), "{both}");
    assert!(
        both.find("Picked up:").unwrap() < both.find("Drop:").unwrap(),
        "same-frame feedback must follow physical event order: {both}"
    );
    assert!(!both.contains("0.125 g material"), "{both}");
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(inventory.contains("Carried matter: 0 g"), "{inventory}");
    assert_eq!(
        calculate_matter_accounting(&session.state).unwrap().total(),
        before
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn direct_action_rejections_and_hotbar_changes_are_plain_language() {
    let mut session = Session::new(24318).unwrap();
    for (command, expected, forbidden) in [
        ("slot 2", "Slot: 2 (empty)", "PlayerQuickAccessSlot"),
        (
            "use",
            "Use: Nothing usable is selected here.",
            "NoActiveSelection",
        ),
        (
            "break",
            "Primary: Block breaking requires a carried tool selected in the hotbar.",
            "BlockExcavationRejected",
        ),
        (
            "drop",
            "Drop: Nothing is selected to drop.",
            "NoActiveSelection",
        ),
    ] {
        let (response, _) = session.command(command).unwrap();
        assert!(response.contains(expected), "{command}: {response}");
        assert!(!response.contains(forbidden), "{command}: {response}");
    }
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
            quantity::parse_decimal_grams(text, "gather quantity"),
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
        assert!(
            quantity::parse_decimal_grams(text, "gather quantity").is_err(),
            "{text}"
        );
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
fn campfire_catalog_lists_each_physical_fuel_cohort() {
    let mut session = stone_campfire_from_wilderness();
    // Preserve the gathered mass but model two distinct thermal cohorts in a
    // current-schema save, then admit it through the normal load boundary.
    let mut saved: serde_json::Value = serde_json::from_slice(&snapshot(&session)).unwrap();
    let inventory = &mut saved["state"]["systems"]["inventory"];
    let next = inventory["next_lot_id"].as_u64().unwrap();
    let lots = inventory["lots"].as_object_mut().unwrap();
    assert_eq!(lots.len(), 1);
    let original = lots.values_mut().next().unwrap();
    let mass = original["mass"].as_u64().unwrap();
    let mut second = original.clone();
    original["mass"] = serde_json::json!(1);
    second["mass"] = serde_json::json!(mass - 1);
    second["id"] = serde_json::json!(next);
    second["profile"]["temperature"] = serde_json::json!(280_000);
    lots.insert(next.to_string(), second);
    inventory["next_lot_id"] = serde_json::json!(next + 1);
    session.state = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();

    let before = snapshot(&session);
    let (catalog, _) = session.command("fire").unwrap();
    assert!(catalog.contains("feed-fire 1 1 ("), "{catalog}");
    assert!(catalog.contains("feed-fire 1 2 ("), "{catalog}");
    assert!(catalog.contains("0.001 g, 0.000012 MJ"), "{catalog}");
    assert_eq!(snapshot(&session), before);
    let (burned, _) = session.command("feed-fire 1 1").unwrap();
    assert!(burned.contains("Burned"), "{burned}");
    let (remaining, _) = session.command("fire").unwrap();
    assert!(remaining.contains("feed-fire 1 1 ("), "{remaining}");
    assert!(!remaining.contains("feed-fire 1 2 ("), "{remaining}");
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
    assert!(catalog.contains("Fuel: up to 1000 g timber log per charge"));
    assert!(catalog.contains("Fuel: up to 500 g wood chips per charge"));
    assert!(catalog.contains("feed-fire 1 1"));
    let before_invalid = snapshot(&session);
    let bad_fuel = session.command("feed-fire 1 2").unwrap_err();
    assert!(bad_fuel.contains("no carried material at that stack index"));
    assert_eq!(snapshot(&session), before_invalid);

    // In mild weather paid construction can leave the survivor fully warm.
    // Observe real unfueled cooling before testing recovery from a fuel charge.
    for _ in 0..12 {
        if session
            .state
            .survival()
            .player()
            .unwrap()
            .thermal_comfort_ppm()
            < 990_000
        {
            break;
        }
        session.command("wait 7200").unwrap();
    }
    assert!(
        session
            .state
            .survival()
            .player()
            .unwrap()
            .thermal_comfort_ppm()
            < 1_000_000,
        "the unfueled worksite must lose some comfort before the recovery comparison"
    );

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
    let fire_stored = session
        .state
        .available_local_energy_stores()
        .next()
        .unwrap()
        .stored();
    assert!(fire_stored > deep_hearth::core::quantity::Energy::ZERO);
    assert!(fire_stored < deep_hearth::content::CAMPFIRE_FUEL_THERMAL_ENERGY);
    assert!(fire_after.contains(&format!(
        "{} stored",
        super::fire::energy_label(fire_stored)
    )));
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
    // The combustion owner accepts a bounded partial fuel charge, so the
    // remaining 300 g of conserved chips can be burned rather than discarded.
    assert!(
        session
            .command("fire")
            .unwrap()
            .0
            .contains("5.000 MJ stored")
    );
    let (second, _) = session.command("feed-fire 1 1").unwrap();
    assert!(second.contains("Burned 300 g wood chips"), "{second}");
    let after_two_charges = snapshot(&session);
    assert!(session.command("feed-fire 1 1").is_err());
    assert_eq!(snapshot(&session), after_two_charges);
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
            .contains("8.000 MJ stored")
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
    assert!(queued.contains("Action queued for the next simulation boundary."));
    let (admitted, _) = session.command("wait 72").unwrap();
    assert!(admitted.contains("Structural construction started."));
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
    let (aimed, _) = session.command("look e-down").unwrap();
    assert!(
        aimed.contains("Break target: framing at (11,2,9) | 3250 g."),
        "the pointed look must identify the structural break target: {aimed}"
    );
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        total,
    );
    session.command("empty").unwrap();
    let (broken, _) = session.command("break e-down").unwrap();
    assert!(broken.contains("Action queued"), "{broken}");
    let (started, _) = session.command("wait boundary").unwrap();
    assert!(
        started.contains("Structural dismantling started"),
        "{started}"
    );
    let (recovered, _) = session.command("wait work").unwrap();
    assert!(
        recovered.contains(
            "Dismantling finished: Recovered 3250 g of structural materials into your pack."
        ),
        "{recovered}"
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
fn trek_distinguishes_new_sightings_from_changes_in_observation_range() {
    let mut session = Session::new(24318).unwrap();
    let (approach, _) = session.command("trek e 1 steady").unwrap();
    assert_eq!(session.state.tick().value(), 0);
    assert!(approach.contains("Trekked 1 of 1 cells"), "{approach}");
    assert!(
        !approach.contains("resident has approached"),
        "the resident did not take a simulation step: {approach}"
    );
    let (travel, _) = session.command("trek s 2").unwrap();
    assert!(travel.contains("Trekked 2 of 2 cells"), "{travel}");
}

#[test]
fn natural_campaign_trek_reaches_finite_wood_and_stone_without_injury() {
    let mut session = Session::new_natural(4_909_227_005_684_505_656).unwrap();
    assert!(
        session
            .opening()
            .contains("Distant visible resource: timber log")
    );
    let before = snapshot(&session);
    for invalid in [
        "trek up",
        "trek e 0",
        "trek n 33",
        "trek",
        "trek steady",
        "descend up",
    ] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    assert_eq!(snapshot(&session), before);
    let mut north = Session::new_natural(4_909_227_005_684_505_656).unwrap();
    let (cliff, _) = north.command("trek n 8 steady").unwrap();
    assert!(cliff.contains("Trekked 5 of 8 cells"), "{cliff}");
    assert!(
        cliff.contains("no clear level or one-metre foothold"),
        "{cliff}"
    );
    let stopped = snapshot(&north);
    assert!(
        north
            .command("trek n 8 steady")
            .unwrap()
            .0
            .contains("Trekked 0 of 8 cells in 0 control ticks")
    );
    assert_eq!(snapshot(&north), stopped);
    let (frame, _) = session.command("trek s 6").unwrap();
    assert!(frame.contains("Trekked 6 of 6 cells"), "{frame}");
    for command in ["trek w 1", "trek s 1", "gather 1 3000", "wait work"] {
        session.command(command).unwrap();
    }
    let (pack, _) = session.command("inventory").unwrap();
    assert!(pack.contains("timber log | 3000 g"), "{pack}");
    for command in ["trek e 1", "trek s 2", "trek s 4", "trek s 3", "trek w 1"] {
        session.command(command).unwrap();
    }
    let (frame, _) = session.command("look").unwrap();
    assert!(frame.contains("stone here at (-363,8,-197)"), "{frame}");
    assert!(frame.contains("available 2770.529 g"), "{frame}");
    let replay_bytes = snapshot(&session);
    let mut replay = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&replay_bytes)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    for command in ["gather 1 2000", "wait work", "inventory", "fire"] {
        let live = session.command(command).unwrap();
        assert_eq!(live, replay.command(command).unwrap());
    }
    assert_eq!(snapshot(&session), snapshot(&replay));
    let (pack, _) = session.command("inventory").unwrap();
    assert!(pack.contains("stone | 2000 g"), "{pack}");
    assert!(
        session
            .command("fire")
            .unwrap()
            .0
            .contains("Campfire construction (build-fire): ready")
    );
    for command in [
        "build-fire",
        "craft 7",
        "wait finish",
        "craft 8",
        "wait finish",
        "assemble 6",
        "take 1",
        "feed-fire 1 1",
    ] {
        let live = session.command(command).unwrap();
        assert_eq!(live, replay.command(command).unwrap());
    }
    assert_eq!(snapshot(&session), snapshot(&replay));
    let pack = session.command("inventory").unwrap().0;
    assert!(
        pack.contains("Carried equipment 1: knapped stone pick | 1000 g"),
        "{pack}"
    );
    let fire = session.command("fire").unwrap().0;
    assert!(fire.contains("12.000 MJ stored"), "{fire}");
    assert_eq!(
        session.state.survival().player().unwrap().vitality(),
        deep_hearth::survival::Vitality::MAXIMUM
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn natural_look_identifies_only_actually_visible_tree_species() {
    let first = Session::new_natural(4).unwrap();
    let second = Session::new_natural(4).unwrap();
    let look = first.opening();
    assert!(
        look.contains("Visible juniper"),
        "real seed 4 has an illuminated, unobstructed nearby juniper: {look}",
    );
    assert_eq!(
        look,
        second.opening(),
        "the observed species must replay from the seed"
    );
    assert!(
        !look.contains("TreeRecord"),
        "the player should see species names rather than debug identities",
    );
}

#[test]
fn natural_worlds_replay_across_seeds_without_a_scripted_starter_trail() {
    use deep_hearth::content::{FORM_NATIVE_METAL, MATERIAL_COPPER};
    use deep_hearth::material::CommodityKey;
    use deep_hearth::matter::calculate_matter_accounting;
    use deep_hearth::world::{HabitatKind, assess_player_habitat};

    let mut habitats = [false; 3];
    let mut sites = std::collections::BTreeSet::new();
    let mut negative = false;
    let mut positive = false;
    for seed in 0..32_u64 {
        let first = Session::new_natural(seed).unwrap();
        let second = Session::new_natural(seed).unwrap();
        assert_eq!(
            snapshot(&first),
            snapshot(&second),
            "seed {seed} must fully replay"
        );
        assert_eq!(
            first
                .state
                .available_adjacent_traversal_destinations()
                .count(),
            0,
            "natural world must not inject the seven-cell tutorial graph"
        );
        assert!(first.state.world().chunks().count() >= 9);
        assert!(
            first.state.available_surface_resources().all(|source| {
                source.commodity() != CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)
            }),
            "natural start must not inject free metal"
        );
        let at = first.player_position().unwrap();
        sites.insert((at.x(), at.y(), at.z()));
        negative |= at.x() < 0 || at.z() < 0;
        positive |= at.x() > 0 || at.z() > 0;
        let kind = assess_player_habitat(&first.state).expect("generated ground has habitat");
        habitats[match kind {
            HabitatKind::DampGrove => 0,
            HabitatKind::MixedWoodland => 1,
            HabitatKind::DryScrub => 2,
        }] = true;
        assert_eq!(
            deep_hearth::core::state::validate_loaded_state(&first.registries, &first.state),
            Ok(())
        );
        assert!(calculate_matter_accounting(&first.state).is_ok());
    }
    assert_eq!(
        habitats, [true; 3],
        "32 natural starts must span all three ecological pressures"
    );
    assert!(
        negative && positive,
        "seeded spawns must exercise signed-world coordinates"
    );
    assert!(
        sites.len() >= 24,
        "seeds must not merely rotate one authored clearing"
    );
}

#[test]
fn requested_habitats_select_real_standing_ground_and_survive_save_replay() {
    use deep_hearth::world::{HabitatKind, assess_player_habitat};
    for habitat in [
        HabitatKind::DryScrub,
        HabitatKind::MixedWoodland,
        HabitatKind::DampGrove,
    ] {
        let first = Session::new_natural_in_habitat(1_837_596_006, Some(habitat)).unwrap();
        let second = Session::new_natural_in_habitat(1_837_596_006, Some(habitat)).unwrap();
        assert_eq!(snapshot(&first), snapshot(&second));
        assert_eq!(assess_player_habitat(&first.state), Some(habitat));
        let body = first.player_position().unwrap();
        assert_eq!(first.state.world().block_at(body), Some(BlockStateId::AIR));
        assert_eq!(
            first
                .state
                .world()
                .block_at(VoxelCoord::new(body.x(), body.y() - 1, body.z())),
            Some(BLOCK_STATE_STONE)
        );
        let bytes = snapshot(&first);
        let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&first.registries)
            .unwrap();
        assert_eq!(
            serde_json::to_vec(&SaveEnvelope::new(&first.registries, &restored)).unwrap(),
            bytes
        );
    }
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
    assert!(first.opening().contains("Visible water 1 east, 1 south"));
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
    assert!(available.contains("2. flint nodule"));
    let (started, _) = session.command("gather 2 300").unwrap();
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
        "gather 2 300",
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
        "gather 2 300",
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

#[test]
fn local_crafting_uses_equipment_without_picking_it_up() {
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
        "drop",
        "wait boundary",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let (recipes, _) = session.command("recipes").unwrap();
    assert!(
        recipes.contains("at 1 (hafted flint woodworking knife)"),
        "{recipes}"
    );
    let before = snapshot(&session);
    assert!(session.command("craft 14 at 2").is_err());
    assert!(session.command("craft 14 with 1 at 1").is_err());
    assert_eq!(snapshot(&session), before);
    let (started, _) = session.command("craft 14 at 1").unwrap();
    assert!(
        started.contains("Work: shape timber boards; scheduled completion in 1m 12s"),
        "{started}"
    );
    let (done, _) = session.command("wait work").unwrap();
    assert!(
        done.contains("Received 800 g timber board (carried)"),
        "{done}"
    );
    let (gear, _) = session.command("gear").unwrap();
    assert!(gear.contains("hafted flint woodworking knife"), "{gear}");
    let (inventory, _) = session.command("inventory").unwrap();
    assert!(
        !inventory.contains("Carried equipment 1: hafted flint woodworking knife"),
        "{inventory}"
    );
    assert_eq!(
        deep_hearth::core::state::validate_loaded_state(&session.registries, &session.state),
        Ok(())
    );
}

#[test]
fn ceramic_workshop_fires_and_cools_the_gathered_player_stack() {
    use deep_hearth::content::{FORM_FIRED_CROCK, MATERIAL_CLAY};
    use deep_hearth::core::quantity::{Mass, Temperature};
    use deep_hearth::material::CommodityKey;

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
        "gather 1 3000",
        "wait work",
        "craft 101",
        "wait work",
        "craft 61 2",
        "wait work",
        "craft 14",
        "wait work",
        "assemble 61",
        "build-heat 16",
        "build-heat 17",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    let before = snapshot(&session);
    let (catalog, _) = session.command("thermal").unwrap();
    assert!(
        catalog.contains("fueled stone pit-kiln hearth"),
        "{catalog}"
    );
    assert!(catalog.contains("stone ceramic cooling bed"), "{catalog}");
    assert!(
        catalog.contains("unfired") || catalog.contains("Stack"),
        "{catalog}"
    );
    // Select by observed physical cohort, never by a hidden material lot ID.
    let stack_index = |session: &Session, key| {
        project_player_inventory(&session.registries, &session.state)
            .unwrap()
            .carried()
            .stacks()
            .iter()
            .position(|stack| stack.commodity() == key)
            .unwrap()
            + 1
    };
    use deep_hearth::content::{FORM_GREENWARE_CROCK, FORM_LOG, MATERIAL_WOOD};
    let green = CommodityKey::new(MATERIAL_CLAY, FORM_GREENWARE_CROCK);
    let fired = CommodityKey::new(MATERIAL_CLAY, FORM_FIRED_CROCK);
    let firing = format!("heat 102 {} 1 1", stack_index(&session, green));
    assert!(
        session.command(&firing).is_err(),
        "an empty hearth cannot fire"
    );
    assert_eq!(snapshot(&session), before);
    let fuel = format!(
        "feed-heat 1 {}",
        stack_index(&session, CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
    );
    session.command(&fuel).unwrap();
    let firing = format!("heat 102 {} 1 1", stack_index(&session, green));
    let (catalog, _) = session.command("thermal").unwrap();
    assert!(catalog.contains(&format!("{firing} | ready")), "{catalog}");
    session.command(&firing).unwrap();
    let finish = |session: &mut Session| {
        for _ in 0..200 {
            if session.state.production().jobs().next().is_none() {
                return;
            }
            session.command("wait 7200").unwrap();
        }
        panic!("thermal batch did not complete");
    };
    finish(&mut session);
    let hot = project_player_inventory(&session.registries, &session.state).unwrap();
    let hot = &hot.carried().stacks()[stack_index(&session, fired) - 1];
    assert_eq!(hot.mass(), Mass::from_milligrams(1_600_000));
    assert_eq!(hot.temperature(), Temperature::from_millikelvin(1_100_000));
    session.command("wait boundary").unwrap();
    let cooling = format!("heat 104 {} 1 2", stack_index(&session, fired));
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
    finish(&mut session);
    finish(&mut restored);
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

    assert!(session.command("stash 1 1 400.001").is_err());
    assert_eq!(snapshot(&session), before_rejections);
    let (partial, _) = session.command("stash 1 1 125.375").unwrap();
    assert!(
        partial.contains("Stashed 125.375 g wood chips"),
        "{partial}"
    );
    let before_partial_rejection = snapshot(&session);
    assert!(session.command("withdraw 1 1 125.376").is_err());
    assert_eq!(snapshot(&session), before_partial_rejection);
    let (partial, _) = session.command("withdraw 1 1 125.375").unwrap();
    assert!(
        partial.contains("Withdrew 125.375 g wood chips"),
        "{partial}"
    );
    assert_eq!(
        deep_hearth::matter::calculate_matter_accounting(&session.state)
            .unwrap()
            .total(),
        before_placement
    );

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
    let mut session = Session::new_natural_in_habitat(
        13_079_275_461_686_118_933,
        Some(deep_hearth::world::HabitatKind::DampGrove),
    )
    .unwrap();
    let threshold = session
        .registries
        .survival()
        .physiology()
        .thermal_comfort()
        .warning_below_ppm();
    let initial = view::urgency(&session.registries, &session.state);
    assert!(!initial.unwrap().requires_provisions_or_warmth());
    let mut reached = false;
    for _ in 0..120 {
        let assessment = assess_survival(&session.registries, &session.state).unwrap();
        if assessment.thermal_comfort_state() != ThermalComfortState::Comfortable {
            let comfort = assessment.thermal_comfort_ppm();
            assert!(comfort < threshold);
            assert!(
                view::urgency(&session.registries, &session.state)
                    .unwrap()
                    .requires_provisions_or_warmth()
            );
            assert!(view::urgency(&session.registries, &session.state) != initial);
            assert!(
                view::status(&session.registries, &session.state)
                    .contains("Warnings: cold exposure")
            );
            reached = true;
            break;
        }
        session.command("wait 7200").unwrap();
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
        "gather 2 300",
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
