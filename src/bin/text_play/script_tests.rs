//! Command-file execution contracts for the text-play adapter.

use super::*;

#[test]
fn repeated_startup_options_are_rejected_instead_of_silently_changing_campaign() {
    for args in [
        ["--seed", "17", "--seed", "19"],
        ["--load", "first", "--load", "second"],
        ["--script", "one.script", "--script", "two.script"],
    ] {
        let parsed = parse_arguments_from(args.map(str::to_owned));
        assert!(parsed.is_err(), "repeated option accepted: {args:?}");
    }
}

#[test]
fn one_startup_source_and_script_remain_independently_selectable() {
    let seeded =
        parse_arguments_from(["--seed", "17", "--script", "journey.script"].map(str::to_owned))
            .unwrap();
    assert_eq!(seeded.seed, Some(17));
    assert_eq!(seeded.load, None);
    assert_eq!(seeded.script, Some(PathBuf::from("journey.script")));

    let loaded = parse_arguments_from(
        ["--load", "campaign", "--script", "journey.script"].map(str::to_owned),
    )
    .unwrap();
    assert_eq!(loaded.seed, None);
    assert_eq!(loaded.load.as_deref(), Some("campaign"));
    assert_eq!(loaded.script, Some(PathBuf::from("journey.script")));
    assert!(
        parse_arguments_from(["--seed", "17", "--load", "campaign"].map(str::to_owned)).is_err()
    );
}

#[test]
fn blocked_walking_stops_at_the_physical_collision_instead_of_consuming_twelve_seconds() {
    let mut session = Session::new(24318).unwrap();
    for _ in 0..7 {
        let (step, _) = session.command("n 240").unwrap();
        assert!(step.contains("Advanced "));
        assert!(!step.contains("Movement stopped"));
    }

    let before = session.command("status").unwrap().0;
    assert!(before.contains("At (8,2,1) | control 32"), "{before}");

    let (blocked, _) = session.command("n 240").unwrap();
    assert!(blocked.contains("Movement stopped"), "{blocked}");
    assert!(blocked.contains("At (8,2,1)"), "{blocked}");
    let ticks: u64 = blocked
        .lines()
        .find_map(|line| line.strip_prefix("Advanced "))
        .and_then(|line| line.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(ticks < 10, "a confirmed collision spent {ticks} ticks");
}

struct ScriptFile(PathBuf);

impl ScriptFile {
    fn new(contents: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "deep_hearth_script_{}_{}.txt",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        Self(path)
    }
}

impl Drop for ScriptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn scripted_command_error_stops_at_its_line_without_executing_following_work() {
    let script = ScriptFile::new("# comment\nwait 1\nno-such-command\nwait 1\n");
    let mut session = Session::new(8).unwrap();
    let error = play_script(&mut session, &script.0).unwrap_err();
    assert!(error.contains(&format!("{}:3:", script.0.display())));
    assert!(error.contains("unknown command or arguments: no-such-command"));
    let (status, _) = session.command("status").unwrap();
    assert!(status.contains("control 1 | simulation 0"));
}

#[test]
fn scripted_quit_is_a_successful_stop_not_a_command_error() {
    let script = ScriptFile::new("wait 1\nquit\nno-such-command\n");
    let mut session = Session::new(8).unwrap();
    assert_eq!(play_script(&mut session, &script.0), Ok(()));
    assert!(
        session
            .command("status")
            .unwrap()
            .0
            .contains("control 1 | simulation 0")
    );
}

#[test]
fn daylight_watch_rejects_bad_phases_and_never_waits_a_day_when_already_there() {
    let mut session = Session::new(24318).unwrap();
    for invalid in ["wait until dusk", "wait until", "wait until morning extra"] {
        assert!(session.command(invalid).is_err(), "{invalid}");
    }
    let (already, _) = session.command("wait until morning").unwrap();
    assert!(already.contains("It is already morning."), "{already}");
    assert!(already.contains("control 0 | simulation 0"), "{already}");
}

#[test]
fn daylight_watch_checks_real_survival_and_stops_before_the_next_phase() {
    let mut session = Session::new(24318).unwrap();
    let (watch, _) = session.command("wait until afternoon").unwrap();
    assert!(
        watch.contains("Daylight watch interrupted"),
        "the uncovered, unfueled player should not be left unattended for six hours: {watch}"
    );
    assert!(!watch.contains("Reached afternoon."), "{watch}");
    let (after, _) = session.command("status").unwrap();
    assert!(after.contains("Weather: Spring Morning"), "{after}");
    if after.contains("Warnings: ") {
        assert!(session.command("wait until afternoon").is_err());
        assert_eq!(session.command("status").unwrap().0, after);
    }
}

#[test]
fn control_command_dispatch_preserves_clock_and_rejects_bad_arguments_atomically() {
    let mut session = Session::new(24318).unwrap();
    let status = |session: &mut Session| session.command("status").unwrap().0;
    for invalid in [
        "n 0",
        "sprint up 1",
        "wait work",
        "wait 1 2",
        "slot 10",
        "prepare n extra",
        "gather 1 repeat 0",
        "gather 1 repeat 33",
        "gather 1 1000 repeat wrong",
    ] {
        assert!(
            session.command(invalid).is_err(),
            "expected rejection for {invalid}"
        );
        assert!(
            status(&mut session).contains("control 0 | simulation 0"),
            "{invalid} advanced time"
        );
    }
    let (already_aligned, running) = session.command("wait boundary").unwrap();
    assert!(running);
    assert!(already_aligned.contains("Already on a simulation boundary"));
    assert!(status(&mut session).contains("control 0 | simulation 0"));

    let (one_tick, _) = session.command("wait 1").unwrap();
    assert!(one_tick.contains("Advanced 1 control ticks"));
    assert!(status(&mut session).contains("control 1 | simulation 0"));
    let (to_boundary, _) = session.command("wait boundary").unwrap();
    assert!(to_boundary.contains("Advanced 71 control ticks"));
    assert!(status(&mut session).contains("control 72 | simulation 1"));
}

#[test]
fn shallow_aim_directions_are_valid_for_pointing_but_not_walking() {
    let mut session = Session::new(24318).unwrap();
    for direction in ["n-shallow", "s-shallow", "e-shallow", "w-shallow"] {
        let (look, _) = session.command(&format!("look {direction}")).unwrap();
        assert!(look.contains("At ("), "{look}");
        assert!(session.command(&format!("sprint {direction} 20")).is_err());
        assert!(session.command(&format!("jump {direction} 20")).is_err());
    }
    assert!(
        session
            .command("status")
            .unwrap()
            .0
            .contains("control 0 | simulation 0")
    );
}

/// Drives the maintained opening script through the same `play_script` path `--script` uses.
///
/// The file is checked in so a reviewer can run the identical commands by hand. Rejection of any
/// line is a real regression in either the command surface or the opening world, not a script bug.
#[test]
fn maintained_opening_script_executes_end_to_end_on_its_seed() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/text_play/opening_smoke.script");
    let mut session = Session::new(24318).unwrap();
    play_script(&mut session, &path)
        .unwrap_or_else(|error| panic!("maintained opening script failed: {error}"));
    let (status, _) = session.command("status").unwrap();
    assert!(
        status.contains("control "),
        "opening script left the session without a control clock: {status}"
    );
}
