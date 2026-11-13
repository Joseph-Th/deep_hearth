//! Authoritative generated-world state, compact chunks and block-space queries.

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::quantity::Mass;
use crate::spatial::VoxelCoord;

use super::WorldRegistry;
#[cfg(test)]
use crate::core::state::AppState;

use super::{
    BlockCollisionShape, BlockStateId, FelledTreeRecord, TransplantedTreeRecord, TreeRecord,
    WorldGenerationProfile,
};

pub const CHUNK_SIDE: usize = 16;
pub const SECTION_SIDE: usize = 16;
pub const SECTION_CELL_COUNT: usize = SECTION_SIDE * SECTION_SIDE * SECTION_SIDE;
pub const WORLD_MIN_Y: i64 = -64;
pub const WORLD_MAX_Y_EXCLUSIVE: i64 = 320;

const CHUNK_SIDE_I64: i64 = CHUNK_SIDE as i64;
const SECTION_SIDE_I64: i64 = SECTION_SIDE as i64;

/// Horizontal identity of one generated chunk column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkCoord {
    x: i64,
    z: i64,
}

impl ChunkCoord {
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

    #[must_use]
    pub fn containing(voxel: VoxelCoord) -> Self {
        Self::new(
            voxel.x().div_euclid(CHUNK_SIDE_I64),
            voxel.z().div_euclid(CHUNK_SIDE_I64),
        )
    }

    #[must_use]
    pub fn world_min_x(self) -> Option<i64> {
        self.x.checked_mul(CHUNK_SIDE_I64)
    }

    #[must_use]
    pub fn world_min_z(self) -> Option<i64> {
        self.z.checked_mul(CHUNK_SIDE_I64)
    }

    /// Origin of a chunk whose entire 16-by-16 footprint fits physical voxel coordinates.
    /// Normal generation and trusted-load admission must accept the same coordinate range.
    pub(crate) fn bounded_world_origin(self) -> Option<(i64, i64)> {
        let min_x = self.world_min_x()?;
        let min_z = self.world_min_z()?;
        min_x.checked_add(CHUNK_SIDE_I64 - 1)?;
        min_z.checked_add(CHUNK_SIDE_I64 - 1)?;
        Some((min_x, min_z))
    }
}

#[cfg(test)]
pub(crate) fn set_generated_block_for_test(
    registry: &WorldRegistry,
    state: &mut AppState,
    voxel: VoxelCoord,
    block_state: BlockStateId,
) {
    let previous = state
        .world()
        .block_at(voxel)
        .unwrap_or_else(|| panic!("test block mutation requires generated in-range world truth"));
    let previous_mass = registry
        .represented_mass_for_state(previous)
        .unwrap_or_else(|| panic!("test previous block state must be registered"));
    let new_mass = registry
        .represented_mass_for_state(block_state)
        .unwrap_or_else(|| panic!("test target block state must be registered"));
    if previous == block_state {
        return;
    }
    let revision = state.world().revision();
    let next_revision = revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("test block mutation exhausted world revision"));
    let coord = ChunkCoord::containing(voxel);
    let world = state.world_state_mut();
    let index = world
        .chunks
        .binary_search_by_key(&coord, WorldChunk::coord)
        .unwrap_or_else(|_| panic!("test block mutation lost generated chunk"));
    world.chunks[index].set_block(voxel, block_state, previous_mass, new_mass);
    world.chunks[index].canonicalize_section_at(voxel);
    world.revision = next_revision;
}

mod section;
mod woodland;
pub use section::WorldSection;

/// One generated 16x16 chunk column. Missing sections inside a generated chunk are canonical air.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldChunk {
    coord: ChunkCoord,
    sections: Vec<WorldSection>,
}

