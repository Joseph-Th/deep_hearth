//! Boundary alignment, exact work intent and queued admission regressions.

use super::*;

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
fn begin_does_not_retarget_work_when_momentum_changes_the_work_location() {
    use deep_hearth::core::time::{DayPhase, SimulationTick};

    let mut session = Session::new(24318).unwrap();
    let night = session
        .registries
        .core()
        .calendar()
        .ticks_until_day_phase(SimulationTick::ZERO, DayPhase::Night)
        .unwrap()
        .value()
        + 1;
    let mut saved: serde_json::Value = serde_json::from_slice(&snapshot(&session)).unwrap();
    saved["state"]["clock"]["simulation_tick"] = serde_json::json!(night);
    saved["state"]["clock"]["control_tick"] =
        serde_json::json!(night * CONTROL_TICKS_PER_SIMULATION_TICK);
    // Isolate movement from the opening colony's independent schedule.
    saved["state"]["systems"]["kobolds"]["colony"] = serde_json::Value::Null;
    session.state = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    for command in ["e 1", "wait 10", "e 3"] {
        session.command(command).unwrap();
    }
    let before = session.player_position().unwrap();
    let mut reference = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&snapshot(&session))
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    reference.command("wait boundary").unwrap();
    let (report, _) = session.command("begin gather 1 100").unwrap();
    assert_ne!(session.player_position().unwrap(), before, "{report}");
    assert!(session.state.player_work().active().is_none(), "{report}");
    assert!(report.contains("your location changed"), "{report}");
    assert_eq!(snapshot(&session), snapshot(&reference));
}

/// An actual seeded crop senesces on the next slow tick. A second fresh
/// berry source at the same site retains the scent, while the night and
/// unchanged regional weather conceal the disappearance from sensory cues.
fn prepare_gather_row_shift_at_boundary() -> Session {
    let mut session = Session::new(24318).unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&snapshot(&session)).unwrap();
    let due = 66_600;
    saved["state"]["clock"]["simulation_tick"] = serde_json::json!(due - 1);
    saved["state"]["clock"]["control_tick"] =
        serde_json::json!(due * CONTROL_TICKS_PER_SIMULATION_TICK - 1);
    let systems = &mut saved["state"]["systems"];
    systems["kobolds"]["colony"] = serde_json::Value::Null;
    let resources = &mut systems["surface"]["resources"];
    let position = resources["13"]["position"].clone();
    assert_eq!(position, serde_json::json!({"x": 3, "y": 1, "z": -5}));
    resources["13"]["perennial"]["ripened_at"] = serde_json::json!(due - 48_000);
    resources["15"]["position"] = position.clone();
    resources["3"]["position"] = position;
    resources["3"]["created_at"] = serde_json::json!(due - 1);
    resources["3"]["material"]["provenance"] = serde_json::json!({
        "earliest_created_at": due - 1,
        "latest_created_at": due - 1,
    });
    systems["logistics"]["player"]["body"]["foot_position"] = serde_json::json!({
        "x_nanometers": 3_500_000_000_i64,
        "y_nanometers": 1_000_000_000_i64,
        "z_nanometers": -4_500_000_000_i64,
    });
    session.state = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&session.registries)
        .unwrap();
    let options = player_surface_gathering_options(&session.registries, &session.state).unwrap();
    assert_eq!(
        options
            .iter()
            .map(|option| option.resource().value())
            .collect::<Vec<_>>(),
        vec![3, 13, 15]
    );
    session
}

#[test]
fn begin_gather_does_not_retarget_a_depleted_row_at_the_same_location() {
    for command in [
        "begin gather 2",
        "begin gather 2 100",
        "begin gather 2 repeat 2",
        "begin gather 2 100 repeat 2",
    ] {
        let mut session = prepare_gather_row_shift_at_boundary();
        let mut reference = prepare_gather_row_shift_at_boundary();
        let position = session.player_position().unwrap();
        reference.command("wait boundary").unwrap();
        let options =
            player_surface_gathering_options(&reference.registries, &reference.state).unwrap();
        assert_eq!(
            options
                .iter()
                .map(|option| option.resource().value())
                .collect::<Vec<_>>(),
            vec![3, 15]
        );
        let (report, _) = session.command(command).unwrap();
        assert_eq!(session.player_position().unwrap(), position, "{report}");
        assert!(session.state.player_work().active().is_none(), "{report}");
        assert!(report.contains("selected gathering source"), "{report}");
        assert_eq!(snapshot(&session), snapshot(&reference));
    }
}

