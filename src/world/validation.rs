//! Trusted-load validation for compact world chunk storage.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;

use super::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldChunk, WorldRegistry,
    WorldSection, WorldState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldValidationError {
    InvalidTreeTransplant {
        origin: crate::spatial::VoxelCoord,
    },
    InvalidStandingTree {
        root: crate::spatial::VoxelCoord,
    },
    InvalidFelledTree {
        root: crate::spatial::VoxelCoord,
    },
    InvalidPhotosyntheticAccounting,
    GenerationProfileWithoutChunks,
    ChunksWithoutGenerationProfile,
    UnknownGenerationSolidState {
        state: BlockStateId,
    },
    AirGenerationSolidState,
    EmptyStateWithNonzeroRevision {
        revision: u64,
    },
    PopulatedStateWithZeroRevision,
    ChunksNotStrictlyOrdered {
        previous: ChunkCoord,
        current: ChunkCoord,
    },
    ChunkCoordinateOverflow {
        chunk: ChunkCoord,
    },
    SectionsNotStrictlyOrdered {
        chunk: ChunkCoord,
        previous_y: i64,
        current_y: i64,
    },
    SectionOutsideWorldBounds {
        chunk: ChunkCoord,
        section_y: i64,
    },
    EmptySectionPalette {
        section_y: i64,
    },
    SectionPaletteTooLarge {
        section_y: i64,
        entries: usize,
    },
    DuplicateSectionPaletteState {
        section_y: i64,
        state: BlockStateId,
    },
    UnusedSectionPaletteState {
        section_y: i64,
        state: BlockStateId,
    },
    UnknownSectionPaletteState {
        chunk: ChunkCoord,
        section_y: i64,
        state: BlockStateId,
    },
    InvalidSectionWordCount {
        section_y: i64,
        expected: usize,
        actual: usize,
    },
    SectionPaletteIndexOutOfRange {
        section_y: i64,
        cell: usize,
        palette_index: usize,
        palette_len: usize,
    },
    SectionNonAirCountMismatch {
        section_y: i64,
        stored: u16,
        actual: u16,
    },
    SectionMassOverrideIndexOutOfRange {
        section_y: i64,
        cell: u16,
    },
    ZeroSectionMassOverride {
        section_y: i64,
        cell: u16,
    },
    SectionMassOverrideOnAir {
        chunk: ChunkCoord,
        section_y: i64,
        cell: u16,
    },
    SectionMassOverrideNotBelowDefault {
        chunk: ChunkCoord,
        section_y: i64,
        cell: u16,
        stored: Mass,
        default: Mass,
    },
    SectionMassOverflow {
        chunk: ChunkCoord,
        section_y: i64,
    },
    SectionMassMismatch {
        chunk: ChunkCoord,
        section_y: i64,
        stored: Mass,
        actual: Mass,
    },
    EmptyStoredSection {
        chunk: ChunkCoord,
        section_y: i64,
    },
}

