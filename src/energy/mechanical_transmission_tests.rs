//! Physical topology limits for non-teleporting line shafts.

use super::*;

fn roller_set(cells: &[(i64, i64)]) -> BTreeMap<VoxelCoord, StructuralElementId> {
    cells
        .iter()
        .enumerate()
        .map(|(index, &(x, z))| {
            (
                VoxelCoord::new(x, 5, z),
                StructuralElementId::new(u32::try_from(index + 1).unwrap()),
            )
        })
        .collect()
}

#[test]
fn only_an_externally_bounded_contiguous_straight_run_has_endpoint_cells() {
    let rollers = roller_set(&[(1, 2), (2, 2), (3, 2)]);
    let (path, low, high) = discover_shaft_run(&rollers, VoxelCoord::new(1, 5, 2), 1, 0)
        .expect("the whole shaft must resolve from its leftmost roller");
    assert_eq!(path.len(), 3);
    assert_eq!(low, VoxelCoord::new(0, 5, 2));
    assert_eq!(high, VoxelCoord::new(4, 5, 2));
    assert_eq!(
        path,
        [
            StructuralElementId::new(1),
            StructuralElementId::new(2),
            StructuralElementId::new(3),
        ]
    );
    assert!(discover_shaft_run(&rollers, VoxelCoord::new(2, 5, 2), 1, 0).is_none());
    assert!(discover_shaft_run(&rollers, VoxelCoord::new(3, 5, 2), 1, 0).is_none());
    assert_eq!(
        discover_shaft_run(&rollers, VoxelCoord::new(1, 5, 2), 0, 1)
            .unwrap()
            .0
            .len(),
        1,
        "a 90-degree shaft cannot turn without a dedicated gear or coupling",
    );

    let mut gap = rollers;
    gap.remove(&VoxelCoord::new(2, 5, 2));
    let (left, _, left_end) = discover_shaft_run(&gap, VoxelCoord::new(1, 5, 2), 1, 0).unwrap();
    let (right, right_start, _) = discover_shaft_run(&gap, VoxelCoord::new(3, 5, 2), 1, 0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(right.len(), 1);
    assert_eq!(left_end, VoxelCoord::new(2, 5, 2));
    assert_eq!(right_start, VoxelCoord::new(2, 5, 2));
}

#[test]
fn runs_resolve_on_both_axes_and_do_not_overflow_world_coordinate_limits() {
    let rollers = roller_set(&[(2, 1), (2, 2), (2, 3)]);
    let (path, low, high) = discover_shaft_run(&rollers, VoxelCoord::new(2, 5, 1), 0, 1).unwrap();
    assert_eq!(path.len(), 3);
    assert_eq!(low, VoxelCoord::new(2, 5, 0));
    assert_eq!(high, VoxelCoord::new(2, 5, 4));
    assert!(
        discover_shaft_run(
            &roller_set(&[(i64::MAX, 2)]),
            VoxelCoord::new(i64::MAX, 5, 2),
            1,
            0,
        )
        .is_none()
    );
    assert!(
        discover_shaft_run(
            &roller_set(&[(i64::MIN, 2)]),
            VoxelCoord::new(i64::MIN, 5, 2),
            1,
            0,
        )
        .is_none()
    );
}
