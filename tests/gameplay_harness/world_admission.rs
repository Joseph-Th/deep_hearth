//! Shared high-fidelity admission for stationary ordinary-play episodes.
//!
//! Fixtures may seed disclosed pre-existing state before the actor exists. Admission binds stationary
//! endpoints to one workshop voxel and creates the real player logistics record; runtime access then
//! uses ordinary locality rules without moving the player or bypassing transport.

use deep_hearth::core::state::AppState;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::exact_local_runtime::{
    assert_exact_local_runtime_ready, initialize_stationary_player_logistics,
    locate_stationary_endpoints,
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
    initialize_player_survival(registries, state).unwrap_or_else(|error| {
        panic!("stationary gameplay {context} survival initialization failed: {error}")
    });
    initialize_stationary_player_logistics(state);
    assert_exact_local_runtime_ready(registries, state, context);
}
