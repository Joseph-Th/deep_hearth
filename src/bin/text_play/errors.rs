//! Player-facing work admission errors without parsing formatted diagnostics.

use std::error::Error;

use deep_hearth::core::time::SimulationBoundaryError;

/// Canonical owners expose nested typed causes. Only an actual scheduling
/// boundary failure warrants the text client's boundary command suggestion;
/// unrelated errors retain their detailed owner diagnostic unchanged.
pub(super) fn work_start_error(error: impl Error + 'static) -> String {
    let mut cause: Option<&(dyn Error + 'static)> = Some(&error);
    while let Some(current) = cause {
        if current.is::<SimulationBoundaryError>() {
            return "This task must start on a simulation boundary. Run 'wait boundary', then retry."
                .to_owned();
        }
        cause = current.source();
    }
    error.to_string()
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
