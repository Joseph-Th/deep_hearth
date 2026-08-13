//! Persistent chunk-agnostic voxel coordinates and checked spatial bounds for world-domain records.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

/// Persistent integer coordinate of one voxel in authoritative world space.
///
/// Coordinates deliberately do not encode a chunk size or streaming partition. Those are storage
/// and performance decisions that can change without changing persistent domain references.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct VoxelCoord {
    x: i64,
    y: i64,
    z: i64,
}

impl VoxelCoord {
    #[must_use]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn x(self) -> i64 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> i64 {
        self.y
    }

    #[must_use]
    pub const fn z(self) -> i64 {
        self.z
    }

    /// Applies a signed voxel displacement without allowing integer wraparound.
    #[must_use]
    pub const fn checked_offset(self, delta: VoxelDelta) -> Option<Self> {
        let Some(x) = self.x.checked_add(delta.x) else {
            return None;
        };
        let Some(y) = self.y.checked_add(delta.y) else {
            return None;
        };
        let Some(z) = self.z.checked_add(delta.z) else {
            return None;
        };
        Some(Self { x, y, z })
    }

    /// Returns a checked displacement from `other` to this coordinate.
    #[must_use]
    pub const fn checked_delta_from(self, other: Self) -> Option<VoxelDelta> {
        let Some(x) = self.x.checked_sub(other.x) else {
            return None;
        };
        let Some(y) = self.y.checked_sub(other.y) else {
            return None;
        };
        let Some(z) = self.z.checked_sub(other.z) else {
            return None;
        };
        Some(VoxelDelta { x, y, z })
    }
}

#[derive(Deserialize)]
struct VoxelBoundsRepresentation {
    min: VoxelCoord,
    max_exclusive: VoxelCoord,
}

impl<'de> Deserialize<'de> for VoxelBounds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = VoxelBoundsRepresentation::deserialize(deserializer)?;
        Self::new(representation.min, representation.max_exclusive)
            .map_err(serde::de::Error::custom)
    }
}

/// Signed voxel displacement independent of absolute world position.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct VoxelDelta {
    x: i64,
    y: i64,
    z: i64,
}

impl VoxelDelta {
    #[must_use]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn x(self) -> i64 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> i64 {
        self.y
    }

    #[must_use]
    pub const fn z(self) -> i64 {
        self.z
    }
}

/// Horizontal column coordinate used by climate, terrain-column, and hydrology projections.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct ColumnCoord {
    x: i64,
    z: i64,
}

impl ColumnCoord {
    #[must_use]
    pub const fn new(x: i64, z: i64) -> Self {
        Self { x, z }
    }

    #[must_use]
    pub const fn x(self) -> i64 {
        self.x
    }

    #[must_use]
    pub const fn z(self) -> i64 {
        self.z
    }
}

impl From<VoxelCoord> for ColumnCoord {
    fn from(value: VoxelCoord) -> Self {
        Self::new(value.x(), value.z())
    }
}

/// Half-open axis-aligned voxel bounds `[min, max_exclusive)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct VoxelBounds {
    min: VoxelCoord,
    max_exclusive: VoxelCoord,
}

impl VoxelBounds {
    /// Builds nonempty half-open bounds with strictly increasing extent on every axis.
    pub fn new(min: VoxelCoord, max_exclusive: VoxelCoord) -> Result<Self, VoxelBoundsError> {
        if min.x() >= max_exclusive.x() {
            return Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::X });
        }
        if min.y() >= max_exclusive.y() {
            return Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::Y });
        }
        if min.z() >= max_exclusive.z() {
            return Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::Z });
        }
        Ok(Self { min, max_exclusive })
    }

    #[must_use]
    pub const fn min(self) -> VoxelCoord {
        self.min
    }

    #[must_use]
    pub const fn max_exclusive(self) -> VoxelCoord {
        self.max_exclusive
    }

    #[must_use]
    pub const fn contains(self, coord: VoxelCoord) -> bool {
        coord.x >= self.min.x
            && coord.x < self.max_exclusive.x
            && coord.y >= self.min.y
            && coord.y < self.max_exclusive.y
            && coord.z >= self.min.z
            && coord.z < self.max_exclusive.z
    }

    /// Returns the exact voxel count when multiplication fits in `u128`.
    #[must_use]
    pub fn voxel_count(self) -> Option<u128> {
        let x = u128::try_from(i128::from(self.max_exclusive.x) - i128::from(self.min.x)).ok()?;
        let y = u128::try_from(i128::from(self.max_exclusive.y) - i128::from(self.min.y)).ok()?;
        let z = u128::try_from(i128::from(self.max_exclusive.z) - i128::from(self.min.z)).ok()?;
        x.checked_mul(y)?.checked_mul(z)
    }
}

/// Named Cartesian axis for spatial validation errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// Invalid half-open voxel bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelBoundsError {
    NonPositiveExtent { axis: Axis },
}

impl Display for VoxelBoundsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonPositiveExtent { axis } => {
                write!(formatter, "voxel bounds have nonpositive {axis:?} extent")
            }
        }
    }
}

impl Error for VoxelBoundsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_offset_rejects_coordinate_wraparound() {
        let origin = VoxelCoord::new(i64::MAX, 0, 0);

        assert_eq!(origin.checked_offset(VoxelDelta::new(1, 0, 0)), None);
        assert_eq!(
            VoxelCoord::new(10, 20, 30).checked_offset(VoxelDelta::new(-2, 3, -4)),
            Some(VoxelCoord::new(8, 23, 26))
        );
    }

    #[test]
    fn bounds_are_half_open_and_count_voxels_exactly() {
        let bounds = match VoxelBounds::new(VoxelCoord::new(-2, 10, 5), VoxelCoord::new(3, 12, 9)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("bounds fixture failed: {error}"),
        };

        assert!(bounds.contains(VoxelCoord::new(-2, 10, 5)));
        assert!(bounds.contains(VoxelCoord::new(2, 11, 8)));
        assert!(!bounds.contains(VoxelCoord::new(3, 11, 8)));
        assert_eq!(bounds.voxel_count(), Some(40));
    }

    #[test]
    fn bounds_reject_zero_or_negative_extent_per_axis() {
        assert_eq!(
            VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(0, 1, 1)),
            Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::X })
        );
        assert_eq!(
            VoxelBounds::new(VoxelCoord::new(0, 2, 0), VoxelCoord::new(1, 1, 1)),
            Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::Y })
        );
    }

    #[test]
    fn column_projection_discards_only_vertical_coordinate() {
        assert_eq!(
            ColumnCoord::from(VoxelCoord::new(12, -400, 91)),
            ColumnCoord::new(12, 91)
        );
    }
}