impl Display for WorldValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTreeTransplant { origin } => write!(
                formatter,
                "tree with origin ({},{},{}) has an invalid transplant path",
                origin.x(),
                origin.y(),
                origin.z()
            ),
            Self::InvalidStandingTree { root } => write!(
                formatter,
                "standing tree at ({},{},{}) is not a valid supported, unburied world tree",
                root.x(),
                root.y(),
                root.z()
            ),
            Self::InvalidFelledTree { root } => write!(
                formatter,
                "felled tree at ({},{},{}) has invalid seed provenance or duplicate history",
                root.x(),
                root.y(),
                root.z()
            ),
            Self::InvalidPhotosyntheticAccounting => formatter.write_str(
                "world photosynthetic input is inconsistent with represented physical matter",
            ),
            Self::GenerationProfileWithoutChunks => formatter
                .write_str("world persists a terrain generation profile without generated chunks"),
            Self::ChunksWithoutGenerationProfile => formatter.write_str(
                "world persists generated chunks without their terrain generation profile",
            ),
            Self::UnknownGenerationSolidState { state } => write!(
                formatter,
                "world generation profile references unknown block state {}",
                state.value()
            ),
            Self::AirGenerationSolidState => formatter
                .write_str("world generation profile cannot use air as its terrain solid state"),
            Self::EmptyStateWithNonzeroRevision { revision } => write!(
                formatter,
                "empty world state has nonzero revision {revision}"
            ),
            Self::PopulatedStateWithZeroRevision => {
                formatter.write_str("populated world state has zero revision")
            }
            Self::ChunksNotStrictlyOrdered { previous, current } => write!(
                formatter,
                "world chunks are not strictly ordered: ({},{}) then ({},{})",
                previous.x(),
                previous.z(),
                current.x(),
                current.z()
            ),
            Self::ChunkCoordinateOverflow { chunk } => write!(
                formatter,
                "world chunk ({},{}) extends beyond representable voxel coordinates",
                chunk.x(),
                chunk.z()
            ),
            Self::SectionsNotStrictlyOrdered {
                chunk,
                previous_y,
                current_y,
            } => write!(
                formatter,
                "chunk ({},{}) sections are not strictly ordered: {previous_y} then {current_y}",
                chunk.x(),
                chunk.z()
            ),
            Self::SectionOutsideWorldBounds { chunk, section_y } => write!(
                formatter,
                "chunk ({},{}) contains section {section_y} outside supported world height",
                chunk.x(),
                chunk.z()
            ),
            Self::EmptySectionPalette { section_y } => {
                write!(
                    formatter,
                    "world section {section_y} has an empty block-state palette"
                )
            }
            Self::SectionPaletteTooLarge { section_y, entries } => write!(
                formatter,
                "world section {section_y} palette has {entries} entries, exceeding its cell count"
            ),
            Self::DuplicateSectionPaletteState { section_y, state } => write!(
                formatter,
                "world section {section_y} palette repeats block state {}",
                state.value()
            ),
            Self::UnusedSectionPaletteState { section_y, state } => write!(
                formatter,
                "world section {section_y} palette contains unused block state {}",
                state.value()
            ),
            Self::UnknownSectionPaletteState {
                chunk,
                section_y,
                state,
            } => write!(
                formatter,
                "chunk ({},{}) section {section_y} references unknown block state {}",
                chunk.x(),
                chunk.z(),
                state.value()
            ),
            Self::InvalidSectionWordCount {
                section_y,
                expected,
                actual,
            } => write!(
                formatter,
                "world section {section_y} stores {actual} packed words; canonical palette width requires {expected}"
            ),
            Self::SectionPaletteIndexOutOfRange {
                section_y,
                cell,
                palette_index,
                palette_len,
            } => write!(
                formatter,
                "world section {section_y} cell {cell} references palette index {palette_index} outside {palette_len} entries"
            ),
            Self::SectionNonAirCountMismatch {
                section_y,
                stored,
                actual,
            } => write!(
                formatter,
                "world section {section_y} stores non-air count {stored} but contains {actual} non-air cells"
            ),
            Self::SectionMassOverrideIndexOutOfRange { section_y, cell } => write!(
                formatter,
                "world section {section_y} stores a mass override for out-of-range cell {cell}"
            ),
            Self::ZeroSectionMassOverride { section_y, cell } => write!(
                formatter,
                "world section {section_y} stores a zero-mass override for cell {cell}"
            ),
            Self::SectionMassOverrideOnAir {
                chunk,
                section_y,
                cell,
            } => write!(
                formatter,
                "chunk ({},{}) section {section_y} stores represented mass for air cell {cell}",
                chunk.x(),
                chunk.z()
            ),
            Self::SectionMassOverrideNotBelowDefault {
                chunk,
                section_y,
                cell,
                stored,
                default,
            } => write!(
                formatter,
                "chunk ({},{}) section {section_y} cell {cell} stores exceptional mass {} mg that is not below default {} mg",
                chunk.x(),
                chunk.z(),
                stored.milligrams(),
                default.milligrams()
            ),
            Self::SectionMassOverflow { chunk, section_y } => write!(
                formatter,
                "chunk ({},{}) section {section_y} represented matter exceeds mass range",
                chunk.x(),
                chunk.z()
            ),
            Self::SectionMassMismatch {
                chunk,
                section_y,
                stored,
                actual,
            } => write!(
                formatter,
                "chunk ({},{}) section {section_y} stores represented mass {} mg but its cells require {} mg",
                chunk.x(),
                chunk.z(),
                stored.milligrams(),
                actual.milligrams()
            ),
            Self::EmptyStoredSection { chunk, section_y } => write!(
                formatter,
                "chunk ({},{}) persists empty section {section_y} instead of canonical section absence",
                chunk.x(),
                chunk.z()
            ),
        }
    }
}

