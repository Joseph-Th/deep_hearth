//! Actor-safe immediate-neighbor observation for finite loose surface matter.

use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::spatial::VoxelCoord;

use super::SurfaceResourceId;

/// Actor-visible loose surface matter on one authorized adjacent traversal voxel.
///
/// Immediate observation reveals only identity, commodity, and destination. Exact finite mass,
/// temperature, composition, and age remain exact-local observations after movement.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdjacentSurfaceResource {
    resource: SurfaceResourceId,
    commodity: CommodityKey,
    position: VoxelCoord,
}

impl AdjacentSurfaceResource {
    #[must_use]
    pub const fn resource(self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn commodity(self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    /// Player-local view of loose matter within two physical voxel steps in
    /// generated terrain. Every returned cell has a short, unoccluded sightline
    /// through authoritative blocks, trees and embodied structures. This is a
    /// coarse orientation read: exact mass and freshness still require the player
    /// to stand at the source.
    ///
    /// The query examines at most 25 neighboring cells rather than scanning
    /// remote resources or leaking undiscovered opportunities beyond walls.
    pub fn visible_in_generated_world(
        registries: &crate::registry::Registries,
        state: &AppState,
    ) -> impl Iterator<Item = Self> {
        let mut nearby = Vec::new();
        if let Some(player) = state.logistics().player() {
            let from = player.position();
            if state.world().has_generated_terrain_at(from) {
                for dx in -2_i64..=2 {
                    let remaining = 2 - dx.abs();
                    for dy in -remaining..=remaining {
                        let last = remaining - dy.abs();
                        for dz in -last..=last {
                            let (Some(x), Some(y), Some(z)) = (
                                from.x().checked_add(dx),
                                from.y().checked_add(dy),
                                from.z().checked_add(dz),
                            ) else {
                                continue;
                            };
                            let to = VoxelCoord::new(x, y, z);
                            if !crate::world::has_clear_two_step_sightline(
                                registries, state, from, to,
                            ) {
                                continue;
                            }
                            nearby.extend(state.surface().available_resources_at(Some(to)).map(
                                |resource| AdjacentSurfaceResource {
                                    resource: resource.id(),
                                    commodity: resource.commodity(),
                                    position: resource.position(),
                                },
                            ));
                        }
                    }
                }
            }
        }
        nearby.sort_unstable_by_key(|resource| resource.resource);
        nearby.into_iter()
    }
}

/// Iterates available loose resources on traversal-authorized face-neighbors of the player.
///
/// Callers cannot provide an arbitrary observer coordinate, and inaccessible adjacent voxels are
/// omitted. This is immediate orientation only, not remote scanning or line-of-sight.
pub fn available_adjacent_surface_resources(
    state: &AppState,
) -> impl Iterator<Item = AdjacentSurfaceResource> + '_ {
    // Only visit authored neighboring positions, then their indexed live parcels.
    // Scanning every surface record here would make a local HUD query grow with
    // remote world generation and the number of historically depleted sources.
    let mut nearby = state
        .logistics()
        .player()
        .into_iter()
        .flat_map(|player| {
            state
                .logistics()
                .traversable_destinations(player.position())
        })
        .flat_map(|position| state.surface().available_resources_at(Some(position)))
        .map(|resource| AdjacentSurfaceResource {
            resource: resource.id(),
            commodity: resource.commodity(),
            position: resource.position(),
        })
        .collect::<Vec<_>>();
    // The old actor projection was sorted by global resource ID. Preserve that
    // stable choice order instead of exposing topology/voxel iteration order.
    nearby.sort_unstable_by_key(|resource| resource.resource);
    nearby.into_iter()
}
