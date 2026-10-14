//! Persisted continuous player-body state owned by logistics.

use serde::{Deserialize, Serialize};

use crate::spatial::{VerticalVelocity, VoxelCoord, WorldPoint};

/// Authoritative continuous body transform for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerBodyState {
    foot_position: WorldPoint,
    vertical_velocity: VerticalVelocity,
}

impl PlayerBodyState {
    #[must_use]
    pub fn centered_on_voxel_floor(position: VoxelCoord) -> Option<Self> {
        Some(Self {
            foot_position: WorldPoint::centered_on_voxel_floor(position)?,
            vertical_velocity: VerticalVelocity::ZERO,
        })
    }

    #[must_use]
    pub const fn foot_position(self) -> WorldPoint {
        self.foot_position
    }

    #[must_use]
    pub const fn vertical_velocity(self) -> VerticalVelocity {
        self.vertical_velocity
    }

    /// Discrete locality used by existing custody/access systems.
    #[must_use]
    pub const fn voxel_position(self) -> VoxelCoord {
        self.foot_position.containing_voxel()
    }

    pub(crate) const fn with_motion(
        self,
        foot_position: WorldPoint,
        vertical_velocity: VerticalVelocity,
    ) -> Self {
        Self {
            foot_position,
            vertical_velocity,
        }
    }

    pub(crate) fn snap_to_voxel_floor(self, position: VoxelCoord) -> Self {
        Self::centered_on_voxel_floor(position).unwrap_or_else(|| {
            panic!("validated voxel step destination must fit continuous world space")
        })
    }
}
