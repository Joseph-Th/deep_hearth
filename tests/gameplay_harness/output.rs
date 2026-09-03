//! Shared gameplay-harness output policy: quiet gates, concise summaries, opt-in trace detail.

use std::env;

pub(super) fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
        || env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

#[allow(dead_code)]
pub(super) fn has_trace_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

// Focused integration crates intentionally use different subsets of these shared channels.
#[allow(unused_macros)]
macro_rules! println {
    ($($argument:tt)*) => {{
        if crate::output::has_trace_output() {
            std::println!($($argument)*);
        }
    }};
}

/// Prints human-readable probe review output for explicit reports or opt-in verbose test runs.
/// Routine gates keep only replay/input lines and compact pass/fail summaries.
#[allow(unused_macros)]
macro_rules! reviewln {
    ($($argument:tt)*) => {{
        if !cfg!(test) || crate::output::has_verbose_output() {
            std::println!($($argument)*);
        }
    }};
}
