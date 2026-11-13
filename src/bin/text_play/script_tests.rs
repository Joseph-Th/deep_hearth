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
