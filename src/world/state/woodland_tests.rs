//! Local woodland lookups must not scan or reveal remote north-south trunks.

use super::*;
use crate::core::quantity::Mass;
use crate::spatial::VoxelBounds;

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