impl WorldChunk {
    pub(crate) const fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            sections: Vec::new(),
        }
    }

    #[must_use]
    pub const fn coord(&self) -> ChunkCoord {
        self.coord
    }

    pub fn sections(&self) -> impl ExactSizeIterator<Item = &WorldSection> {
        self.sections.iter()
    }

    #[must_use]
    pub(crate) fn block_at(&self, voxel: VoxelCoord) -> BlockStateId {
        let section_y = section_y(voxel.y());
        let Ok(section_index) = self
            .sections
            .binary_search_by_key(&section_y, WorldSection::y)
        else {
            return BlockStateId::AIR;
        };
        self.sections[section_index]
            .state_at_index(local_index(voxel))
            .unwrap_or_else(|| unreachable!("voxel local index is always inside section"))
    }

    /// Check one generated sky column without a section lookup at every height.
    ///
    /// Missing sections are canonical air. Decode only populated sections at or above
    /// the observer, retaining individual block opacity (non-air is not always opaque).
    pub(crate) fn has_opaque_block_at_or_above(
        &self,
        registry: &WorldRegistry,
        voxel: VoxelCoord,
    ) -> bool {
        self.sections.iter().any(|section| {
            // Section Y is a signed section index, not a world-space altitude.
            let bottom = section.y() * SECTION_SIDE_I64;
            let top = bottom + SECTION_SIDE_I64;
            if section.non_air_count() == 0 || top <= voxel.y() {
                return false;
            }
            let first_y = bottom.max(voxel.y());
            (first_y..top).any(|y| {
                let at = VoxelCoord::new(voxel.x(), y, voxel.z());
                let block = section
                    .state_at_index(local_index(at))
                    .expect("local section index must be valid");
                registry
                    .block_for_state(block)
                    .is_none_or(|definition| definition.is_opaque())
            })
        })
    }

    pub(crate) fn set_block(
        &mut self,
        voxel: VoxelCoord,
        state: BlockStateId,
        previous_mass: Mass,
        new_mass: Mass,
    ) {
        let target_y = section_y(voxel.y());
        match self
            .sections
            .binary_search_by_key(&target_y, WorldSection::y)
        {
            Ok(index) => {
                self.sections[index].set_state_at_index(
                    local_index(voxel),
                    state,
                    previous_mass,
                    new_mass,
                );
                if self.sections[index].non_air_count() == 0 {
                    self.sections.remove(index);
                }
            }
            Err(index) if state != BlockStateId::AIR => {
                assert_eq!(
                    previous_mass,
                    Mass::ZERO,
                    "missing section must be canonical air"
                );
                let mut section = WorldSection::uniform(target_y, BlockStateId::AIR, Mass::ZERO);
                section.set_state_at_index(local_index(voxel), state, Mass::ZERO, new_mass);
                self.sections.insert(index, section);
            }
            Err(_) => {}
        }
    }

    pub(crate) fn canonicalize_sections(&mut self) {
        for section in &mut self.sections {
            section.canonicalize_palette();
        }
    }

    pub(crate) fn canonicalize_section_at(&mut self, voxel: VoxelCoord) {
        if let Ok(index) = self
            .sections
            .binary_search_by_key(&section_y(voxel.y()), WorldSection::y)
        {
            self.sections[index].canonicalize_palette();
        }
    }
}

// Preserve compact canonical arrays on disk, but index living trees by occupied root
// and felled trees by immutable seed origin. Different trees may occupy and later be
// felled at the same physical site; their biological histories must not collide.
trait TreeIndexRecord {
    fn index_key(&self) -> VoxelCoord;
}

/// A standing trunk is discovered by horizontal proximity. Keep X and Z
/// adjacent in its sole authoritative index so local sky and canopy probes
/// never scan distant north-south woodland with the same X coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TreeSpatialKey {
    x: i64,
    z: i64,
    y: i64,
}

impl TreeSpatialKey {
    const fn new(x: i64, z: i64, y: i64) -> Self {
        Self { x, z, y }
    }
}

impl From<VoxelCoord> for TreeSpatialKey {
    fn from(voxel: VoxelCoord) -> Self {
        Self::new(voxel.x(), voxel.z(), voxel.y())
    }
}

impl TreeIndexRecord for TreeRecord {
    fn index_key(&self) -> VoxelCoord {
        (*self).root()
    }
}

impl TreeIndexRecord for FelledTreeRecord {
    fn index_key(&self) -> VoxelCoord {
        (*self).origin_root()
    }
}

fn serialize_tree_index<K: Ord, T: Serialize, S: Serializer>(
    index: &BTreeMap<K, T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(index.values())
}

fn deserialize_btree_map_no_duplicates_tree_index<
    'de,
    K: Copy + Ord + From<VoxelCoord>,
    T: Deserialize<'de> + TreeIndexRecord,
    D: Deserializer<'de>,
>(
    deserializer: D,
) -> Result<BTreeMap<K, T>, D::Error> {
    let records = Vec::<T>::deserialize(deserializer)?;
    let mut index = BTreeMap::new();
    let mut previous = None;
    for record in records {
        let key = K::from(record.index_key());
        if previous.is_some_and(|previous| previous >= key) {
            return Err(serde::de::Error::custom(
                "tree records must have strictly increasing identities without duplicates",
            ));
        }
        index.insert(key, record);
        previous = Some(key);
    }
    Ok(index)
}

