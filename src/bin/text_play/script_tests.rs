//! Command-file execution contracts for the text-play adapter.

use super::*;

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
