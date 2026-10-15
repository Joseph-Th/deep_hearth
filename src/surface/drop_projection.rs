//! Exact-local player-created world-drop presentation over conserved surface custody.

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::material::{CommodityKey, MaterialComposition, ParticleSizeRange};
use crate::spatial::VoxelCoord;

use super::material_drop::PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS;
use super::{SurfaceResourceId, SurfaceResourceKind};

/// Familiar read model for one player-created loose material parcel at the player's exact voxel.
///
/// Surface custody remains authoritative. Natural opportunities are deliberately absent from this
/// projection so an adapter cannot accidentally render forage or in-place resources as dropped
/// inventory merely because both share the finite surface owner.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerWorldDropProjection {
    resource: SurfaceResourceId,
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: Option<ParticleSizeRange>,
    ambient_storage_age_parts: u128,
    automatic_pickup_available_at: Option<ControlTick>,
    automatic_pickup_ready: bool,
}

impl PlayerWorldDropProjection {
    #[must_use]
    pub const fn resource(&self) -> SurfaceResourceId {
        self.resource
    }

    #[must_use]
    pub const fn position(&self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }

    #[must_use]
    pub const fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.particle_size
    }

    #[must_use]
    pub const fn ambient_storage_age_parts(&self) -> u128 {
        self.ambient_storage_age_parts
    }

    /// Control tick when a deliberate player drop becomes eligible for automatic recovery.
    ///
    /// `None` means this drop has no self-pickup delay, as with excavation output.
    #[must_use]
    pub const fn automatic_pickup_available_at(&self) -> Option<ControlTick> {
        self.automatic_pickup_available_at
    }

    #[must_use]
    pub const fn automatic_pickup_ready(&self) -> bool {
        self.automatic_pickup_ready
    }
}

/// Projects only available player-created drops at the admitted player's exact voxel.
///
/// This is a presentation surface, not pickup authorization. Automatic transfer remains owned by
/// the control-tick pickup path and deliberate natural-resource gathering remains a separate verb.
pub fn available_player_world_drops(
    state: &AppState,
) -> impl Iterator<Item = PlayerWorldDropProjection> + '_ {
    let at = state.tick();
    let control_tick = state.control_tick();
    state
        .available_surface_resources()
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(move |resource| {
            let automatic_pickup_available_at =
                resource.automatic_pickup_delay_started_at().map(|started| {
                    started
                        .checked_add_steps(PLAYER_MATERIAL_DROP_PICKUP_DELAY_CONTROL_TICKS)
                        .unwrap_or_else(|| {
                            panic!("validated surface drop pickup delay exceeds control clock")
                        })
                });
            PlayerWorldDropProjection {
                resource: resource.id(),
                position: resource.position(),
                commodity: resource.commodity(),
                mass: resource.remaining_mass(),
                temperature: resource.temperature(),
                composition: resource.composition().clone(),
                particle_size: resource.particle_size(),
                ambient_storage_age_parts: resource
                    .ambient_storage_age_parts_at(at)
                    .unwrap_or_else(|| {
                        panic!("validated surface drop storage age cannot overflow")
                    }),
                automatic_pickup_ready: automatic_pickup_available_at
                    .is_none_or(|eligible| control_tick >= eligible),
                automatic_pickup_available_at,
            }
        })
}
