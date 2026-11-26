//! Exact bounded membership and ordering of the shared sparse range traversal.

use super::*;

fn assert_matches_direct_filter(
    index: &BTreeMap<VoxelCoord, u32>,
    min: VoxelCoord,
    max: VoxelCoord,
) {
    let expected = index
        .iter()
        .filter(|(at, _)| {
            (min.x()..=max.x()).contains(&at.x())
                && (min.y()..=max.y()).contains(&at.y())
                && (min.z()..=max.z()).contains(&at.z())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        indexed_positions_in_box(index, min, max).collect::<Vec<_>>(),
        expected,
        "inclusive query {min:?}..={max:?}"
    );
}

#[test]
fn bounded_index_matches_ordered_filter_for_dense_and_sparse_boxes() {
    let mut dense = BTreeMap::new();
    let mut sparse = BTreeMap::new();
    for x in -2..=2 {
        for y in -2..=2 {
            for z in -2..=2 {
                let at = VoxelCoord::new(x, y, z);
                let id = dense.len() as u32;
                dense.insert(at, id);
                if (x + 2 * y - z) % 3 == 0 {
                    sparse.insert(at, id);
                }
            }
        }
    }
    // Empty boxes, exact points, partially overlapping and full bounds.
    for x0 in -3..=3 {
        for x1 in x0..=3 {
            for y0 in -3..=3 {
                for y1 in y0..=3 {
                    for z0 in -3..=3 {
                        for z1 in z0..=3 {
                            let min = VoxelCoord::new(x0, y0, z0);
                            let max = VoxelCoord::new(x1, y1, z1);
                            assert_matches_direct_filter(&dense, min, max);
                            assert_matches_direct_filter(&sparse, min, max);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn bounded_index_handles_coordinate_extremes_and_sparse_gaps() {
    let limits = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];
    let mut index = BTreeMap::new();
    for x in limits {
        for y in limits {
            for z in limits {
                index.insert(VoxelCoord::new(x, y, z), index.len() as u32);
            }
        }
    }
    for (min, max) in [
        (
            VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN),
            VoxelCoord::new(i64::MAX, i64::MAX, i64::MAX),
        ),
        (
            VoxelCoord::new(i64::MIN, i64::MAX, i64::MAX),
            VoxelCoord::new(i64::MAX, i64::MAX, i64::MAX),
        ),
        (
            VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN),
            VoxelCoord::new(i64::MAX, i64::MIN, i64::MIN),
        ),
        (
            VoxelCoord::new(i64::MAX, i64::MIN, -1),
            VoxelCoord::new(i64::MAX, i64::MAX, 1),
        ),
        (
            VoxelCoord::new(i64::MIN, -1, i64::MIN),
            VoxelCoord::new(i64::MAX, 1, i64::MIN),
        ),
        (
            VoxelCoord::new(-1, i64::MIN, i64::MAX),
            VoxelCoord::new(1, i64::MAX, i64::MAX),
        ),
        (
            VoxelCoord::new(2, 2, 2),
            VoxelCoord::new(i64::MAX - 2, i64::MAX - 2, i64::MAX - 2),
        ),
    ] {
        assert_matches_direct_filter(&index, min, max);
    }
    let empty = BTreeMap::new();
    assert_matches_direct_filter(
        &empty,
        VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN),
        VoxelCoord::new(i64::MAX, i64::MAX, i64::MAX),
    );
}
