//! Compact persistent chunk/section storage for authoritative block states.

use std::collections::{BTreeMap, BTreeSet};

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

/// Palette-compressed 16-cubed block-state section.
///
/// A one-state section stores no index words. Multi-state sections use the minimum whole number
/// of bits needed for their local palette. The common homogeneous terrain case is therefore one
/// state ID plus a small header rather than 4,096 global IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSection {
    y: i64,
    palette: Vec<BlockStateId>,
    words: Vec<u64>,
    non_air_count: u16,
    represented_mass: Mass,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    mass_overrides: BTreeMap<u16, Mass>,
}

impl WorldSection {
    pub(crate) fn uniform(y: i64, state: BlockStateId, state_mass: Mass) -> Self {
        let represented_mass = if state == BlockStateId::AIR {
            assert_eq!(state_mass, Mass::ZERO, "air section must be massless");
            Mass::ZERO
        } else {
            let total = u128::from(state_mass.milligrams()) * SECTION_CELL_COUNT as u128;
            Mass::from_milligrams(
                u64::try_from(total)
                    .unwrap_or_else(|_| panic!("uniform section represented mass overflowed")),
            )
        };
        Self {
            y,
            palette: vec![state],
            words: Vec::new(),
            non_air_count: if state == BlockStateId::AIR {
                0
            } else {
                SECTION_CELL_COUNT as u16
            },
            represented_mass,
            mass_overrides: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn y(&self) -> i64 {
        self.y
    }

    #[must_use]
    pub fn palette(&self) -> &[BlockStateId] {
        &self.palette
    }

    #[must_use]
    pub const fn non_air_count(&self) -> u16 {
        self.non_air_count
    }

    /// Exact matter represented by all occupied cells in this section.
    #[must_use]
    pub const fn represented_mass(&self) -> Mass {
        self.represented_mass
    }

    #[must_use]
    pub fn packed_word_count(&self) -> usize {
        self.words.len()
    }

    #[must_use]
    pub(crate) fn state_at_index(&self, index: usize) -> Option<BlockStateId> {
        if index >= SECTION_CELL_COUNT {
            return None;
        }
        if self.palette.len() == 1 {
            return self.palette.first().copied();
        }
        let palette_index =
            read_packed_index(&self.words, bits_for_palette(self.palette.len()), index)?;
        self.palette.get(palette_index).copied()
    }

    #[must_use]
    pub(crate) fn mass_override_at_index(&self, index: usize) -> Option<Mass> {
        self.mass_overrides
            .get(&u16::try_from(index).ok()?)
            .copied()
    }

    pub(crate) fn set_mass_override_at_index(
        &mut self,
        index: usize,
        default_mass: Mass,
        remaining_mass: Mass,
    ) {
        assert!(
            index < SECTION_CELL_COUNT,
            "section cell index out of range"
        );
        assert!(
            !default_mass.is_zero(),
            "mass override requires material-backed cell"
        );
        assert!(
            !remaining_mass.is_zero() && remaining_mass <= default_mass,
            "cell remaining mass must stay within its positive default mass"
        );
        assert_ne!(
            self.state_at_index(index),
            Some(BlockStateId::AIR),
            "air cell cannot retain represented matter"
        );
        let key =
            u16::try_from(index).unwrap_or_else(|_| unreachable!("section cell index fits u16"));
        let previous = self
            .mass_overrides
            .get(&key)
            .copied()
            .unwrap_or(default_mass);
        if remaining_mass == default_mass {
            self.mass_overrides.remove(&key);
        } else {
            self.mass_overrides.insert(key, remaining_mass);
        }
        self.represented_mass = self
            .represented_mass
            .checked_sub(previous)
            .and_then(|mass| mass.checked_add(remaining_mass))
            .unwrap_or_else(|| panic!("section represented mass override overflowed"));
    }

    pub(crate) fn set_state_at_index(
        &mut self,
        index: usize,
        state: BlockStateId,
        previous_mass: Mass,
        new_mass: Mass,
    ) {
        assert!(
            index < SECTION_CELL_COUNT,
            "section cell index out of range"
        );
        let previous = self
            .state_at_index(index)
            .unwrap_or_else(|| panic!("trusted section index must resolve"));
        if previous == state {
            assert_eq!(
                previous_mass, new_mass,
                "unchanged block state changed represented mass"
            );
            return;
        }
        let key =
            u16::try_from(index).unwrap_or_else(|_| unreachable!("section cell index fits u16"));
        let represented_previous_mass = self.mass_overrides.remove(&key).unwrap_or(previous_mass);
        let palette_index = match self
            .palette
            .iter()
            .position(|candidate| *candidate == state)
        {
            Some(index) => index,
            None => {
                let old_bits = bits_for_palette(self.palette.len());
                self.palette.push(state);
                let new_bits = bits_for_palette(self.palette.len());
                if old_bits != new_bits {
                    self.words = repack_words(&self.words, old_bits, new_bits);
                }
                self.palette.len() - 1
            }
        };
        let bits = bits_for_palette(self.palette.len());
        if self.words.is_empty() {
            self.words = vec![0; packed_word_count(bits)];
        }
        write_packed_index(&mut self.words, bits, index, palette_index);
        if previous == BlockStateId::AIR {
            self.non_air_count = self
                .non_air_count
                .checked_add(1)
                .unwrap_or_else(|| panic!("section non-air count overflowed"));
        }
        if state == BlockStateId::AIR {
            self.non_air_count = self
                .non_air_count
                .checked_sub(1)
                .unwrap_or_else(|| panic!("section non-air count underflowed"));
        }
        self.represented_mass = self
            .represented_mass
            .checked_sub(represented_previous_mass)
            .and_then(|mass| mass.checked_add(new_mass))
            .unwrap_or_else(|| panic!("section represented mass mutation overflowed"));
    }

    pub(crate) fn canonicalize_palette(&mut self) {
        if self.palette.len() <= 1 {
            return;
        }
        let old_bits = bits_for_palette(self.palette.len());
        let mut used = vec![false; self.palette.len()];
        for cell in 0..SECTION_CELL_COUNT {
            let old_index = read_packed_index(&self.words, old_bits, cell)
                .unwrap_or_else(|| panic!("trusted section cell must resolve before compaction"));
            *used
                .get_mut(old_index)
                .unwrap_or_else(|| panic!("trusted section palette index must resolve")) = true;
        }
        if used.iter().all(|present| *present) {
            return;
        }
        // Keep the original palette order while assigning a dense index to each live state.
        // Scanning packed indices directly avoids a cell-by-palette quadratic search.
        let mut remap = vec![0; self.palette.len()];
        let mut palette = Vec::with_capacity(used.iter().filter(|present| **present).count());
        for (old_index, state) in self.palette.iter().copied().enumerate() {
            if used[old_index] {
                remap[old_index] = palette.len();
                palette.push(state);
            }
        }
        let bits = bits_for_palette(palette.len());
        let mut words = vec![0; packed_word_count(bits)];
        if bits > 0 {
            for cell in 0..SECTION_CELL_COUNT {
                let old_index =
                    read_packed_index(&self.words, old_bits, cell).unwrap_or_else(|| {
                        panic!("trusted section cell must resolve before compaction")
                    });
                write_packed_index(&mut words, bits, cell, remap[old_index]);
            }
        }
        self.palette = palette;
        self.words = words;
    }

    pub(crate) fn validate_storage(&self) -> Result<(), super::WorldValidationError> {
        use super::WorldValidationError;

        if self.palette.is_empty() {
            return Err(WorldValidationError::EmptySectionPalette { section_y: self.y });
        }
        if self.palette.len() > SECTION_CELL_COUNT {
            return Err(WorldValidationError::SectionPaletteTooLarge {
                section_y: self.y,
                entries: self.palette.len(),
            });
        }
        let mut seen = BTreeSet::new();
        for state in &self.palette {
            if !seen.insert(*state) {
                return Err(WorldValidationError::DuplicateSectionPaletteState {
                    section_y: self.y,
                    state: *state,
                });
            }
        }
        let bits = bits_for_palette(self.palette.len());
        let expected_words = packed_word_count(bits);
        if self.words.len() != expected_words {
            return Err(WorldValidationError::InvalidSectionWordCount {
                section_y: self.y,
                expected: expected_words,
                actual: self.words.len(),
            });
        }
        let mut counted_non_air = 0_u16;
        let mut used_palette_indexes = BTreeSet::new();
        for index in 0..SECTION_CELL_COUNT {
            let palette_index = if self.palette.len() == 1 {
                0
            } else {
                read_packed_index(&self.words, bits, index).unwrap_or_else(|| {
                    unreachable!("validated word count covers every section cell")
                })
            };
            let Some(state) = self.palette.get(palette_index) else {
                return Err(WorldValidationError::SectionPaletteIndexOutOfRange {
                    section_y: self.y,
                    cell: index,
                    palette_index,
                    palette_len: self.palette.len(),
                });
            };
            used_palette_indexes.insert(palette_index);
            if *state != BlockStateId::AIR {
                counted_non_air = counted_non_air
                    .checked_add(1)
                    .unwrap_or_else(|| unreachable!("section has at most 4096 non-air cells"));
            }
        }
        if counted_non_air != self.non_air_count {
            return Err(WorldValidationError::SectionNonAirCountMismatch {
                section_y: self.y,
                stored: self.non_air_count,
                actual: counted_non_air,
            });
        }
        if used_palette_indexes.len() != self.palette.len() {
            let unused_index = (0..self.palette.len())
                .find(|index| !used_palette_indexes.contains(index))
                .unwrap_or_else(|| unreachable!("palette usage proved an unused entry"));
            return Err(WorldValidationError::UnusedSectionPaletteState {
                section_y: self.y,
                state: self.palette[unused_index],
            });
        }
        for (&cell, &mass) in &self.mass_overrides {
            if usize::from(cell) >= SECTION_CELL_COUNT {
                return Err(WorldValidationError::SectionMassOverrideIndexOutOfRange {
                    section_y: self.y,
                    cell,
                });
            }
            if mass.is_zero() {
                return Err(WorldValidationError::ZeroSectionMassOverride {
                    section_y: self.y,
                    cell,
                });
            }
        }
        Ok(())
    }
}

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

// Preserve compact canonical arrays on disk, but use an ordered map for logarithmic
// insertion, felling, and trunk collision queries in the live simulation.
trait RootedRecord {
    fn root(&self) -> VoxelCoord;
}

impl RootedRecord for TreeRecord {
    fn root(&self) -> VoxelCoord {
        (*self).root()
    }
}

impl RootedRecord for FelledTreeRecord {
    fn root(&self) -> VoxelCoord {
        (*self).root()
    }
}

fn serialize_root_index<T: Serialize, S: Serializer>(
    index: &BTreeMap<VoxelCoord, T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(index.values())
}

fn deserialize_btree_map_no_duplicates_root_index<
    'de,
    T: Deserialize<'de> + RootedRecord,
    D: Deserializer<'de>,
>(
    deserializer: D,
) -> Result<BTreeMap<VoxelCoord, T>, D::Error> {
    let records = Vec::<T>::deserialize(deserializer)?;
    let mut index = BTreeMap::new();
    let mut previous = None;
    for record in records {
        let root = record.root();
        if previous.is_some_and(|previous| previous >= root) {
            return Err(serde::de::Error::custom(
                "tree records must have strictly increasing roots without duplicates",
            ));
        }
        index.insert(root, record);
        previous = Some(root);
    }
    Ok(index)
}

/// Authoritative generated block-world storage. Chunk absence means not generated/loaded as world
/// truth, while section absence inside a present chunk means canonical air.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldState {
    revision: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    generation_profile: Option<WorldGenerationProfile>,
    chunks: Vec<WorldChunk>,
    #[serde(
        serialize_with = "serialize_root_index",
        deserialize_with = "deserialize_btree_map_no_duplicates_root_index"
    )]
    trees: BTreeMap<VoxelCoord, TreeRecord>,
    /// Dry wood synthesized from the environment and already transferred out of trees.
    assimilated_wood_milligrams: u128,
    /// Seed-verifiable felling receipts account for absorbed carbon after timber changes owner.
    #[serde(
        serialize_with = "serialize_root_index",
        deserialize_with = "deserialize_btree_map_no_duplicates_root_index"
    )]
    felled_trees: BTreeMap<VoxelCoord, FelledTreeRecord>,
    /// Append-only authoritative path of seed-origin-preserving sapling relocations.
    transplants: Vec<TransplantedTreeRecord>,
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

    /// Standing trees are indexed by root location, not repeatedly scanned as block entities.
    pub fn trees(&self) -> impl Iterator<Item = &TreeRecord> {
        self.trees.values()
    }

    #[must_use]
    pub fn tree_at(&self, root: VoxelCoord) -> Option<&TreeRecord> {
        self.trees.get(&root)
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
                .insert(root, FelledTreeRecord::new(tree, at))
                .is_none(),
            "one generated tree may only be felled once"
        );
        self.revision = next_revision;
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
            self.tree_trunk_at(voxel).is_some()
                || block.collision() == BlockCollisionShape::FullCube,
        )
    }

    /// A floor-supported, two-block-high standing column in generated terrain.
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
        self.block_at(floor)
            .and_then(|state| registry.block_for_state(state))
            .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
            && self.block_at(position) == Some(BlockStateId::AIR)
            && self.block_at(head) == Some(BlockStateId::AIR)
            && self.tree_trunk_at(position).is_none()
            && self.tree_trunk_at(head).is_none()
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
                    self.trees.insert(tree.root(), tree).is_none(),
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

