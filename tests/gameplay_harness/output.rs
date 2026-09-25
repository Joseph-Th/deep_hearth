//! Shared gameplay-harness output policy: quiet gates, concise summaries, opt-in trace detail.

use std::env;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static REVIEW_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
#[allow(
    dead_code,
    reason = "broad gameplay audit shares output module but does not run focused report tests"
)]
pub(super) fn set_review_output(enabled: bool) {
    REVIEW_OUTPUT_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn review_output_enabled() -> bool {
    REVIEW_OUTPUT_ENABLED.load(Ordering::Relaxed)
}

#[allow(
    dead_code,
    reason = "focused reports do not all use verbose output controls"
)]
pub(super) fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
        || env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

#[allow(
    dead_code,
    reason = "focused reports do not all emit trace-only narration"
)]
pub(super) fn has_trace_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

// Routine test binaries keep narration disabled at runtime. The ignored focused-report tests flip
// the shared review switch so exploration can reuse the already-built focused test artifact.
#[cfg(test)]
#[allow(unused_macros)]
macro_rules! println {
    ($($argument:tt)*) => {{
        if crate::output::review_output_enabled() && crate::output::has_trace_output() {
            std::println!($($argument)*);
        }
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! println {
    ($($argument:tt)*) => {{
        if crate::output::has_trace_output() {
            std::println!($($argument)*);
        }
    }};
}

/// Prints human-readable probe review output in the explicit gameplay report.
#[cfg(test)]
#[allow(unused_macros)]
macro_rules! reviewln {
    ($($argument:tt)*) => {{
        if crate::output::review_output_enabled() {
            std::println!($($argument)*);
        }
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! reviewln {
    ($($argument:tt)*) => {{
        std::println!($($argument)*);
    }};
}
