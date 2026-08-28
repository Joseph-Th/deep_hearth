//! Fresh replay-root generation for bounded organic gameplay sampling.

use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use super::seed::mix64;

/// Produces a fresh root for one bounded organic sample.
///
/// The root is always printed by callers before execution so a surprising run is exactly replayable.
/// Maintained anchors remain separate and stable; this root only selects supplemental variation.
pub(super) fn fresh_root(salt: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let folded = (now as u64) ^ ((now >> 64) as u64) ^ u64::from(process::id()) ^ salt;
    mix64(folded)
}
