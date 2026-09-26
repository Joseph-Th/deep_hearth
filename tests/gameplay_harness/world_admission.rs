//! Shared high-fidelity admission for stationary ordinary-play episodes.
//!
//! Gameplay fixtures may seed a disclosed pre-existing world before the actor exists. Once an
//! ordinary-play episode begins, however, runtime access must use the same logistics-locality
//! rules as the game. This helper binds pre-existing stationary endpoints to one workshop voxel
//! and creates the real player logistics record; it does not move the player or bypass transport.

use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::logistics::{
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use deep_hearth::spatial::VoxelCoord;

pub(super) const STATIONARY_PLAYER_ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

/// Locates pre-existing stationary endpoints at the ordinary player's workshop voxel.
///
/// Call this only after all fixture seeding for those endpoints and before runtime actions begin.
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
///
/// The carried stockpile is intentionally small because these episodes exercise stationary local
/// custody rather than haulage. No tested action depends on this bootstrap capacity.
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