/// Authoritative generated block-world storage. Chunk absence means not generated/loaded as world
/// truth, while section absence inside a present chunk means canonical air.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldState {
    revision: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    generation_profile: Option<WorldGenerationProfile>,
    chunks: Vec<WorldChunk>,
    #[serde(
        serialize_with = "serialize_tree_index",
        deserialize_with = "deserialize_btree_map_no_duplicates_tree_index"
    )]
    trees: BTreeMap<TreeSpatialKey, TreeRecord>,
    /// Dry wood synthesized from the environment and already transferred out of trees.
    assimilated_wood_milligrams: u128,
    /// Seed-origin-indexed felling receipts account for absorbed carbon after timber changes owner.
    /// A location may appear in multiple receipts when different trees are planted there.
    #[serde(
        serialize_with = "serialize_tree_index",
        deserialize_with = "deserialize_btree_map_no_duplicates_tree_index"
    )]
    felled_trees: BTreeMap<VoxelCoord, FelledTreeRecord>,
    /// Append-only authoritative path of seed-origin-preserving sapling relocations.
    transplants: Vec<TransplantedTreeRecord>,
}

/// Public diagnostics must not reveal the deterministic generation seed. It
/// determines undiscovered resource placements and belongs to world-generation
/// authority, not to an actor-readable observation or incidental debug log.
impl Debug for WorldState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorldState")
            .field("revision", &self.revision)
            .field("generated_chunk_count", &self.chunks.len())
            .finish_non_exhaustive()
    }
}

