//! Contract tests for root runtime state and trusted continuation.

use super::*;
use crate::content::build_registries;
#[test]
fn new_state_starts_at_zero_and_validates() {
    let registries = build_registries();
    let state = AppState::new();

    assert_eq!(state.tick(), SimulationTick::ZERO);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn app_state_debug_does_not_expose_hidden_geology() {
    let state = AppState::new();

    let debug = format!("{state:?}");

    assert!(debug.contains("geological_knowledge"));
    assert!(!debug.contains("geology:"));
}

#[cfg(feature = "test-soak")]
#[path = "state_tests/soak.rs"]
mod soak;