#[test]
fn begin_gather_keeps_a_surviving_source_when_its_row_moves() {
    for suffix in ["", " 100", " repeat 2", " 100 repeat 2"] {
        let mut session = prepare_gather_row_shift_at_boundary();
        let mut reference = prepare_gather_row_shift_at_boundary();
        reference.command("wait boundary").unwrap();
        let expected = reference.command(&format!("gather 2{suffix}")).unwrap().0;
        let report = session
            .command(&format!("begin gather 3{suffix}"))
            .unwrap()
            .0;
        assert!(expected.contains("Gathering started"), "{expected}");
        assert!(report.contains("Gathering started"), "{report}");
        assert_eq!(snapshot(&session), snapshot(&reference));
    }
}

#[test]
fn queued_finish_stops_at_admission_when_pickup_occurs_on_the_same_frame() {
    use deep_hearth::content::{FORM_BOARD, FORM_CHIP, MATERIAL_WOOD};
    use deep_hearth::material::CommodityKey;

    let mut session = Session::new(80705).unwrap();
    prepare_timber_panel_materials(&mut session);
    let select = |session: &mut Session, form| {
        let inventory = project_player_inventory(&session.registries, &session.state).unwrap();
        let index = inventory
            .carried()
            .stacks()
            .iter()
            .position(|stack| stack.commodity() == CommodityKey::new(MATERIAL_WOOD, form))
            .unwrap()
            + 1;
        session.command(&format!("select {index}")).unwrap();
    };
    select(&mut session, FORM_CHIP);
    for command in ["wait boundary", "wait 31", "drop"] {
        session.command(command).unwrap();
    }
    assert_eq!(
        session.state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK,
        32
    );
    select(&mut session, FORM_BOARD);
    let (queued, _) = session.command("use e-shallow").unwrap();
    assert!(queued.contains("Action queued"), "{queued}");
    let before = snapshot(&session);
    let mut reference = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (admission, _) = reference.command("wait work").unwrap();
    assert!(admission.contains("Picked up:"), "{admission}");
    assert!(
        admission.contains("Structural construction started"),
        "{admission}"
    );
    let (report, _) = session.command("wait finish").unwrap();
    assert!(report.contains("Picked up:"), "{report}");
    assert!(
        report.contains("Structural construction started"),
        "{report}"
    );
    assert!(!report.contains("Construction finished"), "{report}");
    assert!(session.state.player_work().active().is_some());
    assert_eq!(snapshot(&session), snapshot(&reference));
}

#[test]
fn begin_does_not_start_work_after_pickup_on_the_alignment_boundary() {
    let mut session = Session::new(24318).unwrap();
    for command in [
        "e 20",
        "wait 10",
        "gather 1 100",
        "wait work",
        "select 1",
        "wait boundary",
        "wait 31",
        "drop",
    ] {
        session
            .command(command)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
    assert_eq!(
        session.state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK,
        32,
        "the dropped parcel's 40-control-tick grace period ends on the next boundary"
    );
    let before = snapshot(&session);
    let mut reference = Session {
        registries: build_registries(),
        state: serde_json::from_slice::<LoadedSaveEnvelope>(&before)
            .unwrap()
            .into_state(&session.registries)
            .unwrap(),
        seed: None,
    };
    let (ordinary, _) = reference.command("wait boundary").unwrap();
    assert!(ordinary.contains("Picked up:"), "{ordinary}");
    assert_eq!(
        reference.state.control_tick().value() % CONTROL_TICKS_PER_SIMULATION_TICK,
        0
    );
    let (report, _) = session.command("begin gather 1 100").unwrap();
    assert!(report.contains("Picked up:"), "{report}");
    assert!(report.contains("Begin interrupted"), "{report}");
    assert!(session.state.player_work().active().is_none());
    assert_eq!(snapshot(&session), snapshot(&reference));
}
