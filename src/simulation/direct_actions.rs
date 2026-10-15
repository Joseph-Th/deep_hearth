//! Ordered direct player interactions evaluated after authoritative body motion.

use crate::core::state::AppState;
use crate::inventory::{StorageDefinitionId, validate_place_player_storage_at};
use crate::logistics::{
    PlayerActiveSelection, player_body_occupies_voxel, validate_drop_equipment,
    validate_select_quick_access_slot,
};
use crate::registry::Registries;
use crate::surface::{PlayerMaterialDropOutcome, validate_drop_player_material};
use crate::survival::validate_eat_lot_to_full;
use crate::world::{
    BlockStateId, raycast_player_block, validate_start_player_block_excavation_active,
};

use super::control::{PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent};
use super::outcome::{
    ActiveSelectionDropOutcome, DirectActionOutcomes, PrimaryActionOutcome,
    QuickAccessSelectionOutcome, SecondaryActionOutcome,
};

pub(super) fn attempt_direct_actions(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
) -> DirectActionOutcomes {
    let quick_access = attempt_quick_access_selection(state, input.quick_access_slot());
    let primary = if state.player_work().active().is_none() {
        attempt_primary_action(registries, state, input.primary())
    } else {
        None
    };
    let secondary = if state.player_work().active().is_none() {
        attempt_secondary_action(registries, state, input.secondary())
    } else {
        None
    };
    let drop = attempt_active_selection_drop(registries, state, input.drop_active_selection());
    DirectActionOutcomes::new(quick_access, primary, secondary, drop)
}

fn attempt_quick_access_selection(
    state: &mut AppState,
    requested: Option<crate::logistics::PlayerQuickAccessSlot>,
) -> Option<QuickAccessSelectionOutcome> {
    let slot = requested?;
    Some(match validate_select_quick_access_slot(state, slot) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous quick-access selection became stale: {error}"
                )
            });
            QuickAccessSelectionOutcome::Selected {
                slot,
                selection: state.logistics().active_selection(),
            }
        }
        Err(error) => QuickAccessSelectionOutcome::Rejected(error),
    })
}

fn attempt_primary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<PrimaryControlIntent>,
) -> Option<PrimaryActionOutcome> {
    let intent = intent?;
    match intent {
        PrimaryControlIntent::BlockExcavation { direction } => Some(
            match validate_start_player_block_excavation_active(registries, state, direction) {
                Ok(validated) => {
                    let started = validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous primary block excavation became stale: {error}"
                        )
                    });
                    PrimaryActionOutcome::BlockExcavationStarted(started)
                }
                Err(error) => PrimaryActionOutcome::BlockExcavationRejected(error),
            },
        ),
    }
}

fn attempt_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<SecondaryControlIntent>,
) -> Option<SecondaryActionOutcome> {
    let intent = intent?;
    match intent {
        SecondaryControlIntent::UseActive { direction } => {
            let Some(selection) = state.logistics().active_selection() else {
                return Some(SecondaryActionOutcome::NoActiveSelection);
            };
            let commodity = match selection {
                PlayerActiveSelection::Equipment(_) => {
                    return Some(SecondaryActionOutcome::ActiveEquipmentNotContextual);
                }
                PlayerActiveSelection::Material(commodity) => commodity,
            };
            if let Some(outcome) =
                attempt_selected_storage_placement(registries, state, commodity, direction)
            {
                return Some(outcome);
            }
            if registries.survival().get_food(commodity).is_none() {
                return Some(SecondaryActionOutcome::MaterialNotContextual);
            }
            if !state.control_tick().is_simulation_boundary() {
                return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
            }
            let carried = state
                .logistics()
                .player()
                .map(|player| player.carried_stockpile())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: active material selection requires player")
                });
            let lots = state
                .inventory()
                .lot_ids(carried)
                .filter(|lot| {
                    state
                        .inventory()
                        .get_lot(*lot)
                        .is_some_and(|record| record.commodity() == commodity)
                })
                .collect::<Vec<_>>();
            let lot = match lots.as_slice() {
                [] => return Some(SecondaryActionOutcome::MaterialUnavailable),
                [lot] => *lot,
                lots => {
                    return Some(SecondaryActionOutcome::AmbiguousMaterialLots {
                        count: lots.len(),
                    });
                }
            };
            Some(match validate_eat_lot_to_full(registries, state, lot) {
                Ok(Some(validated)) => {
                    let outcome = validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous selected-food use became stale: {error}"
                        )
                    });
                    SecondaryActionOutcome::EatingStarted(outcome)
                }
                Ok(None) => SecondaryActionOutcome::AlreadySatisfied,
                Err(error) => SecondaryActionOutcome::EatingRejected(error),
            })
        }
    }
}

fn attempt_selected_storage_placement(
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
    let target = match raycast_player_block(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return Some(SecondaryActionOutcome::NoStoragePlacementTarget),
        Err(error) => return Some(SecondaryActionOutcome::StorageTargetRejected(error)),
    };
    let Some(position) = target.face().adjacent_voxel(target.voxel()) else {
        return Some(SecondaryActionOutcome::StoragePlacementCoordinateOverflow);
    };
    match state.world().block_at(position) {
        None => {
            return Some(SecondaryActionOutcome::StoragePlacementWorldUnavailable { position });
        }
        Some(BlockStateId::AIR) => {}
        Some(block) => {
            return Some(SecondaryActionOutcome::StoragePlacementOccupied {
                position,
                state: block,
            });
        }
    }
    if player_body_occupies_voxel(registries, state, position) {
        return Some(SecondaryActionOutcome::StoragePlacementIntersectsPlayer { position });
    }
    if state.logistics().has_stationary_occupant_at(position) {
        return Some(SecondaryActionOutcome::StoragePlacementLocationOccupied { position });
    }
    Some(
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
        },
    )
}

fn attempt_active_selection_drop(
    registries: &Registries,
    state: &mut AppState,
    requested: bool,
) -> Option<ActiveSelectionDropOutcome> {
    if !requested {
        return None;
    }
    let Some(selection) = state.logistics().active_selection() else {
        return Some(ActiveSelectionDropOutcome::NoActiveSelection);
    };
    Some(match selection {
        PlayerActiveSelection::Equipment(equipment) => {
            let position = state
                .logistics()
                .player()
                .map(|player| player.position())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: active carried equipment requires an admitted player"
                    )
                });
            match validate_drop_equipment(registries, state, equipment) {
                Ok(validated) => {
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous active-equipment drop became stale: {error}"
                        )
                    });
                    ActiveSelectionDropOutcome::EquipmentDropped {
                        equipment,
                        position,
                    }
                }
                Err(error) => ActiveSelectionDropOutcome::EquipmentRejected(error),
            }
        }
        PlayerActiveSelection::Material(commodity) => {
            match validate_drop_player_material(registries, state, commodity) {
                Ok(validated) => {
                    let outcome: PlayerMaterialDropOutcome =
                        validated.commit(state).unwrap_or_else(|error| {
                            panic!(
                                "runtime invariant broken: synchronous active-material drop became stale: {error}"
                            )
                        });
                    ActiveSelectionDropOutcome::MaterialDropped(outcome)
                }
                Err(error) => ActiveSelectionDropOutcome::MaterialRejected(error),
            }
        }
    })
}
