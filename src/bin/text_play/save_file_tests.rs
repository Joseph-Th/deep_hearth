//! Filesystem publication regressions for text-play saves.

use super::super::Session;
use super::*;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};

fn snapshot(session: &Session) -> Vec<u8> {
    serde_json::to_vec(&SaveEnvelope::new(&session.registries, &session.state)).unwrap()
}

struct IsolatedSaveDirectory(PathBuf);

impl IsolatedSaveDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "deep_hearth_save_{}_{}",
            std::process::id(),
            serial
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn slot(&self) -> PathBuf {
        self.0.join("campaign.sav")
    }
}

impl Drop for IsolatedSaveDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn abandoned_staging_files_do_not_block_a_new_save_or_get_deleted() {
    let directory = IsolatedSaveDirectory::new();
    let abandoned = directory
        .0
        .join(format!(".campaign.sav.{}.0.pending", std::process::id()));
    let older_abandoned = directory.0.join(".campaign.pending");
    std::fs::write(&abandoned, b"abandoned partial save").unwrap();
    std::fs::write(&older_abandoned, b"older partial save").unwrap();
    let session = Session::new(8).unwrap();
    let expected = snapshot(&session);

    publish_new_save(&directory.slot(), &expected).unwrap();
    assert_eq!(std::fs::read(directory.slot()).unwrap(), expected);
    assert_eq!(
        std::fs::read(&abandoned).unwrap(),
        b"abandoned partial save",
        "an old staged file may belong to another writer"
    );
    assert_eq!(
        std::fs::read(&older_abandoned).unwrap(),
        b"older partial save"
    );
    let encoded = std::fs::read(directory.slot()).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let reloaded = decoded.into_state(&session.registries).unwrap();
    assert_eq!(
        serde_json::to_vec(&SaveEnvelope::new(&session.registries, &reloaded)).unwrap(),
        expected
    );
}

#[test]
fn save_publication_never_replaces_an_existing_slot() {
    let directory = IsolatedSaveDirectory::new();
    let destination = directory.slot();
    std::fs::write(&destination, b"original campaign").unwrap();
    assert!(
        publish_new_save(&destination, b"competing campaign")
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(std::fs::read(destination).unwrap(), b"original campaign");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn concurrent_save_publications_have_one_winner_and_no_staging_leaks() {
    let directory = IsolatedSaveDirectory::new();
    let destination = directory.slot();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let writers = [
        b"first distinct save".as_slice(),
        b"second distinct save".as_slice(),
    ]
    .into_iter()
    .map(|contents| {
        let destination = destination.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            publish_new_save(&destination, contents)
        })
    })
    .collect::<Vec<_>>();
    barrier.wait();
    let results = writers
        .into_iter()
        .map(|writer| writer.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.contains("already exists"))
    );
    let bytes = std::fs::read(destination).unwrap();
    assert!(bytes == b"first distinct save" || bytes == b"second distinct save");
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}
