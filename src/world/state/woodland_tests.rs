//! Local woodland lookups must not scan or reveal remote north-south trunks.

use super::*;
use crate::core::quantity::Mass;
use crate::spatial::VoxelBounds;

#[test]
fn trunk_clearance_matches_canonical_root_scan_across_all_axes() {
    let mut world = WorldState::new();
    for x in -2..=2 {
        for z in -2..=2 {
            // Deliberately vary root height independently of horizontal order.
            let root = VoxelCoord::new(x, (x + 2 * z) % 3, z);
            world.trees.insert(
                TreeSpatialKey::from(root),
                TreeRecord::new(root, Mass::from_milligrams(8_000_000)),
            );
        }
    }
    for root in [
        VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN),
        VoxelCoord::new(i64::MAX - 1, i64::MAX - 1, i64::MAX - 1),
        VoxelCoord::new(0, i64::MIN, 0),
        VoxelCoord::new(0, i64::MAX, 0),
        VoxelCoord::new(0, 0, -10_000),
        VoxelCoord::new(0, 0, 10_000),
    ] {
        world.trees.insert(
            TreeSpatialKey::from(root),
            TreeRecord::new(root, Mass::from_milligrams(8_000_000)),
        );
    }
    let compare = |bounds: VoxelBounds| {
        let expected = world.trees().find_map(|tree| {
            let root = tree.root();
            if bounds.has_voxel(root) {
                Some(root)
            } else {
                root.y()
                    .checked_add(1)
                    .map(|y| VoxelCoord::new(root.x(), y, root.z()))
                    .filter(|upper| bounds.has_voxel(*upper))
            }
        });
        assert_eq!(
            world.first_tree_trunk_in_bounds(bounds),
            expected,
            "wrong first obstruction for {bounds:?}"
        );
    };
    for x0 in -3..3 {
        for x1 in x0 + 1..=3 {
            for y0 in -3..3 {
                for y1 in y0 + 1..=3 {
                    for z0 in -3..3 {
                        for z1 in z0 + 1..=3 {
                            compare(
                                VoxelBounds::new(
                                    VoxelCoord::new(x0, y0, z0),
                                    VoxelCoord::new(x1, y1, z1),
                                )
                                .unwrap(),
                            );
                        }
                    }
                }
            }
        }
    }
    for (min, max) in [
        (
            VoxelCoord::new(i64::MIN, i64::MIN, i64::MIN),
            VoxelCoord::new(i64::MIN + 1, i64::MIN + 1, i64::MIN + 1),
        ),
        (
            VoxelCoord::new(i64::MIN, i64::MIN + 1, i64::MIN),
            VoxelCoord::new(i64::MIN + 1, i64::MIN + 2, i64::MIN + 1),
        ),
        (
            VoxelCoord::new(i64::MAX - 1, i64::MAX - 1, i64::MAX - 1),
            VoxelCoord::new(i64::MAX, i64::MAX, i64::MAX),
        ),
        (
            VoxelCoord::new(0, i64::MAX - 1, 0),
            VoxelCoord::new(1, i64::MAX, 1),
        ),
        (
            VoxelCoord::new(i64::MIN, 8, i64::MIN),
            VoxelCoord::new(i64::MAX, 9, i64::MAX),
        ),
    ] {
        compare(VoxelBounds::new(min, max).unwrap());
    }
}

#[test]
fn standing_tree_index_bounds_both_horizontal_axes_and_roundtrips_canonically() {
    let mut world = WorldState::new();
    let nearby = [
        VoxelCoord::new(-3, 0, -3),
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(1, 0, 1),
        VoxelCoord::new(3, 0, 0),
    ];
    let mut roots = nearby.to_vec();
    // This single X column spans thousands of otherwise irrelevant Z sites.
    // Horizontal tree queries are frequent sky and canopy operations.
    for z in -2_000_i64..=2_000 {
        if z.abs() > 3 {
            roots.push(VoxelCoord::new(0, 0, z));
        }
    }
    roots.extend([
        VoxelCoord::new(i64::MIN, 0, i64::MIN),
        VoxelCoord::new(i64::MAX, 0, i64::MAX),
    ]);
    for root in roots {
        let tree = TreeRecord::new(root, Mass::from_milligrams(8_000_000));
        assert!(
            world
                .trees
                .insert(TreeSpatialKey::from(root), tree)
                .is_none()
        );
    }
    assert_eq!(
        world
            .trees_near_horizontal(VoxelCoord::new(0, 0, 0), 3)
            .map(|tree| tree.root())
            .collect::<Vec<_>>(),
        nearby,
    );
    assert_eq!(
        world
            .trees_near_horizontal(VoxelCoord::new(i64::MIN, 0, i64::MIN), 0)
            .map(|tree| tree.root())
            .collect::<Vec<_>>(),
        [VoxelCoord::new(i64::MIN, 0, i64::MIN)],
    );
    assert_eq!(
        world
            .trees_near_horizontal(VoxelCoord::new(i64::MAX, 0, i64::MAX), 0)
            .map(|tree| tree.root())
            .collect::<Vec<_>>(),
        [VoxelCoord::new(i64::MAX, 0, i64::MAX)],
    );
    for root in nearby {
        assert_eq!(world.tree_at(root).map(|tree| tree.root()), Some(root));
    }
    assert_eq!(
        world.first_tree_trunk_in_bounds(
            VoxelBounds::new(VoxelCoord::new(0, 1, 0), VoxelCoord::new(1, 2, 1)).unwrap(),
        ),
        Some(VoxelCoord::new(0, 1, 0)),
        "an upper living trunk cell must still obstruct exact construction bounds"
    );

    let serialized = serde_json::to_value(&world).unwrap();
    let restored: WorldState = serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(restored, world);
    assert_eq!(
        restored
            .trees_near_horizontal(VoxelCoord::new(0, 0, 0), 3)
            .map(|tree| tree.root())
            .collect::<Vec<_>>(),
        nearby,
    );
    let mut noncanonical = serialized;
    noncanonical["trees"].as_array_mut().unwrap().swap(0, 1);
    assert!(
        serde_json::from_value::<WorldState>(noncanonical).is_err(),
        "a reordered save cannot silently change the canonical tree identity index"
    );
}
