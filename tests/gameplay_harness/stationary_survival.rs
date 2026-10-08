//! Explicit inherited survival pressure for stationary gameplay episodes.
//!
//! Most focused probes start at full reserves and depend only on `world_admission`. Keeping the
//! pressure variants here prevents those unrelated targets from compiling survival-fixture policy.

use deep_hearth::content::gameplay_fixture::{
    seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary,
};
use deep_hearth::core::state::AppState;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::exact_local_runtime::{
    assert_exact_local_runtime_ready, initialize_stationary_player_logistics,
    locate_stationary_endpoints,
};
use super::stationary_survival_start::StationarySurvivalStart;

/// Admits one stationary episode with an explicit, disclosed inherited survival state.
///
/// Pressure variants are fixture-only starting conditions. Once admitted, all subsequent reserve
/// changes still come from canonical simulation work, direct consumption, and ticks.
pub(super) fn admit_stationary_player_with_survival_start(
    registries: &Registries,
    state: &mut AppState,
    stockpiles: &[StockpileId],
    fluid_stores: &[FluidStoreId],
    survival_start: StationarySurvivalStart,
    context: &'static str,
) {
    locate_stationary_endpoints(state, stockpiles, fluid_stores);
    match survival_start {
        StationarySurvivalStart::FullReserve => {
            initialize_player_survival(registries, state).unwrap_or_else(|error| {
                panic!("stationary gameplay {context} survival initialization failed: {error}")
            });
        }
        StationarySurvivalStart::HungerWarningBoundary => {
            seed_player_survival_at_hunger_warning_boundary(registries, state);
        }
        StationarySurvivalStart::HydrationWarningBoundary => {
            seed_player_survival_at_hydration_warning_boundary(registries, state);
        }
    }
    initialize_stationary_player_logistics(state);
    assert_exact_local_runtime_ready(registries, state, context);
}
