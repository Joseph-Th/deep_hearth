//! Runtime boundary for gameplay evidence that deliberately abstracts player movement.
//!
//! Progression and fieldwork exercise canonical prospecting, mining, production, survival, and
//! persistence semantics across multiple world regions. The production game does not yet own a
//! player movement/path authority, so these episodes must not create a logistics player and then
//! pretend remote actions satisfy ordinary locality.

use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::registry::Registries;

pub(super) fn assert_spatial_proxy_runtime_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    assert!(
        state.survival().player().is_some(),
        "spatial-proxy gameplay {context} has no admitted survival player"
    );
    assert!(
        state.logistics().player().is_none(),
        "spatial-proxy gameplay {context} must not claim ordinary locality without production movement/path authority"
    );
    validate_loaded_state(registries, state).unwrap_or_else(|error| {
        panic!("spatial-proxy gameplay {context} admitted invalid runtime state: {error}")
    });
}
