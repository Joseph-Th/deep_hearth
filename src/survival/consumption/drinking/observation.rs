//! Player-facing local fluid observation, separate from drinking admission and custody changes.

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::{FluidDefinitionId, FluidStoreId};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

/// Actor-visible drinkable source at one face-adjacent opening-graph voxel.
///
/// Only source identity, fluid identity and destination are visible. Exact volume and temperature
/// remain exact-local observations. Generated terrain uses the separate world-owned
/// `available_visible_generated_water` query, which reveals hot water without treating it as safe.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdjacentDrinkSource {
    store: FluidStoreId,
    fluid: FluidDefinitionId,
    position: VoxelCoord,
}

impl AdjacentDrinkSource {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }

    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }
}

/// Actor-visible finite drink source currently usable at the player's exact voxel.
///
/// Drinking still goes through its current admission validator, which rechecks local access,
/// volume, temperature, attention, and physiological constraints.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalDrinkSource {
    store: FluidStoreId,
    fluid: FluidDefinitionId,
    available_volume: Volume,
    temperature: Temperature,
}

impl LocalDrinkSource {
    #[must_use]
    pub const fn store(self) -> FluidStoreId {
        self.store
    }

    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn available_volume(self) -> Volume {
        self.available_volume
    }

    #[must_use]
    pub const fn temperature(self) -> Temperature {
        self.temperature
    }
}

fn drink_source_at(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    position: VoxelCoord,
) -> Option<AdjacentDrinkSource> {
    let contents = state.fluid().get_store(store)?.contents()?;
    let drink = registries.survival().get_drink(contents.fluid())?;
    drink
        .consumption_temperature()
        .contains(contents.temperature())
        .then_some(AdjacentDrinkSource {
            store,
            fluid: contents.fluid(),
            position,
        })
}

/// Opening-graph observation of exact face-neighbor drinkable sources.
///
/// Empty, inaccessible, non-drinkable, hot, local and distant sources are omitted. Dead players
/// have no observations; controlled logistics-only fixtures retain their existing view.
pub fn available_adjacent_drink_sources<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = AdjacentDrinkSource> + 'a {
    let mut nearby = state
        .player_observation_position()
        .into_iter()
        .flat_map(|position| state.logistics().traversable_destinations(position))
        .flat_map(|position| {
            state
                .logistics()
                .fluid_stores_at(position)
                .filter_map(move |store| drink_source_at(registries, state, store, position))
        })
        .collect::<Vec<_>>();
    nearby.sort_unstable_by_key(|source| source.store);
    nearby.into_iter()
}

/// Exact-local drinking observation, including remainders below one serving.
///
/// Empty, unlocated, non-drinkable, temperature-incompatible and remote stores are omitted.
pub fn available_local_drink_sources<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = LocalDrinkSource> + 'a {
    state
        .available_local_fluid_stores()
        .filter_map(move |store| {
            let contents = store.contents()?;
            let drink = registries.survival().get_drink(contents.fluid())?;
            drink
                .consumption_temperature()
                .contains(contents.temperature())
                .then_some(LocalDrinkSource {
                    store: store.id(),
                    fluid: contents.fluid(),
                    available_volume: contents.volume(),
                    temperature: contents.temperature(),
                })
        })
}
