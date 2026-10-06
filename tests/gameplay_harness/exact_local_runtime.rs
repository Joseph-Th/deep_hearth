//! Runtime-state invariant for ordinary exact-local gameplay evidence.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::logistics::{
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use deep_hearth::registry::Registries;
use deep_hearth::spatial::VoxelCoord;

pub(super) const STATIONARY_PLAYER_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

/// Locates pre-existing stationary endpoints at the ordinary player's workshop voxel.
///
/// Call this only after all fixture seeding for those endpoints and before runtime actions begin.
#[allow(
    dead_code,
    reason = "some focused targets only validate exact-local state and do not perform stationary fixture admission"
)]
pub(super) fn locate_stationary_endpoints(
    state: &mut AppState,
    stockpiles: &[StockpileId],
    fluid_stores: &[FluidStoreId],
) {
    for &stockpile in stockpiles {
        validate_place_ground_stockpile(state, stockpile, STATIONARY_PLAYER_ORIGIN)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay stockpile {} placement failed: {error}",
                    stockpile.value()
                )
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay stockpile {} placement commit failed: {error}",
                    stockpile.value()
                )
            });
    }
    for &store in fluid_stores {
        validate_place_fluid_store(state, store, STATIONARY_PLAYER_ORIGIN)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay fluid store {} placement failed: {error}",
                    store.value()
                )
            })
            .commit(state)
            .unwrap_or_else(|error| {
                panic!(
                    "stationary gameplay fluid store {} placement commit failed: {error}",
                    store.value()
                )
            });
    }
}

/// Initializes the real player logistics owner at the stationary workshop voxel.
#[allow(
    dead_code,
    reason = "some focused targets only validate exact-local state and do not perform stationary fixture admission"
)]
pub(super) fn initialize_stationary_player_logistics(state: &mut AppState) {
    validate_initialize_player_logistics(state, STATIONARY_PLAYER_ORIGIN, Mass::from_milligrams(1))
        .unwrap_or_else(|error| {
            panic!("stationary gameplay logistics initialization failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("stationary gameplay logistics initialization commit failed: {error}")
        });
}

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
