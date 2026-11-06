//! Indexed living woodland, finite biomass growth, and permanent tree history.

use super::{FelledTreeRecord, TransplantedTreeRecord, TreeRecord, VoxelCoord, WorldState};

impl WorldState {
    /// Standing trees are indexed by root location, not repeatedly scanned as block entities.
    pub fn trees(&self) -> impl Iterator<Item = &TreeRecord> {
        self.trees.values()
    }

    #[must_use]
    pub fn tree_at(&self, root: VoxelCoord) -> Option<&TreeRecord> {
        self.trees.get(&root)
    }

    /// Only tree roots in a bounded horizontal footprint can shade one sky
    /// column. Index by x before filtering z; distant planted forests must
    /// not make one player's local light query scan every tree in the world.
    pub(crate) fn trees_near_horizontal(
        &self,
        position: VoxelCoord,
        radius: i64,
    ) -> impl Iterator<Item = &TreeRecord> {
        let low = position.x().saturating_sub(radius);
        let high = position.x().saturating_add(radius);
        let low_z = position.z().saturating_sub(radius);
        let high_z = position.z().saturating_add(radius);
        self.trees
            .range(
                VoxelCoord::new(low, i64::MIN, i64::MIN)
                    ..=VoxelCoord::new(high, i64::MAX, i64::MAX),
            )
            .map(|(_, tree)| tree)
            .filter(move |tree| (low_z..=high_z).contains(&tree.root().z()))
    }

    /// World-history receipts are for diagnostics and trusted-load checks, not discovery.
    pub(crate) fn felled_trees(&self) -> impl Iterator<Item = &FelledTreeRecord> {
        self.felled_trees.values()
    }

    pub(crate) fn transplants(&self) -> impl Iterator<Item = &TransplantedTreeRecord> {
        self.transplants.iter()
    }

    #[must_use]
    pub(crate) const fn committed_assimilated_wood_milligrams(&self) -> u128 {
        self.assimilated_wood_milligrams
    }

    /// Physical trunk voxels have the same collision authority as terrain blocks.
    #[must_use]
    pub fn tree_trunk_at(&self, voxel: VoxelCoord) -> Option<&TreeRecord> {
        self.tree_at(voxel).or_else(|| {
            let below_y = voxel.y().checked_sub(1)?;
            self.tree_at(VoxelCoord::new(voxel.x(), below_y, voxel.z()))
        })
    }

    /// Bounded trunk lookup for construction geometry; the sorted root index avoids scanning
    /// every distant tree in an expanding world.
    pub(crate) fn first_tree_trunk_in_bounds(
        &self,
        bounds: crate::spatial::VoxelBounds,
    ) -> Option<VoxelCoord> {
        let from = VoxelCoord::new(bounds.min().x(), i64::MIN, i64::MIN);
        let to = VoxelCoord::new(bounds.max_exclusive().x(), i64::MIN, i64::MIN);
        for tree in self.trees.range(from..to).map(|(_, tree)| tree) {
            let root = tree.root();
            if bounds.has_voxel(root) {
                return Some(root);
            }
            if let Some(y) = root.y().checked_add(1) {
                let upper = VoxelCoord::new(root.x(), y, root.z());
                if bounds.has_voxel(upper) {
                    return Some(upper);
                }
            }
        }
        None
    }

    /// Current cumulative external biosynthesis, including the mass still held by living trees.
    /// A projection avoids incrementing every tree on every 3.6-second simulation tick.
    pub(crate) fn assimilated_wood_at(
        &self,
        at: crate::core::time::SimulationTick,
    ) -> Option<crate::core::quantity::AggregateMass> {
        let mut total = self.assimilated_wood_milligrams;
        for tree in self.trees.values() {
            let gained = tree
                .wood_at(at)
                .milligrams()
                .checked_sub(tree.initial_wood().milligrams())?;
            total = total.checked_add(u128::from(gained))?;
        }
        Some(crate::core::quantity::AggregateMass::from_milligrams(total))
    }

    /// Move a juvenile's complete woody biomass without creating or consuming matter.
    /// The seed origin stays attached so future felling and trusted replay can audit it.
    pub(crate) fn transplant_tree(
        &mut self,
        from: VoxelCoord,
        to: VoxelCoord,
        expected_revision: u64,
        next_revision: u64,
        at: crate::core::time::SimulationTick,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !self.trees.contains_key(&to),
            "validated transplant destination occupied"
        );
        let tree = self
            .trees
            .remove(&from)
            .expect("validated sapling disappeared");
        assert!(self.trees.insert(to, tree.transplanted_to(to)).is_none());
        self.transplants.push(TransplantedTreeRecord::new(
            tree.origin_root(),
            from,
            to,
            at,
        ));
        self.revision = next_revision;
    }

    pub(crate) fn remove_felled_tree(
        &mut self,
        root: VoxelCoord,
        expected_revision: u64,
        next_revision: u64,
        at: crate::core::time::SimulationTick,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let tree = self
            .trees
            .remove(&root)
            .expect("validated standing tree disappeared");
        let gained = tree
            .wood_at(at)
            .milligrams()
            .checked_sub(tree.initial_wood().milligrams())
            .expect("tree biomass cannot shrink below its initial mass");
        self.assimilated_wood_milligrams = self
            .assimilated_wood_milligrams
            .checked_add(u128::from(gained))
            .expect("validated tree felling must retain external biosynthesis headroom");
        assert!(
            self.felled_trees
                .insert(tree.origin_root(), FelledTreeRecord::new(tree, at))
                .is_none(),
            "one generated tree may only be felled once"
        );
        self.revision = next_revision;
    }
}