impl Error for WorldValidationError {}

pub fn validate_loaded_world(
    registry: &WorldRegistry,
    state: &WorldState,
) -> Result<(), WorldValidationError> {
    validate_generation_profile(registry, state)?;
    validate_revision(state)?;
    let mut previous_chunk = None;
    for chunk in state.chunks() {
        validate_chunk_order(previous_chunk, chunk.coord())?;
        previous_chunk = Some(chunk.coord());
        validate_chunk(registry, chunk)?;
    }
    let transplant_paths = validate_transplant_paths(state)?;
    let mut previous_tree = None;
    let mut occupied_seed_origins = BTreeSet::new();
    for tree in state.trees() {
        let root = tree.root();
        let origin = tree.origin_root();
        let valid = state
            .generation_profile()
            .and_then(|profile| {
                super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
            })
            .map(|original| original.transplanted_to(root))
            == Some(*tree)
            && transplant_paths
                .get(&origin)
                .is_none_or(|(last, _)| *last == root)
            && (transplant_paths.contains_key(&origin) || root == origin)
            && state.get_chunk(ChunkCoord::containing(origin)).is_some()
            && occupied_seed_origins.insert(origin)
            && previous_tree.is_none_or(|previous| previous < root)
            && tree.initial_wood().milligrams() > 0
            && tree.initial_wood().milligrams() <= 1_600_000_000
            && root.y().checked_sub(1).is_some_and(|floor| {
                state
                    .block_at(crate::spatial::VoxelCoord::new(root.x(), floor, root.z()))
                    .and_then(|block| registry.block_for_state(block))
                    .is_some_and(|block| block.collision() == super::BlockCollisionShape::FullCube)
            })
            && state.block_at(root) == Some(BlockStateId::AIR)
            && root.y().checked_add(1).is_some_and(|head| {
                state.block_at(crate::spatial::VoxelCoord::new(root.x(), head, root.z()))
                    == Some(BlockStateId::AIR)
            });
        if !valid {
            return Err(WorldValidationError::InvalidStandingTree { root });
        }
        previous_tree = Some(root);
    }
    // The cumulative biomass-input ledger is a cache of exactly reproducible felled-tree
    // records. Decode cannot forge atmospheric mass by tweaking an unaudited total.
    let mut previous_felled = None;
    let mut verified_input = 0u128;
    for record in state.felled_trees() {
        let root = record.root();
        let origin = record.origin_root();
        let Some(original) = state.generation_profile().and_then(|profile| {
            super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
        }) else {
            return Err(WorldValidationError::InvalidFelledTree { root });
        };
        if original.root() != origin
            || transplant_paths
                .get(&origin)
                .is_some_and(|(last, at)| *last != root || *at > record.felled_at())
            || (!transplant_paths.contains_key(&origin) && root != origin)
            || !occupied_seed_origins.insert(origin)
            || state.tree_at(root).is_some()
            || state.get_chunk(ChunkCoord::containing(root)).is_none()
            || previous_felled.is_some_and(|previous| previous >= root)
        {
            return Err(WorldValidationError::InvalidFelledTree { root });
        }
        let grown = original
            .wood_at(record.felled_at())
            .milligrams()
            .checked_sub(original.initial_wood().milligrams())
            .ok_or(WorldValidationError::InvalidFelledTree { root })?;
        verified_input = verified_input
            .checked_add(u128::from(grown))
            .ok_or(WorldValidationError::InvalidPhotosyntheticAccounting)?;
        previous_felled = Some(root);
    }
    for origin in transplant_paths.keys() {
        if !occupied_seed_origins.contains(origin) {
            return Err(WorldValidationError::InvalidTreeTransplant { origin: *origin });
        }
    }
    if verified_input != state.committed_assimilated_wood_milligrams() {
        return Err(WorldValidationError::InvalidPhotosyntheticAccounting);
    }
    Ok(())
}

