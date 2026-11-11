//! Discover nearby located fluid without exposing the hidden regional water ledger.

use crate::content::FLUID_WATER;
use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

/// An actual visible source, not its quantity, temperature, or an inferred
/// remote aquifer. Drinking still requires ordinary exact-local access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleWaterSource {
    store: FluidStoreId,
    position: VoxelCoord,
}

impl VisibleWaterSource {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }
}

#[must_use]
pub fn available_visible_generated_water(
    registries: &Registries,
    state: &AppState,
) -> Vec<VisibleWaterSource> {
    let Some(player) = state.logistics().player() else {
        return Vec::new();
    };
    if state
        .survival()
        .player()
        .is_none_or(|survival| survival.vitality() == Vitality::ZERO)
    {
        return Vec::new();
    }
    let from = player.position();
    if !state.world().has_generated_terrain_at(from) {
        return Vec::new();
    }
    let mut nearby = Vec::new();
    for to in super::nearby_two_step_cells(from) {
        // Most cells in the local sight volume hold no fluid. Check the
        // exact-position index before expensive sight and sky queries;
        // this is only a private candidate gate, not remote disclosure.
        let mut stores = state.logistics().fluid_stores_at(to).peekable();
        if stores.peek().is_none() || !super::player_can_see_from_eye(registries, state, from, to) {
            continue;
        }
        for store in stores {
            if state
                .fluid()
                .get_store(store)
                .and_then(|record| record.contents())
                .is_some_and(|contents| contents.fluid() == FLUID_WATER)
            {
                nearby.push(VisibleWaterSource {
                    store,
                    position: to,
                });
            }
        }
    }
    nearby.sort_unstable_by_key(|source| source.store);
    nearby
}

#[cfg(test)]
#[path = "spring_sight_tests.rs"]
mod tests;
