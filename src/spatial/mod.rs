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
#[serde(deny_unknown_fields)]
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Half-open axis-aligned voxel bounds `[min, max_exclusive)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct VoxelBounds {
    min: VoxelCoord,
    max_exclusive: VoxelCoord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AxisContact {
    Separate,
    Abut,
    Overlap,
}

const fn axis_contact(left_min: i64, left_max: i64, right_min: i64, right_max: i64) -> AxisContact {
    if left_min < right_max && right_min < left_max {
        AxisContact::Overlap
    } else if left_max == right_min || right_max == left_min {
        AxisContact::Abut
    } else {
        AxisContact::Separate
    }
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
    pub const fn has_voxel(self, coord: VoxelCoord) -> bool {
        coord.x >= self.min.x
            && coord.x < self.max_exclusive.x
            && coord.y >= self.min.y
            && coord.y < self.max_exclusive.y
            && coord.z >= self.min.z
            && coord.z < self.max_exclusive.z
    }

    /// Reports whether every voxel in `other` lies inside these half-open bounds.
    #[must_use]
    pub const fn contains_bounds(self, other: Self) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.min.z <= other.min.z
            && self.max_exclusive.x >= other.max_exclusive.x
            && self.max_exclusive.y >= other.max_exclusive.y
            && self.max_exclusive.z >= other.max_exclusive.z
    }

    /// Reports whether two nonempty half-open bounds share at least one voxel.
    #[must_use]
    pub const fn has_intersection(self, other: Self) -> bool {
        matches!(
            (
                axis_contact(
                    self.min.x,
                    self.max_exclusive.x,
                    other.min.x,
                    other.max_exclusive.x,
                ),
                axis_contact(
                    self.min.y,
                    self.max_exclusive.y,
                    other.min.y,
                    other.max_exclusive.y,
                ),
                axis_contact(
                    self.min.z,
                    self.max_exclusive.z,
                    other.min.z,
                    other.max_exclusive.z,
                ),
            ),
            (
                AxisContact::Overlap,
                AxisContact::Overlap,
                AxisContact::Overlap
            )
        )
    }

    /// Reports whether two voxel regions share positive-area contact.
    ///
    /// Shared volume counts as contact. Otherwise the bounds must overlap with positive extent on
    /// two axes and abut on the third. Edge-only and corner-only touches return false.
    #[must_use]
    pub const fn has_face_contact(self, other: Self) -> bool {
        use AxisContact::{Abut, Overlap};

        matches!(
            (
                axis_contact(
                    self.min.x,
                    self.max_exclusive.x,
                    other.min.x,
                    other.max_exclusive.x,
                ),
                axis_contact(
                    self.min.y,
                    self.max_exclusive.y,
                    other.min.y,
                    other.max_exclusive.y,
                ),
                axis_contact(
                    self.min.z,
                    self.max_exclusive.z,
                    other.min.z,
                    other.max_exclusive.z,
                ),
            ),
            (Overlap, Overlap, Overlap)
                | (Abut, Overlap, Overlap)
                | (Overlap, Abut, Overlap)
                | (Overlap, Overlap, Abut)
        )
    }

    /// Returns the nonempty half-open overlap of two bounds, if one exists.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let min = VoxelCoord::new(
            std::cmp::max(self.min.x, other.min.x),
            std::cmp::max(self.min.y, other.min.y),
            std::cmp::max(self.min.z, other.min.z),
        );
        let max_exclusive = VoxelCoord::new(
            std::cmp::min(self.max_exclusive.x, other.max_exclusive.x),
            std::cmp::min(self.max_exclusive.y, other.max_exclusive.y),
            std::cmp::min(self.max_exclusive.z, other.max_exclusive.z),
        );
        Self::new(min, max_exclusive).ok()
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
#[path = "mod_tests.rs"]
mod tests;