/// Replay immutable transplants from each seed-derived root. Current coordinates alone
/// cannot authorize a decoded move or felling receipt.
fn validate_transplant_paths(
    state: &WorldState,
) -> Result<
    BTreeMap<
        crate::spatial::VoxelCoord,
        (
            crate::spatial::VoxelCoord,
            crate::core::time::SimulationTick,
        ),
    >,
    WorldValidationError,
> {
    let mut paths = BTreeMap::new();
    let mut previous_tick = None;
    for record in state.transplants() {
        let origin = record.origin();
        let from = record.from();
        let to = record.to();
        let at = record.transplanted_at();
        let original = state.generation_profile().and_then(|profile| {
            super::trees::seeded_tree_identity(profile, ChunkCoord::containing(origin))
        });
        let valid = original.is_some_and(|tree| {
            tree.root() == origin && tree.wood_at(at).milligrams() < 10_000_000
        }) && previous_tick.is_none_or(|previous| previous <= at)
            && from.y() == to.y()
            && from
                .x()
                .abs_diff(to.x())
                .checked_add(from.z().abs_diff(to.z()))
                == Some(2)
            && state.get_chunk(ChunkCoord::containing(origin)).is_some()
            && state.get_chunk(ChunkCoord::containing(from)).is_some()
            && state.get_chunk(ChunkCoord::containing(to)).is_some()
            && paths.get(&origin).is_none_or(|(last, _)| *last == from)
            && (paths.contains_key(&origin) || from == origin);
        if !valid {
            return Err(WorldValidationError::InvalidTreeTransplant { origin });
        }
        paths.insert(origin, (to, at));
        previous_tick = Some(at);
    }
    Ok(paths)
}

fn validate_generation_profile(
    registry: &WorldRegistry,
    state: &WorldState,
) -> Result<(), WorldValidationError> {
    let chunk_count = state.chunks().len();
    let Some(profile) = state.generation_profile() else {
        return if chunk_count == 0 {
            Ok(())
        } else {
            Err(WorldValidationError::ChunksWithoutGenerationProfile)
        };
    };
    if chunk_count == 0 {
        return Err(WorldValidationError::GenerationProfileWithoutChunks);
    }
    if profile.solid_state() == BlockStateId::AIR {
        return Err(WorldValidationError::AirGenerationSolidState);
    }
    if registry.get_state(profile.solid_state()).is_none() {
        return Err(WorldValidationError::UnknownGenerationSolidState {
            state: profile.solid_state(),
        });
    }
    Ok(())
}

fn validate_revision(state: &WorldState) -> Result<(), WorldValidationError> {
    match (state.chunks().len(), state.revision()) {
        (0, revision) if revision != 0 => {
            Err(WorldValidationError::EmptyStateWithNonzeroRevision { revision })
        }
        (count, 0) if count != 0 => Err(WorldValidationError::PopulatedStateWithZeroRevision),
        _ => Ok(()),
    }
}

fn validate_chunk_order(
    previous: Option<ChunkCoord>,
    current: ChunkCoord,
) -> Result<(), WorldValidationError> {
    if previous.is_some_and(|previous| previous >= current) {
        return Err(WorldValidationError::ChunksNotStrictlyOrdered {
            previous: previous.unwrap_or_else(|| unreachable!("checked previous chunk exists")),
            current,
        });
    }
    Ok(())
}

