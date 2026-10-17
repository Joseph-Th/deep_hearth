//! Canonical persisted representation of bounded opening-access topology.

use serde::{Deserialize, Serialize};

use crate::spatial::VoxelCoord;

/// One undirected, persisted face-adjacent opening-access edge.
///
/// Endpoints are stored in coordinate order so the bounded observation/planning graph has one
/// canonical representation. World generation owns creation; continuous body locomotion does not
/// require these edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraversalEdge {
    first: VoxelCoord,
    second: VoxelCoord,
}

impl TraversalEdge {
    #[must_use]
    pub(crate) fn new(left: VoxelCoord, right: VoxelCoord) -> Option<Self> {
        if !left.is_face_adjacent_to(right) {
            return None;
        }
        let (first, second) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        Some(Self { first, second })
    }

    #[must_use]
    pub(crate) const fn endpoints(self) -> (VoxelCoord, VoxelCoord) {
        (self.first, self.second)
    }

    #[must_use]
    pub(crate) fn other(self, endpoint: VoxelCoord) -> Option<VoxelCoord> {
        if endpoint == self.first {
            Some(self.second)
        } else if endpoint == self.second {
            Some(self.first)
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn is_canonical(self) -> bool {
        self.first < self.second && self.first.is_face_adjacent_to(self.second)
    }
}
