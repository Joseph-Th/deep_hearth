//! Shared gameplay-harness output policy: quiet gates, concise summaries, opt-in trace detail.

#[cfg(not(test))]
use std::env;

#[cfg(not(test))]
#[allow(
    dead_code,
    reason = "report examples do not all consume verbose controls"
)]
pub(super) fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
        || env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

#[cfg(not(test))]
#[allow(
    dead_code,
    reason = "report examples do not all emit trace-only narration"
)]
pub(super) fn has_trace_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

// Test binaries stay quiet during routine gates. Explicit scoped reports reuse the same artifact;
// review output is enabled below, while narration still requires trace mode.
#[cfg(test)]
#[allow(unused_macros)]
macro_rules! println {
    ($($argument:tt)*) => {{
        if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some()
            && std::env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
        {
            std::println!($($argument)*);
        } else {
            let _ = std::format_args!($($argument)*);
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
        if std::env::var_os("DEEP_HEARTH_GAMEPLAY_REPORT").is_some() {
            std::println!($($argument)*);
        } else {
            let _ = std::format_args!($($argument)*);
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
