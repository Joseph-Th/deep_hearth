//! Renderer-neutral block-world meshing.
//!
//! Meshing derives surface-shader geometry from authoritative generated terrain and living trees.
//! It owns no state: an adapter remeshes a section after the world revision that changed it.
//! Faces between opaque cells are culled, uniformly shaded coplanar faces are merged greedily,
//! and every corner carries ambient occlusion plus a bounded presentation skylight sample.

mod quad;
mod section;
mod trees;
mod vertex;
mod volume;

use crate::core::time::{CalendarDefinition, SimulationTick};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::texture::{BakedBlockAppearance, BakedTextureArray};
use crate::world::{BlockStateId, ChunkCoord, WorldRegistry, WorldState};

pub use vertex::{
    SURFACE_VERTEX_OFFSETS, SURFACE_VERTEX_STRIDE, SectionMesh, SurfaceMesh, SurfaceVertex,
};

#[derive(Clone, Copy, Debug)]
struct StateLook {
    opaque: bool,
    appearance: Option<BakedBlockAppearance>,
}

/// Dense per-state opacity and resolved face textures.
struct StateLooks<'a> {
    registry: &'a WorldRegistry,
    looks: Vec<Option<StateLook>>,
}

impl<'a> StateLooks<'a> {
    fn new(registry: &'a WorldRegistry, textures: &BakedTextureArray) -> Self {
        let mut looks = Vec::new();
        for state in registry.states() {
            let Some(block) = registry.get_block(state.block()) else {
                continue;
            };
            let slot = usize::from(state.id().value());
            if looks.len() <= slot {
                looks.resize(slot + 1, None);
            }
            looks[slot] = Some(StateLook {
                opaque: block.is_opaque(),
                appearance: block
                    .appearance()
                    .and_then(|appearance| textures.get_block(appearance)),
            });
        }
        Self { registry, looks }
    }

    const fn registry(&self) -> &'a WorldRegistry {
        self.registry
    }

    fn look(&self, state: BlockStateId) -> Option<StateLook> {
        self.looks
            .get(usize::from(state.value()))
            .copied()
            .flatten()
    }

    /// Unregistered states hide neighbors, matching generated sky-column opacity.
    fn is_opaque(&self, state: BlockStateId) -> bool {
        self.look(state).is_none_or(|look| look.opaque)
    }
}

/// Reusable meshing context for one registry and texture bake.
pub struct SectionMesher<'a> {
    looks: StateLooks<'a>,
    calendar: CalendarDefinition,
    trunk: Option<BakedBlockAppearance>,
    foliage: Option<BakedBlockAppearance>,
}

impl<'a> SectionMesher<'a> {
    #[must_use]
    pub fn new(registries: &'a Registries, textures: &BakedTextureArray) -> Self {
        Self {
            looks: StateLooks::new(registries.world(), textures),
            calendar: registries.core().calendar(),
            trunk: textures.get_block(crate::content::BLOCK_TREE_TRUNK),
            foliage: textures.get_block(crate::content::BLOCK_FOLIAGE),
        }
    }

    /// Terrain faces owned by one generated 16-cubed section. Missing chunks yield no geometry;
    /// faces toward ungenerated neighbors are kept so the explored edge remains closed.
    #[must_use]
    pub fn mesh_section(
        &self,
        world: &WorldState,
        chunk: ChunkCoord,
        section_y: i64,
        origin: VoxelCoord,
    ) -> SectionMesh {
        section::mesh_section(&self.looks, world, chunk, section_y, origin)
    }

    /// Trunks and seasonal crowns of the living trees rooted in one chunk column.
    #[must_use]
    pub fn mesh_chunk_trees(
        &self,
        world: &WorldState,
        chunk: ChunkCoord,
        at: SimulationTick,
        origin: VoxelCoord,
    ) -> SectionMesh {
        trees::mesh_chunk_trees(self, world, chunk, at, origin)
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
