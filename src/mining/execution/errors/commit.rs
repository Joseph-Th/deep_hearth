//! Late commit failures after a validated mining start becomes stale.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::labor::PlayerWorkCommitError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartCommitError {
    TargetNoLongerResolved,
    /// Hidden source state changed after validation. Exact reserve values stay non-oracular.
    TargetChanged,
    StaleLogistics {
        expected: u64,
        actual: u64,
    },
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    StaleMining {
        expected: u64,
        actual: u64,
    },
    StaleStructure {
        expected: u64,
        actual: u64,
    },
    Work(PlayerWorkCommitError),
}

impl Display for MiningStartCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetNoLongerResolved => formatter.write_str(
                "validated mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::TargetChanged => formatter.write_str(
                "validated mining target changed after validation; resolve the target again",
            ),
            Self::StaleLogistics { expected, actual } => write!(
                formatter,
                "validated mining start expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "validated mining start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining start expected mining revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructure { expected, actual } => write!(
                formatter,
                "validated mining start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::Work(error) => write!(
                formatter,
                "validated mining start player-work state changed: {error}"
            ),
        }
    }
}

impl Error for MiningStartCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::TargetNoLongerResolved
            | Self::TargetChanged
            | Self::StaleLogistics { .. }
            | Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::StaleMining { .. }
            | Self::StaleStructure { .. } => None,
        }
    }
}
