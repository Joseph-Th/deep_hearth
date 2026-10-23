//! Persistent block-world definitions, compact chunk storage, generation, and trusted-load validation.

mod definitions;
mod excavation;
mod generation;
mod state;
mod targeting;
mod validation;

pub use definitions::{
    BlockCollisionShape, BlockDefinition, BlockDefinitionId, BlockExcavationDefinition,
    BlockPhysicalDefinition, BlockStateDefinition, BlockStateId, WorldRegistry,
};
pub(crate) use excavation::{
    BlockExcavationCancellationPlan, BlockExcavationTickError, apply_block_excavation_cancellation,
    apply_block_excavation_tick, decide_block_excavation_cancellation,
    decide_block_excavation_completion, decide_block_excavation_tick,
    validate_start_player_block_excavation_active,
};
pub use excavation::{
    BlockExcavationCommitError, BlockExcavationError, BlockExcavationOutcome,
    BlockExcavationOutputReceipt, BlockExcavationRequest, BlockExcavationStartOutcome,
    ValidatedPlayerBlockExcavationStart, validate_start_player_block_excavation,
};
pub(crate) use generation::WorldGenerationProfile;
pub(crate) use generation::first_buried_stationary_enclosure_in_world;
pub(crate) use generation::first_embodied_structure_collision_in_world;
pub(crate) use generation::first_solid_block_in_structural_bounds;
pub(crate) use generation::temple_residents_at;
pub(crate) use generation::temple_warden_route_at;
pub use generation::{
    ChunkGenerationCommitError, ChunkGenerationError, ValidatedChunkGeneration,
    ValidatedPlayerChunkDemand, WorldGenerationSeed, validate_generate_chunk,
    validate_generate_player_neighborhood,
};
#[cfg(test)]
pub(crate) use state::set_generated_block_for_test;
pub use state::{
    CHUNK_SIDE, ChunkCoord, SECTION_CELL_COUNT, SECTION_SIDE, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y,
    WorldChunk, WorldSection, WorldState,
};
pub(crate) use targeting::raycast_player_air_voxel;
pub use targeting::{PlayerBlockTarget, PlayerBlockTargetError, raycast_player_block};
pub use validation::{WorldValidationError, validate_loaded_world};

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "temple_tests.rs"]
mod temple_tests;
