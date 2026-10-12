//! Canonical persisted representation of immediate player traversal access.

use serde::{Deserialize, Serialize};

use crate::spatial::VoxelCoord;

/// One undirected, explicitly authorized face-adjacent player traversal edge.
///
/// Endpoints are stored in coordinate order so persisted traversal access has one canonical
/// representation. World generation owns creation; ordinary movement only queries these edges.
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
