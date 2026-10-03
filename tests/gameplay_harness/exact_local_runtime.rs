//! Runtime-state invariant for ordinary exact-local gameplay evidence.

use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelCoord;

pub(super) const STATIONARY_PLAYER_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

/// Verifies that one exact-local ordinary world is fully admitted to runtime state the player can
/// actually act on.
///
/// Controlled setup may create endpoints before the actor exists, but none may remain locationless
/// once ordinary player logistics is initialized.
pub(super) fn assert_exact_local_runtime_ready(
    registries: &Registries,
    state: &AppState,
    context: &'static str,
) {
    let player = state.logistics().player().unwrap_or_else(|| {
        panic!("stationary gameplay {context} has no admitted logistics player")
    });
    assert_eq!(
        player.position(),
        STATIONARY_PLAYER_ORIGIN,
        "stationary gameplay {context} player moved away from its exact-local workshop"
    );
    for stockpile in state.inventory().stockpiles() {
        assert_eq!(
            state.logistics().stockpile_position(stockpile.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} stockpile {} is not local to the admitted player",
            stockpile.id().value()
        );
    }
    for equipment in state.equipment().equipment() {
        assert_eq!(
            state.logistics().equipment_position(equipment.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} equipment {} is not local to the admitted player",
            equipment.id().value()
        );
    }
    for store in state.energy().stores() {
        assert_eq!(
            state.logistics().energy_store_position(store.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} energy store {} is not local to the admitted player",
            store.id().value()
        );
    }
    for store in state.fluid().stores() {
        assert_eq!(
            state.logistics().fluid_store_position(store.id()),
            Some(STATIONARY_PLAYER_ORIGIN),
            "stationary gameplay {context} fluid store {} is not local to the admitted player",
            store.id().value()
        );
    }
    validate_loaded_state(registries, state).unwrap_or_else(|error| {
        panic!("stationary gameplay {context} admitted invalid runtime state: {error}")
    });
}
