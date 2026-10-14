//! Fixed-point continuous world-space primitives for deterministic direct motion.

use serde::{Deserialize, Serialize};

use super::VoxelCoord;

pub const NANOMETERS_PER_VOXEL: i64 = 1_000_000_000;
const HALF_VOXEL_NANOMETERS: i64 = NANOMETERS_PER_VOXEL / 2;

/// Signed fixed-point point in authoritative world space.
///
/// One voxel is exactly one metre. Nanometre storage makes the 50 ms control-step integration of
/// authored human-scale velocities deterministic without floating-point state.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct WorldPoint {
    x_nanometers: i64,
    y_nanometers: i64,
    z_nanometers: i64,
}

impl WorldPoint {
    #[must_use]
    pub const fn new(x_nanometers: i64, y_nanometers: i64, z_nanometers: i64) -> Self {
        Self {
            x_nanometers,
            y_nanometers,
            z_nanometers,
        }
    }

    /// Places a body foot point at the horizontal center of one voxel and its lower Y face.
    #[must_use]
    pub fn centered_on_voxel_floor(voxel: VoxelCoord) -> Option<Self> {
        let x = voxel
            .x()
            .checked_mul(NANOMETERS_PER_VOXEL)?
            .checked_add(HALF_VOXEL_NANOMETERS)?;
        let y = voxel.y().checked_mul(NANOMETERS_PER_VOXEL)?;
        let z = voxel
            .z()
            .checked_mul(NANOMETERS_PER_VOXEL)?
            .checked_add(HALF_VOXEL_NANOMETERS)?;
        Some(Self::new(x, y, z))
    }

    #[must_use]
    pub const fn x_nanometers(self) -> i64 {
        self.x_nanometers
    }

    #[must_use]
    pub const fn y_nanometers(self) -> i64 {
        self.y_nanometers
    }

    #[must_use]
    pub const fn z_nanometers(self) -> i64 {
        self.z_nanometers
    }

    /// Voxel containing the body's foot point / horizontal center.
    #[must_use]
    pub const fn containing_voxel(self) -> VoxelCoord {
        VoxelCoord::new(
            self.x_nanometers.div_euclid(NANOMETERS_PER_VOXEL),
            self.y_nanometers.div_euclid(NANOMETERS_PER_VOXEL),
            self.z_nanometers.div_euclid(NANOMETERS_PER_VOXEL),
        )
    }
}

/// Signed fixed-point vertical velocity in nanometres per second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerticalVelocity {
    nanometers_per_second: i64,
}

impl VerticalVelocity {
    pub const ZERO: Self = Self::new(0);

    #[must_use]
    pub const fn new(nanometers_per_second: i64) -> Self {
        Self {
            nanometers_per_second,
        }
    }

    #[must_use]
    pub const fn nanometers_per_second(self) -> i64 {
        self.nanometers_per_second
    }
}

/// Signed fixed-point horizontal velocity in nanometres per second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HorizontalVelocity {
    x_nanometers_per_second: i64,
    z_nanometers_per_second: i64,
}

impl HorizontalVelocity {
    pub const ZERO: Self = Self::new(0, 0);

    #[must_use]
    pub const fn new(x_nanometers_per_second: i64, z_nanometers_per_second: i64) -> Self {
        Self {
            x_nanometers_per_second,
            z_nanometers_per_second,
        }
    }

    #[must_use]
    pub const fn x_nanometers_per_second(self) -> i64 {
        self.x_nanometers_per_second
    }

    #[must_use]
    pub const fn z_nanometers_per_second(self) -> i64 {
        self.z_nanometers_per_second
    }
}
