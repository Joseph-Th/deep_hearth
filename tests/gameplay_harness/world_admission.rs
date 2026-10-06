//! Shared high-fidelity admission for stationary ordinary-play episodes.
//!
//! Fixtures may seed disclosed pre-existing state before the actor exists. Admission binds stationary
//! endpoints to one workshop voxel and creates the real player logistics record; runtime access then
//! uses ordinary locality rules without moving the player or bypassing transport.

use deep_hearth::content::gameplay_fixture::{
    seed_player_survival_at_hunger_warning_boundary,
    seed_player_survival_at_hydration_warning_boundary,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::fluid::FluidStoreId;
use deep_hearth::inventory::StockpileId;
use deep_hearth::logistics::{
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

pub(super) use super::exact_local_runtime::{
    STATIONARY_PLAYER_ORIGIN, assert_exact_local_runtime_ready,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StationarySurvivalStart {
    FullReserve,
    HungerWarningBoundary,
    HydrationWarningBoundary,
}

impl StationarySurvivalStart {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::FullReserve => "full-reserve",
            Self::HungerWarningBoundary => "hunger-warning-boundary",
            Self::HydrationWarningBoundary => "hydration-warning-boundary",
        }
    }
}

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

/// Admits one ordinary stationary episode through both player owners after all fixture seeding.
pub(super) fn admit_stationary_player(
    registries: &Registries,
    state: &mut AppState,
    stockpiles: &[StockpileId],
    fluid_stores: &[FluidStoreId],
    context: &'static str,
) {
    admit_stationary_player_with_survival_start(
        registries,
        state,
        stockpiles,
        fluid_stores,
        StationarySurvivalStart::FullReserve,
        context,
    );
}

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
