//! Discover nearby located fluid without exposing the hidden regional water ledger.

use crate::content::FLUID_WATER;
use crate::core::quantity::Volume;
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

/// Surface water in the natural basin the living player is standing in.
/// A spent basin remains recognizable at one's feet; no distant empty pool
/// or hidden subsurface reserve is disclosed.
#[must_use]
pub fn assess_player_local_natural_spring(state: &AppState) -> Option<Volume> {
    let player = state.logistics().player()?;
    if state
        .survival()
        .player()
        .is_none_or(|survival| survival.vitality() == Vitality::ZERO)
        || !state.world().has_generated_terrain_at(player.position())
    {
        return None;
    }
    state
        .logistics()
        .fluid_stores_at(player.position())
        .find(|store| state.fluid().is_natural_spring(*store))
        .and_then(|store| {
            state
                .fluid()
                .get_store(store)
                .map(|record| record.stored_volume())
        })
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

/// Nonlocal water landmarks within the same bounded landscape sight radius as trees.
/// No remote amounts, interaction rights, or undiscovered aquifers are disclosed.
#[must_use]
pub fn available_visible_water_landmarks(
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
    let mut visible = state
        .logistics()
        .fluid_store_locations_near(from, super::LANDMARK_SIGHT_HORIZONTAL_RADIUS)
        .filter(|(_, to)| {
            let dy = to.y().abs_diff(from.y());
            dy <= 6 && to.x().abs_diff(from.x()) + dy + to.z().abs_diff(from.z()) > 2
        })
        .filter(|(store, to)| {
            state
                .fluid()
                .get_store(*store)
                .and_then(|record| record.contents())
                .is_some_and(|contents| contents.fluid() == FLUID_WATER)
                && super::targeting::player_can_see_distant_cell(registries, state, *to)
        })
        .map(|(store, position)| VisibleWaterSource { store, position })
        .collect::<Vec<_>>();
    visible.sort_unstable_by_key(|source| source.store);
    visible
}

#[cfg(test)]
#[path = "spring_sight_tests.rs"]
mod tests;
