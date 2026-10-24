//! Player-local, non-reserving candidates for one chosen exact pour volume.
//!
//! A caller does not have to know store IDs in advance or rebuild fluid
//! temperature, geometry, container capacity and support rules. This read
//! projection intentionally reveals only the player's own voxel and its
//! immediate gravity destination, never distant fluid ownership.

use crate::core::quantity::Volume;
use crate::core::state::AppState;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    FluidContents, FluidStoreId, FluidTransferError, validate_gravity_feed_fluid_to_lower_store,
    validate_transfer_fluid_between_local_stores,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerFluidTransferRoute {
    SameVoxel,
    OneVoxelDown,
}

/// A proposed volume, including either its exact future contents or the
/// canonical typed blocker. This is an observation, not an authorization.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerFluidTransferOption {
    source: FluidStoreId,
    destination: FluidStoreId,
    route: PlayerFluidTransferRoute,
    requested: Volume,
    source_after: Option<FluidContents>,
    destination_after: Option<FluidContents>,
    blocker: Option<FluidTransferError>,
}

impl PlayerFluidTransferOption {
    #[must_use]
    pub const fn source(&self) -> FluidStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> FluidStoreId {
        self.destination
    }

    #[must_use]
    pub const fn route(&self) -> PlayerFluidTransferRoute {
        self.route
    }

    #[must_use]
    pub const fn requested(&self) -> Volume {
        self.requested
    }

    #[must_use]
    pub const fn source_after(&self) -> Option<FluidContents> {
        self.source_after
    }

    #[must_use]
    pub const fn destination_after(&self) -> Option<FluidContents> {
        self.destination_after
    }

    #[must_use]
    pub const fn blocker(&self) -> Option<&FluidTransferError> {
        self.blocker.as_ref()
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.blocker.is_none()
    }
}

/// Lists actual local/downhill transfers for the specified *exact* volume.
///
/// An option's ready status is not a promise that smaller or larger volumes
/// can transfer: exact-temperature arithmetic can admit one amount and reject
/// a neighboring amount. The executing caller must revalidate with the route's
/// canonical validator, especially after world, logistics or fluid changes.
pub fn player_fluid_transfer_options(
    registries: &Registries,
    state: &AppState,
    requested: Volume,
) -> Result<Vec<PlayerFluidTransferOption>, FluidTransferError> {
    validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => FluidTransferError::PlayerNotAdmitted,
        PlayerActionValidationError::PlayerDead => FluidTransferError::PlayerDead,
    })?;
    if requested.is_zero() {
        return Err(FluidTransferError::ZeroVolume);
    }
    let position = state
        .logistics()
        .player()
        .expect("validated player exists")
        .position();
    let below = position
        .y()
        .checked_sub(1)
        .map(|y| VoxelCoord::new(position.x(), y, position.z()));
    let mut destinations = state
        .logistics()
        .fluid_stores_at(position)
        .map(|store| (store, PlayerFluidTransferRoute::SameVoxel))
        .chain(below.into_iter().flat_map(|below| {
            state
                .logistics()
                .fluid_stores_at(below)
                .map(|store| (store, PlayerFluidTransferRoute::OneVoxelDown))
        }))
        .collect::<Vec<_>>();
    destinations.sort_unstable_by_key(|(store, _)| *store);
    let mut options = Vec::new();
    for source in state.available_local_fluid_stores() {
        if source.contents().is_none() {
            continue;
        }
        for &(destination, route) in &destinations {
            if source.id() == destination {
                continue;
            }
            let assessment = match route {
                PlayerFluidTransferRoute::SameVoxel => {
                    validate_transfer_fluid_between_local_stores(
                        registries,
                        state,
                        source.id(),
                        destination,
                        requested,
                    )
                }
                PlayerFluidTransferRoute::OneVoxelDown => {
                    validate_gravity_feed_fluid_to_lower_store(
                        registries,
                        state,
                        source.id(),
                        destination,
                        requested,
                    )
                }
            };
            let (source_after, destination_after, blocker) = match assessment {
                Ok(validated) => (
                    validated.after_source,
                    Some(validated.after_destination),
                    None,
                ),
                Err(error) => (None, None, Some(error)),
            };
            options.push(PlayerFluidTransferOption {
                source: source.id(),
                destination,
                route,
                requested,
                source_after,
                destination_after,
                blocker,
            });
        }
    }
    Ok(options)
}
