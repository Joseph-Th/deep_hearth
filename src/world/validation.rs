//! Trusted-load validation for compact world chunk storage.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;

use super::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldChunk, WorldRegistry,
    WorldSection, WorldState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldValidationError {
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
    Ok(())
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
