//! Shared gameplay-harness output policy: quiet gates, concise summaries, opt-in trace detail.

#[cfg(not(test))]
use std::env;

#[cfg(not(test))]
pub(super) fn has_verbose_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some()
        || env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

#[cfg(not(test))]
pub(super) fn has_trace_output() -> bool {
    env::var_os("DEEP_HEARTH_GAMEPLAY_TRACE").is_some()
}

// Routine test binaries type-check narration but do not code-generate it. Full review/trace output
// belongs to the explicit non-test gameplay report so focused repair targets stay small and fast.
#[cfg(test)]
#[allow(unused_macros)]
macro_rules! println {
    ($($argument:tt)*) => {{
        if false {
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
        if false {
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
