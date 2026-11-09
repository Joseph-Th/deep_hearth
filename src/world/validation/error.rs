//! Typed trusted-load failures for generated world and woodland records.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;

use super::super::{BlockStateId, ChunkCoord};

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
    InvalidGenerationSubstrate {
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
            Self::InvalidGenerationSubstrate { state } => write!(
                formatter,
                "world generation profile uses non-colliding or massless substrate block state {}",
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
