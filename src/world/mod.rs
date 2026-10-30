//! Persistent block-world definitions, compact chunk storage, generation, and trusted-load validation.

mod definitions;
mod excavation;
mod felling;
mod generation;
mod state;
mod targeting;
mod transplant;
mod trees;
mod validation;

pub(crate) use definitions::mixed_full_cube_mass;
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
pub use felling::{
    TreeFellingCommitError, TreeFellingError, TreeFellingOutcome, TreeFellingStartError,
    ValidatedTreeFellingStart, validate_start_player_tree_felling,
};
pub(crate) use felling::{
    apply_tree_felling_wear, decide_tree_felling_wear, tree_felling_condition_after,
    tree_felling_duration, tree_felling_exertion, validate_due_player_tree_felling,
};
pub(crate) use generation::WorldGenerationProfile;
pub(crate) use generation::first_buried_stationary_enclosure_in_world;
pub(crate) use generation::first_embodied_structure_collision_in_world;
pub(crate) use generation::first_solid_block_in_structural_bounds;
pub(crate) use generation::nearby_temple_residents;
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
pub use targeting::{PlayerBlockTarget, PlayerBlockTargetError, raycast_player_block};
pub(crate) use targeting::{raycast_player_air_voxel, raycast_player_air_voxel_with_reach};
pub use transplant::{
    SaplingTransplantCommitError, SaplingTransplantError, SaplingTransplantOutcome,
    ValidatedSaplingTransplant, validate_transplant_juvenile,
};
pub(crate) use trees::generated_tree;
pub use trees::{BIOLOGICAL_YEAR_TICKS, TreeRecord};
pub(crate) use trees::{FelledTreeRecord, TransplantedTreeRecord};
pub use validation::{WorldValidationError, validate_loaded_world};

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trees_tests.rs"]
mod trees_tests;

#[cfg(test)]
#[path = "transplant_tests.rs"]
mod transplant_tests;

#[cfg(test)]
#[path = "tree_chopping_tests.rs"]
mod tree_chopping_tests;

#[cfg(test)]
#[path = "tree_occupancy_tests.rs"]
mod tree_occupancy_tests;

#[cfg(test)]
#[path = "chunk_query_tests.rs"]
mod chunk_query_tests;

#[cfg(test)]
#[path = "temple_tests.rs"]
mod temple_tests;

#[cfg(test)]
#[path = "mineral_tests.rs"]
mod mineral_tests;

#[cfg(test)]
#[path = "mineral_grade_tests.rs"]
mod mineral_grade_tests;
