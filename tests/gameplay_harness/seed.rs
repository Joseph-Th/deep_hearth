//! Seed mixing and bounded fresh-root generation shared by gameplay harnesses.

use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

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