fn validate_chunk(
    registry: &WorldRegistry,
    chunk: &WorldChunk,
) -> Result<(), WorldValidationError> {
    if chunk.coord().bounded_world_origin().is_none() {
        return Err(WorldValidationError::ChunkCoordinateOverflow {
            chunk: chunk.coord(),
        });
    }
    let minimum_section = WORLD_MIN_Y.div_euclid(super::SECTION_SIDE as i64);
    let maximum_section_exclusive = WORLD_MAX_Y_EXCLUSIVE.div_euclid(super::SECTION_SIDE as i64);
    let mut previous_section = None;
    for section in chunk.sections() {
        if let Some(previous_y) = previous_section
            && previous_y >= section.y()
        {
            return Err(WorldValidationError::SectionsNotStrictlyOrdered {
                chunk: chunk.coord(),
                previous_y,
                current_y: section.y(),
            });
        }
        previous_section = Some(section.y());
        if section.y() < minimum_section || section.y() >= maximum_section_exclusive {
            return Err(WorldValidationError::SectionOutsideWorldBounds {
                chunk: chunk.coord(),
                section_y: section.y(),
            });
        }
        validate_section(registry, chunk.coord(), section)?;
    }
    Ok(())
}

fn validate_section(
    registry: &WorldRegistry,
    chunk: ChunkCoord,
    section: &WorldSection,
) -> Result<(), WorldValidationError> {
    section.validate_storage()?;
    if section.non_air_count() == 0 {
        return Err(WorldValidationError::EmptyStoredSection {
            chunk,
            section_y: section.y(),
        });
    }
    for state in section.palette() {
        if registry.get_state(*state).is_none() {
            return Err(WorldValidationError::UnknownSectionPaletteState {
                chunk,
                section_y: section.y(),
                state: *state,
            });
        }
    }
    validate_section_mass(registry, chunk, section)
}

fn validate_section_mass(
    registry: &WorldRegistry,
    chunk: ChunkCoord,
    section: &WorldSection,
) -> Result<(), WorldValidationError> {
    // A one-state section with no partial excavation represents 4,096 identical
    // cells. Avoid thousands of redundant palette and registry lookups per load.
    if let Some(state) = section.uniform_state_without_mass_overrides() {
        let per_cell = registry
            .represented_mass_for_state(state)
            .unwrap_or_else(|| unreachable!("validated block state has a block definition"));
        let total = u128::from(per_cell.milligrams()) * super::SECTION_CELL_COUNT as u128;
        let expected = u64::try_from(total)
            .map(Mass::from_milligrams)
            .map_err(|_| WorldValidationError::SectionMassOverflow {
                chunk,
                section_y: section.y(),
            })?;
        return validate_section_mass_total(chunk, section, expected);
    }
    let mut expected = Mass::ZERO;
    for cell in 0..super::SECTION_CELL_COUNT {
        let state = section
            .state_at_index(cell)
            .unwrap_or_else(|| unreachable!("validated section resolves every cell"));
        let default_mass = registry
            .represented_mass_for_state(state)
            .unwrap_or_else(|| unreachable!("validated block state has a block definition"));
        let mass = match section.mass_override_at_index(cell) {
            Some(_) if state == BlockStateId::AIR => {
                return Err(WorldValidationError::SectionMassOverrideOnAir {
                    chunk,
                    section_y: section.y(),
                    cell: u16::try_from(cell)
                        .unwrap_or_else(|_| unreachable!("section cell index fits u16")),
                });
            }
            Some(stored) if stored >= default_mass => {
                return Err(WorldValidationError::SectionMassOverrideNotBelowDefault {
                    chunk,
                    section_y: section.y(),
                    cell: u16::try_from(cell)
                        .unwrap_or_else(|_| unreachable!("section cell index fits u16")),
                    stored,
                    default: default_mass,
                });
            }
            Some(stored) => stored,
            None => default_mass,
        };
        expected = expected
            .checked_add(mass)
            .ok_or(WorldValidationError::SectionMassOverflow {
                chunk,
                section_y: section.y(),
            })?;
    }
    validate_section_mass_total(chunk, section, expected)
}

fn validate_section_mass_total(
    chunk: ChunkCoord,
    section: &WorldSection,
    expected: Mass,
) -> Result<(), WorldValidationError> {
    if expected != section.represented_mass() {
        return Err(WorldValidationError::SectionMassMismatch {
            chunk,
            section_y: section.y(),
            stored: section.represented_mass(),
            actual: expected,
        });
    }
    Ok(())
}