fn bits_for_palette(palette_len: usize) -> u8 {
    if palette_len <= 1 {
        0
    } else {
        usize::BITS as u8 - (palette_len - 1).leading_zeros() as u8
    }
}

fn packed_word_count(bits: u8) -> usize {
    if bits == 0 {
        0
    } else {
        (SECTION_CELL_COUNT * usize::from(bits)).div_ceil(u64::BITS as usize)
    }
}

fn read_packed_index(words: &[u64], bits: u8, index: usize) -> Option<usize> {
    if bits == 0 {
        return Some(0);
    }
    let bit_offset = index.checked_mul(usize::from(bits))?;
    let word_index = bit_offset / u64::BITS as usize;
    let shift = bit_offset % u64::BITS as usize;
    let mask = (1_u64 << bits) - 1;
    let first = *words.get(word_index)? >> shift;
    let spill = shift + usize::from(bits) > u64::BITS as usize;
    let value = if spill {
        let second = *words.get(word_index + 1)?;
        first | (second << (u64::BITS as usize - shift))
    } else {
        first
    };
    usize::try_from(value & mask).ok()
}

fn write_packed_index(words: &mut [u64], bits: u8, index: usize, value: usize) {
    assert!(bits > 0);
    let bit_offset = index * usize::from(bits);
    let word_index = bit_offset / u64::BITS as usize;
    let shift = bit_offset % u64::BITS as usize;
    let mask = (1_u64 << bits) - 1;
    let value = u64::try_from(value).unwrap_or_else(|_| panic!("palette index exceeds u64"));
    assert_eq!(value & !mask, 0, "palette index exceeds packed bit width");
    words[word_index] &= !(mask << shift);
    words[word_index] |= value << shift;
    if shift + usize::from(bits) > u64::BITS as usize {
        let spill_bits = shift + usize::from(bits) - u64::BITS as usize;
        let spill_mask = (1_u64 << spill_bits) - 1;
        words[word_index + 1] &= !spill_mask;
        words[word_index + 1] |= value >> (u64::BITS as usize - shift);
    }
}

fn repack_words(old_words: &[u64], old_bits: u8, new_bits: u8) -> Vec<u64> {
    let mut words = vec![0; packed_word_count(new_bits)];
    for index in 0..SECTION_CELL_COUNT {
        let value = if old_bits == 0 {
            0
        } else {
            read_packed_index(old_words, old_bits, index)
                .unwrap_or_else(|| panic!("trusted old packed section must cover every cell"))
        };
        write_packed_index(&mut words, new_bits, index, value);
    }
    words
}