impl WorldState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            generation_profile: None,
            chunks: Vec::new(),
            trees: BTreeMap::new(),
            assimilated_wood_milligrams: 0,
            felled_trees: BTreeMap::new(),
            transplants: Vec::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(crate) const fn generation_profile(&self) -> Option<WorldGenerationProfile> {
        self.generation_profile
    }

    pub fn chunks(&self) -> impl ExactSizeIterator<Item = &WorldChunk> {
        self.chunks.iter()
    }

    /// Generated columns intersecting a physical horizontal footprint, in canonical order.
    /// Chunk identity is sorted by x then z, so bound the x range by binary search
    /// rather than scanning distant columns for each structural construction request.
    pub(crate) fn chunks_intersecting_horizontal_bounds(
        &self,
        bounds: crate::spatial::VoxelBounds,
    ) -> impl Iterator<Item = &WorldChunk> {
        let first_x = bounds.min().x().div_euclid(CHUNK_SIDE_I64);
        let last_x = (bounds.max_exclusive().x() - 1).div_euclid(CHUNK_SIDE_I64);
        let first_z = bounds.min().z().div_euclid(CHUNK_SIDE_I64);
        let last_z = (bounds.max_exclusive().z() - 1).div_euclid(CHUNK_SIDE_I64);
        let from = self
            .chunks
            .partition_point(|chunk| chunk.coord().x() < first_x);
        let to = self
            .chunks
            .partition_point(|chunk| chunk.coord().x() <= last_x);
        self.chunks[from..to]
            .iter()
            .filter(move |chunk| (first_z..=last_z).contains(&chunk.coord().z()))
    }

    #[must_use]
    pub fn get_chunk(&self, coord: ChunkCoord) -> Option<&WorldChunk> {
        self.chunks
            .binary_search_by_key(&coord, WorldChunk::coord)
            .ok()
            .map(|index| &self.chunks[index])
    }

    /// Whether terrain has actually been generated for this horizontal
    /// location. The persisted generation profile identifies the world; it
    /// does not grant physical terrain authority to distant missing chunks.
    #[must_use]
    pub(crate) fn has_generated_terrain_at(&self, position: VoxelCoord) -> bool {
        self.get_chunk(ChunkCoord::containing(position)).is_some()
    }

    /// Returns a block state only when the containing chunk is generated and the coordinate lies
    /// within the supported vertical world range.
    #[must_use]
    pub fn block_at(&self, voxel: VoxelCoord) -> Option<BlockStateId> {
        if !(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y()) {
            return None;
        }
        self.get_chunk(ChunkCoord::containing(voxel))
            .map(|chunk| chunk.block_at(voxel))
    }

    /// Physical full-cell collision owned by the generated world. Living tree
    /// trunks are solid even though their terrain block is air. None preserves
    /// the distinction between unknown world truth and a known clear voxel:
    /// callers decide whether an ungenerated cell may admit custody or movement.
    #[must_use]
    pub(crate) fn full_cell_collision_at(
        &self,
        registry: &WorldRegistry,
        voxel: VoxelCoord,
    ) -> Option<bool> {
        let block = registry.block_for_state(self.block_at(voxel)?)?;
        Some(
            block.collision() == BlockCollisionShape::FullCube
                || self.tree_trunk_at(voxel).is_some(),
        )
    }

    /// A collision-supported, two-block-high standing column in generated terrain.
    /// Solid living trunks provide floor contact just like full terrain blocks.
    /// Missing chunks and out-of-range floor/head cells are never traversable ground.
    #[must_use]
    pub(crate) fn has_standing_clearance(
        &self,
        registry: &WorldRegistry,
        position: VoxelCoord,
    ) -> bool {
        let (Some(floor_y), Some(head_y)) =
            (position.y().checked_sub(1), position.y().checked_add(1))
        else {
            return false;
        };
        let floor = VoxelCoord::new(position.x(), floor_y, position.z());
        let head = VoxelCoord::new(position.x(), head_y, position.z());
        self.full_cell_collision_at(registry, floor) == Some(true)
            && self.block_at(position) == Some(BlockStateId::AIR)
            && self.block_at(head) == Some(BlockStateId::AIR)
            && self.tree_trunk_at(position).is_none()
            && self.tree_trunk_at(head).is_none()
    }

    /// Permanent rooted woodland needs a full terrain block, not temporary
    /// support from another living trunk. Player footing intentionally permits
    /// either support, but a trunk cannot outlive the tree beneath its roots.
    pub(crate) fn has_tree_root_terrain_floor(
        &self,
        registry: &WorldRegistry,
        root: VoxelCoord,
    ) -> bool {
        root.y().checked_sub(1).is_some_and(|floor_y| {
            self.block_at(VoxelCoord::new(root.x(), floor_y, root.z()))
                .and_then(|block| registry.block_for_state(block))
                .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
    }

    pub(crate) fn represented_mass_at(
        &self,
        registry: &WorldRegistry,
        voxel: VoxelCoord,
    ) -> Option<Mass> {
        let state = self.block_at(voxel)?;
        let default_mass = registry.represented_mass_for_state(state)?;
        if state == BlockStateId::AIR {
            return Some(Mass::ZERO);
        }
        let chunk = self.get_chunk(ChunkCoord::containing(voxel))?;
        let target_section_y = section_y(voxel.y());
        let Ok(section_index) = chunk
            .sections
            .binary_search_by_key(&target_section_y, WorldSection::y)
        else {
            return Some(default_mass);
        };
        Some(
            chunk.sections[section_index]
                .mass_override_at_index(local_index(voxel))
                .unwrap_or(default_mass),
        )
    }

    pub(crate) fn insert_generated_chunks(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        profile: WorldGenerationProfile,
        chunks: Vec<WorldChunk>,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.generation_profile
                .is_none_or(|existing| existing == profile),
            "generated chunk insertion cannot change persistent world generation identity"
        );
        self.generation_profile = Some(profile);
        for chunk in chunks {
            let tree = super::generated_tree(profile, &chunk);
            let coord = chunk.coord();
            let index = self
                .chunks
                .binary_search_by_key(&coord, WorldChunk::coord)
                .unwrap_or_else(|index| index);
            assert!(
                self.chunks
                    .get(index)
                    .is_none_or(|existing| existing.coord() != coord),
                "generated chunk insertion cannot replace existing world truth"
            );
            self.chunks.insert(index, chunk);
            if let Some(tree) = tree {
                assert!(
                    self.trees
                        .insert(TreeSpatialKey::from(tree.root()), tree)
                        .is_none(),
                    "generated chunk cannot replace an existing tree"
                );
            }
        }
        self.revision = next_revision;
    }

    pub(crate) fn apply_excavated_cell_mass(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        voxel: VoxelCoord,
        expected_state: BlockStateId,
        default_mass: Mass,
        remaining_mass: Mass,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let coord = ChunkCoord::containing(voxel);
        let chunk_index = self
            .chunks
            .binary_search_by_key(&coord, WorldChunk::coord)
            .unwrap_or_else(|_| panic!("mass override requires generated world truth"));
        let target_section_y = section_y(voxel.y());
        let section_index = self.chunks[chunk_index]
            .sections
            .binary_search_by_key(&target_section_y, WorldSection::y)
            .unwrap_or_else(|_| panic!("mass override requires occupied section truth"));
        assert_eq!(
            self.chunks[chunk_index].sections[section_index].state_at_index(local_index(voxel)),
            Some(expected_state),
            "excavation target state changed after validation"
        );
        if remaining_mass.is_zero() {
            self.chunks[chunk_index].set_block(voxel, BlockStateId::AIR, default_mass, Mass::ZERO);
            self.chunks[chunk_index].canonicalize_section_at(voxel);
        } else {
            self.chunks[chunk_index].sections[section_index].set_mass_override_at_index(
                local_index(voxel),
                default_mass,
                remaining_mass,
            );
        }
        self.revision = next_revision;
    }
}

fn section_y(y: i64) -> i64 {
    y.div_euclid(SECTION_SIDE_I64)
}

fn local_index(voxel: VoxelCoord) -> usize {
    let x = voxel.x().rem_euclid(SECTION_SIDE_I64) as usize;
    let y = voxel.y().rem_euclid(SECTION_SIDE_I64) as usize;
    let z = voxel.z().rem_euclid(SECTION_SIDE_I64) as usize;
    (y * SECTION_SIDE + z) * SECTION_SIDE + x
}
