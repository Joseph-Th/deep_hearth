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

/// Record the scout's direct knowledge. The colony cannot act on it until the scout returns.
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
        .is_some_and(|colony| colony.witnessed_machinery_at.is_none())
        && scout_sees_engineered_machinery(registries, state, from)
    {
        state
            .kobold_state_mut()
            .colony
            .as_mut()
            .expect("observed scout remains in the same colony")
            .witnessed_machinery_at = Some(at);
    }
}

impl KoboldColony {
    /// The report becomes colony knowledge only after an actual completed return.
    /// Remaining outside (including a player-blocked den entry) postpones mobilization.
    pub(super) fn report_progress_at_home(&mut self, at: SimulationTick) {
        if self.scout_step == 0
            && self.mobilized_at.is_none()
            && self.witnessed_machinery_at.is_some()
        {
            self.mobilized_at = Some(at);
        }
    }

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
        let aligned = |at: SimulationTick| {
            at.value() >= FIRST_SCOUT_TICK
                && at <= current
                && (at.value() - FIRST_SCOUT_TICK) % SCOUT_STEP_TICKS == 0
        };
        if self.witnessed_machinery_at.is_some_and(|at| !aligned(at))
            || self.mobilized_at.is_some_and(|at| !aligned(at))
            || self
                .mobilized_at
                .is_some_and(|at| self.witnessed_machinery_at.is_none_or(|seen| seen >= at))
        {
            return Err("kobold industrial knowledge lacks a possible witness and den report");
        }
        // While the actual arrival remains in bounded track history, validate
        // the reported time against a real, adjacent step ending at the den.
        if self.mobilized_at.is_some_and(|reported| {
            self.tracks
                .first()
                .is_some_and(|first| first.left_at() <= reported)
                && !self
                    .tracks
                    .iter()
                    .any(|trail| trail.left_at() == reported && trail.toward() == self.route[0])
        }) {
            return Err("kobold mobilization report lacks a real return to the den");
        }
        Ok(())
    }
}

/// Only tangible, locally seen installations can change the colony's expectations.
/// A stationary industrial machine is evidence even before it is powered or mounted;
/// portable tools, carried equipment, and distant workshop records are not.
pub(super) fn scout_sees_engineered_machinery(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
) -> bool {
    let engineered_store = state
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
        });
    engineered_store
        || state
            .logistics()
            .equipment_locations_near(scout, 2)
            .into_iter()
            .any(|(equipment, position)| {
                let distance = u128::from(scout.x().abs_diff(position.x()))
                    + u128::from(scout.y().abs_diff(position.y()))
                    + u128::from(scout.z().abs_diff(position.z()));
                distance <= 2
                    && state
                        .equipment()
                        .get_equipment(equipment)
                        .and_then(|record| {
                            registries.equipment().get_equipment(record.definition())
                        })
                        .is_some_and(|definition| definition.requires_structural_support())
                    && scout_visible_from(registries, state, scout, position)
            })
}

#[cfg(test)]
#[path = "mobilization_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "machinery_witness_tests.rs"]
mod machinery_witness_tests;
