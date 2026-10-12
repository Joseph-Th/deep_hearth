//! Actor-safe immediate-neighbor observation for finite loose surface matter.

use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::spatial::VoxelCoord;

use super::{SurfaceResourceId, SurfaceResourceLifecycle};

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
    let player_position = state.logistics().player().map(|player| player.position());
    state.surface().resources().filter_map(move |resource| {
        let player = player_position?;
        (resource.lifecycle() == SurfaceResourceLifecycle::Available
            && !resource.remaining_mass().is_zero()
            && state
                .logistics()
                .is_step_authorized(player, resource.position()))
        .then_some(AdjacentSurfaceResource {
            resource: resource.id(),
            commodity: resource.commodity(),
            position: resource.position(),
        })
    })
}
