//! Text-play save filenames and atomic no-replacement filesystem publication.

use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

/// Publish a validated save without exposing a partial file or overwriting another
/// writer's slot. Creating a hard link in the same directory is an atomic
/// create-if-absent operation; a prior existence check followed by rename is not.
///
/// Unique create-new staging names tolerate abandoned files from crashed writers
/// without deleting another process's still-active staging work.
pub(super) fn publish_new_save(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let directory = destination
        .parent()
        .ok_or("save destination has no parent")?;
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("save destination has no valid file name")?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create save directory: {error}"))?;
    if destination.exists() {
        return Err(format!(
            "save slot already exists: {}",
            destination.display()
        ));
    }
    let mut staging = None;
    for attempt in 0..128_u32 {
        let temporary = directory.join(format!(
            ".{filename}.{}.{}.pending",
            std::process::id(),
            attempt
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => {
                staging = Some((temporary, file));
                break;
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create staging save: {error}")),
        }
    }
    let (temporary, mut file) =
        staging.ok_or("cannot create staging save: all unique names are occupied")?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("save write failed: {error}"));
    }
    let published = std::fs::hard_link(&temporary, destination);
    // Once the link exists the save is committed. Cleanup must not turn
    // success into a retryable error even if the staging unlink fails.
    let _ = std::fs::remove_file(&temporary);
    match published {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Err(format!(
            "save slot already exists: {}",
            destination.display()
        )),
        Err(error) => Err(format!("save publication failed: {error}")),
    }
}

pub(super) fn slot_path(slot: &str) -> Result<PathBuf, String> {
    if slot.is_empty()
        || slot.len() > 48
        || !slot
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(
            "save slots must be 1-48 ASCII letters, digits, hyphens, or underscores".to_owned(),
        );
    }
    Ok(PathBuf::from("saves").join(format!("{slot}.sav")))
}

#[cfg(test)]
#[path = "save_file_tests.rs"]
mod tests;
