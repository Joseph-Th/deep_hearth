//! Familiar maximum-fit stack transfer between carried custody and one exact-local container.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::{MaterialStackProjection, StockpileId};
use crate::registry::Registries;

use super::ground::validate_ground_access;
use super::{
    GroundMaterialTransferCommitError, GroundMaterialTransferError,
    ValidatedGroundMaterialTransfer, validate_drop_stack_to_ground,
    validate_pickup_stack_from_ground,
};

/// Failure while composing one familiar quick-move gesture over canonical material relocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerQuickMoveError {
    PlayerNotAdmitted,
    UnknownDestination {
        stockpile: StockpileId,
    },
    DestinationFull {
        stockpile: StockpileId,
    },
    UnsupportedEndpoints {
        source: StockpileId,
        destination: StockpileId,
        carried: StockpileId,
    },
    Transfer(GroundMaterialTransferError),
}

impl Display for PlayerQuickMoveError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "quick-move destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::DestinationFull { stockpile } => write!(
                formatter,
                "quick-move destination stockpile {} has no available capacity",
                stockpile.value()
            ),
            Self::UnsupportedEndpoints {
                source,
                destination,
                carried,
            } => write!(
                formatter,
                "quick-move requires exactly one carried endpoint {}; source is {}, destination is {}",
                carried.value(),
                source.value(),
                destination.value()
            ),
            Self::Transfer(error) => write!(formatter, "quick-move transfer failed: {error}"),
        }
    }
}

impl Error for PlayerQuickMoveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transfer(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::UnknownDestination { .. }
            | Self::DestinationFull { .. }
            | Self::UnsupportedEndpoints { .. } => None,
        }
    }
}

/// Committed result of one maximum-fit carried/container transfer gesture.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerQuickMoveOutcome {
    source: StockpileId,
    destination: StockpileId,
    mass: Mass,
}

impl PlayerQuickMoveOutcome {
    #[must_use]
    pub const fn source(self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn destination(self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

/// Revision-bound familiar quick-move action.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerQuickMove {
    outcome: PlayerQuickMoveOutcome,
    transfer: ValidatedGroundMaterialTransfer,
}

impl ValidatedPlayerQuickMove {
    pub const fn outcome(&self) -> PlayerQuickMoveOutcome {
        self.outcome
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerQuickMoveOutcome, GroundMaterialTransferCommitError> {
        self.transfer.commit(state)?;
        Ok(self.outcome)
    }
}

/// Quick-moves as much of one familiar stack as destination capacity currently permits.
///
/// Exactly one endpoint must be the admitted player's carried stockpile. The other endpoint is
/// still validated by the existing same-voxel ground-transfer owner, so this convenience surface
/// does not weaken locality, storage-profile, structural-load, movement, survival, or stale-state
/// checks. A caller typically supplies the opened exact-local container as `destination` for a
/// carried stack, or the carried stockpile as `destination` for a container stack.
pub fn validate_quick_move_player_stack(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    destination: StockpileId,
) -> Result<ValidatedPlayerQuickMove, PlayerQuickMoveError> {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .ok_or(PlayerQuickMoveError::PlayerNotAdmitted)?;
    let source = stack.stockpile();
    let direction = match (source == carried, destination == carried) {
        (true, false) => QuickMoveDirection::Drop,
        (false, true) => QuickMoveDirection::Pickup,
        _ => {
            return Err(PlayerQuickMoveError::UnsupportedEndpoints {
                source,
                destination,
                carried,
            });
        }
    };
    // Authorize the ground endpoint before consulting its capacity. Otherwise a remote
    // or dead-player request could reveal container fullness instead of the canonical
    // locality/liveness rejection returned by the transfer owner.
    let ground = match direction {
        QuickMoveDirection::Pickup => source,
        QuickMoveDirection::Drop => destination,
    };
    let _ = validate_ground_access(state, ground).map_err(PlayerQuickMoveError::Transfer)?;
    // A full destination must not turn an obsolete inventory selection into a seemingly
    // current capacity assessment. Keep locality/liveness checks ahead of this snapshot check.
    stack
        .validate_current(state)
        .map_err(GroundMaterialTransferError::Stack)
        .map_err(PlayerQuickMoveError::Transfer)?;
    let destination_record = state.inventory().get_stockpile(destination).ok_or(
        PlayerQuickMoveError::UnknownDestination {
            stockpile: destination,
        },
    )?;
    let mass = stack.mass().min(destination_record.available_capacity());
    if mass.is_zero() {
        return Err(PlayerQuickMoveError::DestinationFull {
            stockpile: destination,
        });
    }
    let transfer = match direction {
        QuickMoveDirection::Pickup => {
            validate_pickup_stack_from_ground(registries, state, source, stack, mass)
        }
        QuickMoveDirection::Drop => {
            validate_drop_stack_to_ground(registries, state, destination, stack, mass)
        }
    }
    .map_err(PlayerQuickMoveError::Transfer)?;
    Ok(ValidatedPlayerQuickMove {
        outcome: PlayerQuickMoveOutcome {
            source,
            destination,
            mass,
        },
        transfer,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickMoveDirection {
    Pickup,
    Drop,
}

#[cfg(test)]
#[path = "quick_move_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "quick_move_access_tests.rs"]
mod access_tests;
