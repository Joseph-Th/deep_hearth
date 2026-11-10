//! Trusted-load validation for compact world chunk storage.

use crate::core::quantity::Mass;

use super::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldChunk, WorldRegistry,
    WorldSection, WorldState,
};

mod error;
mod woodland;

pub use error::WorldValidationError;
use woodland::validate_loaded_woodland;

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
    validate_loaded_woodland(registry, state)
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
    if !registry.is_material_backed_full_cube_state(profile.solid_state()) {
        return Err(WorldValidationError::InvalidGenerationSubstrate {
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
    for &state in section.palette() {
        if registry.get_state(state).is_none() {
            return Err(WorldValidationError::UnknownSectionPaletteState {
                chunk,
                section_y: section.y(),
                state,
            });
        }
    }
    // Almost every deep generated section is a single physical block state.
    // Keep its validation free of both cell iteration and a palette allocation.
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
    // A mixed section has only a handful of unique block identities. Resolve
    // their mass once, then use packed palette indices for every individual cell.
    let palette_masses = section
        .palette()
        .iter()
        .map(|&state| {
            registry
                .represented_mass_for_state(state)
                .unwrap_or_else(|| unreachable!("validated block state has a block definition"))
        })
        .collect::<Vec<_>>();
    validate_section_mass(chunk, section, &palette_masses)
}

fn validate_section_mass(
    chunk: ChunkCoord,
    section: &WorldSection,
    palette_masses: &[Mass],
) -> Result<(), WorldValidationError> {
    let mut expected = Mass::ZERO;
    for cell in 0..super::SECTION_CELL_COUNT {
        let palette_index = section
            .palette_index_at_index(cell)
            .unwrap_or_else(|| unreachable!("validated section resolves every cell"));
        let state = section.palette()[palette_index];
        let default_mass = palette_masses[palette_index];
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
