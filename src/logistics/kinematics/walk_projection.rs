//! Read-only local walk projections through the authoritative player-body kernel.

mod replay;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::{SurvivalExertion, SurvivalResourceBudget, SurvivalResourceProjectionError};

use super::{PlayerBodyMotionError, assess_player_carrying};

/// Failure to project one explicit, caller-supplied horizontal walk path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWalkProjectionError {
    PathEmpty,
    ProjectedLoadExceedsCapacity {
        carried: Mass,
        capacity: Mass,
    },
    PlayerNotAdmitted,
    PlayerDead,
    RequiresSimulationBoundary,
    PlayerBusy,
    PlayerNotStanding,
    PlayerAlreadyMoving,
    PlayerNotGrounded,
    PendingLocomotionExertion,
    DestinationNotHorizontalAdjacent {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    DestinationUnreachableBeforeBoundary {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    DestinationOvershot {
        destination: VoxelCoord,
        actual: VoxelCoord,
    },
    BodyMotion(PlayerBodyMotionError),
    ResourceProjection(SurvivalResourceProjectionError),
}

impl Display for PlayerWalkProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathEmpty => formatter.write_str("walk projection path is empty"),
            Self::ProjectedLoadExceedsCapacity { carried, capacity } => write!(
                formatter,
                "walk projection carried mass {} mg exceeds capacity {} mg",
                carried.milligrams(),
                capacity.milligrams()
            ),
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerDead => formatter.write_str("dead player cannot project a walk"),
            Self::RequiresSimulationBoundary => {
                formatter.write_str("walk projection requires a settled simulation boundary")
            }
            Self::PlayerBusy => formatter.write_str("player is already performing work"),
            Self::PlayerNotStanding => {
                formatter.write_str("player must be standing to project a walk")
            }
            Self::PlayerAlreadyMoving => {
                formatter.write_str("player must be motionless to project a walk")
            }
            Self::PlayerNotGrounded => {
                formatter.write_str("player must be grounded to project a walk")
            }
            Self::PendingLocomotionExertion => formatter
                .write_str("player must settle prior locomotion exertion before projecting a walk"),
            Self::DestinationNotHorizontalAdjacent {
                origin,
                destination,
            } => write!(
                formatter,
                "walk destination {destination:?} is not horizontally face-adjacent to {origin:?}"
            ),
            Self::DestinationUnreachableBeforeBoundary {
                origin,
                destination,
            } => write!(
                formatter,
                "walk from {origin:?} to {destination:?} cannot settle before the next simulation boundary"
            ),
            Self::DestinationOvershot {
                destination,
                actual,
            } => write!(
                formatter,
                "walk toward {destination:?} entered unexpected voxel {actual:?}"
            ),
            Self::BodyMotion(error) => write!(formatter, "player body motion failed: {error}"),
            Self::ResourceProjection(error) => {
                write!(formatter, "walk survival projection failed: {error:?}")
            }
        }
    }
}

impl Error for PlayerWalkProjectionError {}

/// Exact control-time and next-boundary survival cost for an explicit local walk path.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWalkProjection {
    origin: VoxelCoord,
    destination: VoxelCoord,
    control_ticks: u64,
    locomotion_exertion: SurvivalExertion,
    resource_budget: SurvivalResourceBudget,
}

impl PlayerWalkProjection {
    #[must_use]
    pub const fn origin(self) -> VoxelCoord {
        self.origin
    }

    #[must_use]
    pub const fn destination(self) -> VoxelCoord {
        self.destination
    }

    #[must_use]
    pub const fn control_ticks(self) -> u64 {
        self.control_ticks
    }

    #[must_use]
    pub const fn locomotion_exertion(self) -> SurvivalExertion {
        self.locomotion_exertion
    }

    pub const fn resource_budget(self) -> SurvivalResourceBudget {
        self.resource_budget
    }
}

/// Projects one horizontal face-adjacent walk from a settled slow boundary.
///
/// This is a convenience wrapper over [`project_player_walk_path`]. It does not generate routes.
pub fn project_player_adjacent_walk(
    registries: &Registries,
    state: &AppState,
    destination: VoxelCoord,
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    project_player_walk_path(registries, state, &[destination])
}

/// Projects a caller-supplied sequence of horizontal face-adjacent walk destinations.
///
/// The current player must be standing, grounded, motionless, idle, and at a slow-simulation
/// boundary with prior locomotion exertion settled. Every leg is resolved through the canonical
/// body/collision kernel. Straight consecutive legs retain held-input momentum; a turn requires
/// braking at its waypoint, and the final destination must be motionless before the next slow
/// boundary. The result's resource budget is the exact basal-plus-locomotion charge at that boundary.
/// This function deliberately does not discover, rank, or route between destinations.
pub fn project_player_walk_path(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    let carrying =
        assess_player_carrying(state).ok_or(PlayerWalkProjectionError::PlayerNotAdmitted)?;
    replay::project_walk_path_with_load(
        registries,
        state,
        destinations,
        carrying.stored(),
        carrying.capacity(),
    )
}

/// Projects an explicit walk path using a hypothetical future carried load.
///
/// Both walking speed and locomotion exertion use the projected mass, so the caller can compare
/// planned return hauls without manufacturing inventory custody. Physical capacity
/// is always read from the admitted body's authoritative carrying assessment,
/// rather than trusted as an arbitrary caller-supplied hypothetical. Route
/// discovery remains caller-owned.
pub fn project_player_walk_path_for_load(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
    carried: Mass,
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    let capacity = assess_player_carrying(state)
        .ok_or(PlayerWalkProjectionError::PlayerNotAdmitted)?
        .capacity();
    replay::project_walk_path_with_load(registries, state, destinations, carried, capacity)
}

#[cfg(test)]
#[path = "walk_projection_tests.rs"]
mod tests;
