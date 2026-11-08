//! Physical, pointed placement of material-backed ground storage.

use crate::core::state::AppState;
use crate::inventory::{StorageDefinitionId, validate_place_player_storage_at};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::BlockCollisionShape;

use super::{DirectPlacementTargetError, resolve_direct_placement_target};
use crate::simulation::outcome::SecondaryActionOutcome;

pub(in crate::simulation::direct_actions) fn attempt_selected_storage_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: crate::material::CommodityKey,
    direction: crate::spatial::ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let definitions = registries
        .storage()
        .definitions()
        .filter(|definition| {
            let inputs = definition.assembly_profile().inputs();
            inputs.len() == 1 && inputs[0].commodity() == commodity
        })
        .map(|definition| definition.id())
        .collect::<Vec<StorageDefinitionId>>();
    let definition = match definitions.as_slice() {
        [] => return None,
        [definition] => *definition,
        definitions => {
            return Some(SecondaryActionOutcome::AmbiguousStorageDefinitions {
                count: definitions.len(),
            });
        }
    };
    Some(attempt_storage_placement(
        registries, state, definition, direction,
    ))
}

/// Authoritative pointed placement shared by one-piece selected-material shortcuts and explicit
/// multi-input storage-plan actions. Both use the same terrain, occupation and construction rules.
pub(in crate::simulation::direct_actions) fn attempt_storage_placement(
    registries: &Registries,
    state: &mut AppState,
    definition: StorageDefinitionId,
    direction: crate::spatial::ViewDirection,
) -> SecondaryActionOutcome {
    let (_, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(DirectPlacementTargetError::Target(error)) => {
            return SecondaryActionOutcome::StorageTargetRejected(error);
        }
        Err(DirectPlacementTargetError::NoTarget) => {
            return SecondaryActionOutcome::NoStoragePlacementTarget;
        }
        Err(DirectPlacementTargetError::CoordinateOverflow) => {
            return SecondaryActionOutcome::StoragePlacementCoordinateOverflow;
        }
        Err(DirectPlacementTargetError::WorldUnavailable { position }) => {
            return SecondaryActionOutcome::StoragePlacementWorldUnavailable { position };
        }
        Err(DirectPlacementTargetError::Occupied { position, state }) => {
            return SecondaryActionOutcome::StoragePlacementOccupied { position, state };
        }
        Err(DirectPlacementTargetError::IntersectsPlayer { position }) => {
            return SecondaryActionOutcome::StoragePlacementIntersectsPlayer { position };
        }
        Err(DirectPlacementTargetError::LocationOccupied { position }) => {
            return SecondaryActionOutcome::StoragePlacementLocationOccupied { position };
        }
    };
    // Pointed placement must not suspend a freestanding container in empty air. The lower-level
    // stockpile allocator deliberately owns custody rather than terrain physics; this direct
    // interaction owns the generated-world contact requirement.
    let supported = position
        .y()
        .checked_sub(1)
        .and_then(|y| {
            state
                .world()
                .block_at(VoxelCoord::new(position.x(), y, position.z()))
        })
        .and_then(|block| registries.world().block_for_state(block))
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube);
    if !supported {
        return SecondaryActionOutcome::StoragePlacementUnsupported { position };
    }
    match validate_place_player_storage_at(registries, state, definition, position) {
        Ok(validated) => {
            let stockpile = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous selected-storage placement became stale: {error}"
                    )
                });
            SecondaryActionOutcome::StoragePlaced {
                definition,
                stockpile,
            }
        }
        Err(error) => SecondaryActionOutcome::StoragePlacementRejected { definition, error },
    }
}
