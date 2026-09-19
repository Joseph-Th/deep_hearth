//! Timed player dismantling of material-backed storage enclosures.
//!
//! Dismantling is careful uninstallation: completion returns the exact embodied body to a distinct
//! recovery stockpile, so a build/dismantle cycle preserves matter by design and charges only
//! authored labor time plus survival exertion. Destructive reconfiguration is the separate manual
//! salvage route, which conserves mass as boards plus represented chips instead.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::{StorageEnclosureDismantlingWork, ValidatedPlayerWorkStart};

use super::{
    StockpileId, StockpileStorageProfile, StorageDefinitionId, ValidatedInboundReservation,
};

mod admission;
mod completion_validation;
mod errors;
mod tick;

pub use admission::validate_start_storage_enclosure_dismantling;
pub(crate) use completion_validation::validate_storage_dismantling_target_for_completion;
pub use errors::{StorageEnclosureDismantlingCommitError, StorageEnclosureDismantlingError};
pub use tick::StorageEnclosureDismantlingOutcome;
pub(crate) use tick::{
    StorageEnclosureDismantlingCancellationPlan, StorageEnclosureDismantlingTickError,
    StorageEnclosureDismantlingTickPlan, apply_storage_enclosure_dismantling_cancellation,
    apply_storage_enclosure_dismantling_tick, decide_storage_enclosure_dismantling_cancellation,
    decide_storage_enclosure_dismantling_tick,
};

/// Admission result for one dismantling interval. The enclosure remains installed until completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageEnclosureDismantlingStartOutcome {
    target: StockpileId,
    recovery_destination: StockpileId,
    definition: StorageDefinitionId,
    recovered_mass: crate::core::quantity::Mass,
    completes_at: SimulationTick,
}

impl StorageEnclosureDismantlingStartOutcome {
    #[must_use]
    pub const fn target(self) -> StockpileId {
        self.target
    }
    #[must_use]
    pub const fn recovery_destination(self) -> StockpileId {
        self.recovery_destination
    }
    #[must_use]
    pub const fn definition(self) -> StorageDefinitionId {
        self.definition
    }
    #[must_use]
    pub const fn recovered_mass(self) -> crate::core::quantity::Mass {
        self.recovered_mass
    }
    #[must_use]
    pub const fn completes_at(self) -> SimulationTick {
        self.completes_at
    }
}

/// Revision-bound proof that dismantling can reserve recovery capacity and player labor atomically.
#[must_use]
pub struct ValidatedStorageEnclosureDismantlingStart {
    target: StockpileId,
    definition: StorageDefinitionId,
    enclosure_created_at: SimulationTick,
    expected_profile: StockpileStorageProfile,
    reservation: ValidatedInboundReservation,
    work: StorageEnclosureDismantlingWork,
    player_work: ValidatedPlayerWorkStart,
}

impl ValidatedStorageEnclosureDismantlingStart {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StorageEnclosureDismantlingStartOutcome, StorageEnclosureDismantlingCommitError>
    {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.reservation.expected_revision() {
            return Err(
                StorageEnclosureDismantlingCommitError::StaleInventoryRevision {
                    expected: self.reservation.expected_revision(),
                    actual: actual_revision,
                },
            );
        }
        let target = state.inventory().get_stockpile(self.target).ok_or(
            StorageEnclosureDismantlingCommitError::UnknownTarget {
                stockpile: self.target,
            },
        )?;
        if target.storage_profile() != self.expected_profile {
            return Err(
                StorageEnclosureDismantlingCommitError::TargetProfileChanged {
                    stockpile: self.target,
                },
            );
        }
        let enclosure = target.enclosure().ok_or(
            StorageEnclosureDismantlingCommitError::TargetEnclosureChanged {
                stockpile: self.target,
            },
        )?;
        if enclosure.definition() != self.definition
            || enclosure.created_at() != self.enclosure_created_at
        {
            return Err(
                StorageEnclosureDismantlingCommitError::TargetEnclosureChanged {
                    stockpile: self.target,
                },
            );
        }
        self.player_work
            .precheck(state)
            .map_err(StorageEnclosureDismantlingCommitError::PlayerWork)?;
        self.reservation.assert_matches_state(state.inventory());
        self.reservation.apply(state.inventory_state_mut());
        self.player_work.apply(state);
        Ok(StorageEnclosureDismantlingStartOutcome {
            target: self.work.target(),
            recovery_destination: self.work.recovery_destination(),
            definition: self.work.definition(),
            recovered_mass: self.work.recovered_mass(),
            completes_at: self.work.completes_at(),
        })
    }
}

#[cfg(test)]
#[path = "enclosure_dismantling_tests.rs"]
mod tests;
