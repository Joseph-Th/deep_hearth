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
