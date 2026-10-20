//! Engineered progress is noticed locally by a physical scout, never read as a player level.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::EnergyCarrier;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::visibility::scout_visible_from;
use super::{
    FIRST_SCOUT_TICK, KoboldColony, MOBILIZED_BULK_SPLIT_BATCH, MOBILIZED_FORAGE_BATCH,
    MOBILIZED_FORAGE_INTERVAL_TICKS, SCOUT_BULK_SPLIT_BATCH, SCOUT_FORAGE_BATCH,
    SCOUT_FORAGE_INTERVAL_TICKS, SCOUT_STEP_TICKS,
};

/// Record the first direct sighting on the already sparse scout activity clock.
pub(super) fn observe_progress(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
    from: VoxelCoord,
) {
    if state
        .kobold_state()
        .colony
        .as_ref()
        .is_some_and(|colony| colony.mobilized_at.is_none())
        && scout_sees_engineered_machinery(registries, state, from)
    {
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .expect("observed scout remains in the same colony")
            .mobilized_at = Some(at);
    }
}

impl KoboldColony {
    pub(super) fn salvage_batch(&self) -> Mass {
        self.mobilized_at
            .map(|_| MOBILIZED_BULK_SPLIT_BATCH)
            .unwrap_or(SCOUT_BULK_SPLIT_BATCH)
    }

    pub(super) fn forage_batch(&self) -> Mass {
        self.mobilized_at
            .map(|_| MOBILIZED_FORAGE_BATCH)
            .unwrap_or(SCOUT_FORAGE_BATCH)
    }

    pub(super) fn forage_interval(&self) -> u64 {
        self.mobilized_at
            .map(|_| MOBILIZED_FORAGE_INTERVAL_TICKS)
            .unwrap_or(SCOUT_FORAGE_INTERVAL_TICKS)
    }

    pub(super) fn validate_mobilization(
        &self,
        current: SimulationTick,
    ) -> Result<(), &'static str> {
        if self.mobilized_at.is_some_and(|at| {
            at.value() < FIRST_SCOUT_TICK
                || at > current
                || (at.value() - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS != 0
        }) {
            return Err("kobold mobilization lacks a possible local scout observation time");
        }
        Ok(())
    }
}

/// Nearby, unobstructed nonthermal machinery marks progress beyond campfire
/// survival. Even an empty store is installed, tangible engineered equipment.
pub(super) fn scout_sees_engineered_machinery(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
) -> bool {
    state
        .logistics()
        .energy_store_locations_near(scout, 2)
        .into_iter()
        .any(|(store, position)| {
            let Some(record) = state.energy().get_store(store) else {
                return false;
            };
            let Some(definition) = registries.energy().get_store(record.definition()) else {
                return false;
            };
            definition.carrier() != EnergyCarrier::Thermal
                && scout_visible_from(registries, state, scout, position)
        })
}

#[cfg(test)]
#[path = "mobilization_tests.rs"]
mod tests;
