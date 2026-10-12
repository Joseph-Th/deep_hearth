//! Shared high-fidelity admission for stationary ordinary-play episodes.
//!
//! Fixtures may seed disclosed pre-existing state before the actor exists. Admission binds stationary
//! endpoints to one workshop voxel and creates the real player logistics record; runtime access then
//! uses ordinary locality rules without moving the player or bypassing transport.

use deep_hearth::core::state::AppState;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::player::validate_admit_player;
use deep_hearth::registry::Registries;

use super::exact_local_runtime::{
    STATIONARY_PLAYER_ORIGIN, assert_exact_local_runtime_ready, locate_stationary_endpoints,
};

/// Admits one ordinary stationary episode through both player owners after all fixture seeding.
pub(super) fn admit_stationary_player(
    registries: &Registries,
    state: &mut AppState,
    stockpiles: &[StockpileId],
    fluid_stores: &[FluidStoreId],
    context: &'static str,
) {
    locate_stationary_endpoints(state, stockpiles, fluid_stores);
    validate_admit_player(registries, state, STATIONARY_PLAYER_ORIGIN)
        .unwrap_or_else(|error| panic!("stationary gameplay {context} admission failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("stationary gameplay {context} admission commit failed: {error}")
        });
    assert_exact_local_runtime_ready(registries, state, context);
}
