//! Persistent block-world definitions, compact chunk storage, generation, and trusted-load validation.

mod definitions;
mod generation;
mod state;
mod validation;

pub use definitions::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockPhysicalDefinition,
    BlockStateDefinition, BlockStateId, WorldRegistry,
};
pub use generation::{
    ChunkGenerationCommitError, ChunkGenerationError, ValidatedChunkGeneration,
    WorldGenerationSeed, validate_generate_chunk,
};
pub use state::{
    CHUNK_SIDE, ChunkCoord, SECTION_CELL_COUNT, SECTION_SIDE, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y,
    WorldChunk, WorldSection, WorldState,
};
pub use validation::{WorldValidationError, validate_loaded_world};

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
