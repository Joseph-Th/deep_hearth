//! Persistent exclusive ownership of the locally controlled player's active work.

use serde::{Deserialize, Serialize};

use crate::mining::MiningJobId;
use crate::production::ProductionJobId;

/// Durable activity currently monopolizing the local player's labor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerWork {
    ManualCraft { job: ProductionJobId },
    Mining { job: MiningJobId },
}

/// Single-player labor owner with an explicit revision for cross-system transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerWorkState {
    revision: u64,
    active: Option<PlayerWork>,
}

impl PlayerWorkState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            active: None,
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn active(&self) -> Option<PlayerWork> {
        self.active
    }

    pub(crate) fn apply_start(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        work: PlayerWork,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert!(self.active.is_none());
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        self.active = Some(work);
        self.revision = next_revision;
    }

    pub(crate) fn apply_release(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        work: PlayerWork,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(self.active, Some(work));
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        self.active = None;
        self.revision = next_revision;
    }
}
