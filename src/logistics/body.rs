//! Persisted continuous player-body state owned by logistics.

use serde::{Deserialize, Serialize};

use crate::spatial::{HorizontalVelocity, VerticalVelocity, VoxelCoord, WorldPoint};

/// Persisted collision stance for the continuous player body.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStance {
    #[default]
    Standing,
    Crouching,
}

/// Authoritative continuous body transform for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerBodyState {
    foot_position: WorldPoint,
    horizontal_velocity: HorizontalVelocity,
    vertical_velocity: VerticalVelocity,
    stance: PlayerStance,
}

impl PlayerBodyState {
    #[must_use]
    pub fn centered_on_voxel_floor(position: VoxelCoord) -> Option<Self> {
        Some(Self {
            foot_position: WorldPoint::centered_on_voxel_floor(position)?,
            horizontal_velocity: HorizontalVelocity::ZERO,
            vertical_velocity: VerticalVelocity::ZERO,
            stance: PlayerStance::Standing,
        })
    }

    #[must_use]
    pub const fn foot_position(self) -> WorldPoint {
        self.foot_position
    }

    #[must_use]
    pub const fn horizontal_velocity(self) -> HorizontalVelocity {
        self.horizontal_velocity
    }

    #[must_use]
    pub const fn vertical_velocity(self) -> VerticalVelocity {
        self.vertical_velocity
    }

    #[must_use]
    pub const fn stance(self) -> PlayerStance {
        self.stance
    }

    /// Discrete locality used by existing custody/access systems.
    #[must_use]
    pub const fn voxel_position(self) -> VoxelCoord {
        self.foot_position.containing_voxel()
    }

    pub(crate) const fn with_motion(
        self,
        foot_position: WorldPoint,
        horizontal_velocity: HorizontalVelocity,
        vertical_velocity: VerticalVelocity,
        stance: PlayerStance,
    ) -> Self {
        Self {
            foot_position,
            horizontal_velocity,
            vertical_velocity,
            stance,
        }
    }

    pub(crate) const fn with_stance(self, stance: PlayerStance) -> Self {
        Self { stance, ..self }
    }

    #[cfg(test)]
    pub(crate) fn snap_to_voxel_floor(self, position: VoxelCoord) -> Self {
        Self::centered_on_voxel_floor(position).unwrap_or_else(|| {
            panic!("test player relocation destination must fit continuous world space")
        })
    }
}
