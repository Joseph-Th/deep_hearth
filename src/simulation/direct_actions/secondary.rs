//! Empty-hand gathering and drinking, selected food, and local combustion interactions.

use crate::content::{FORM_BOARD, FORM_LOG, MATERIAL_WOOD};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::{validate_assemble_energy_store, validate_combust_selected_fuel};
use crate::inventory::project_material_stacks;
use crate::logistics::PlayerActiveSelection;
use crate::material::CommodityKey;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::ViewDirection;
use crate::surface::{
    PlayerSurfaceGatheringOption, SurfaceGatheringError, SurfaceResourceId,
    player_surface_gathering_options, validate_start_player_surface_gathering_option,
};
use crate::survival::{
    available_local_drink_sources, validate_drink_store_to_full, validate_eat_stack_to_full,
};
use crate::world::raycast_player_block;

use super::SecondaryActionOutcome;
use super::placement::{
    attempt_selected_storage_placement, attempt_selected_timber_cover_placement,
    attempt_selected_timber_placement,
};

enum DirectSurfaceGatheringResolution {
    None,
    One(PlayerSurfaceGatheringOption),
    AmbiguousResources { count: usize },
    AmbiguousMethods { count: usize },
}

fn resolve_direct_surface_gathering(
    registries: &Registries,
    state: &AppState,
) -> DirectSurfaceGatheringResolution {
    let options = player_surface_gathering_options(registries, state).unwrap_or_else(|error| {
        panic!("runtime invariant broken: direct gathering option projection failed: {error}")
    });
    // The surface owner projects methods grouped by resource. Inspect each group once rather
    // than re-counting the entire option set for every visible source.
    if let Some(methods) = options
        .chunk_by(|left, right| left.resource() == right.resource())
        .find(|methods| methods.len() > 1)
    {
        return DirectSurfaceGatheringResolution::AmbiguousMethods {
            count: methods.len(),
        };
    }
    match options.as_slice() {
        [] => DirectSurfaceGatheringResolution::None,
        [option] => DirectSurfaceGatheringResolution::One(*option),
        options => DirectSurfaceGatheringResolution::AmbiguousResources {
            count: options.len(),
        },
    }
}

pub(super) fn attempt_empty_hand_secondary_action(
    registries: &Registries,
    state: &mut AppState,
) -> SecondaryActionOutcome {
    let drink_sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    let gathering = resolve_direct_surface_gathering(registries, state);
    let surface_resources = match gathering {
        DirectSurfaceGatheringResolution::AmbiguousMethods { count } => {
            return SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods { count };
        }
        DirectSurfaceGatheringResolution::AmbiguousResources { count } => count,
        DirectSurfaceGatheringResolution::One(_) => 1,
        DirectSurfaceGatheringResolution::None => 0,
    };
    if surface_resources != 0 && !drink_sources.is_empty() {
        return SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: drink_sources.len(),
            surface_resources,
        };
    }
    if surface_resources > 1 {
        return SecondaryActionOutcome::AmbiguousLocalSurfaceResources {
            count: surface_resources,
        };
    }
    if let DirectSurfaceGatheringResolution::One(target) = gathering {
        return attempt_direct_surface_gathering(registries, state, target);
    }
    match drink_sources.as_slice() {
        [] => SecondaryActionOutcome::NoActiveSelection,
        [source] => attempt_direct_drinking(registries, state, source.store()),
        sources => SecondaryActionOutcome::AmbiguousLocalDrinkSources {
            count: sources.len(),
        },
    }
}

fn attempt_direct_surface_gathering(
    registries: &Registries,
    state: &mut AppState,
    option: PlayerSurfaceGatheringOption,
) -> SecondaryActionOutcome {
    if let Some(blocker) = option.hand_handling_blocker() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(blocker);
    }
    if !state.control_tick().is_simulation_boundary() {
        return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
    }
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: local surface resource requires player")
        });
    let available_capacity = state
        .inventory()
        .get_stockpile(carried)
        .map(|stockpile| stockpile.available_capacity())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: admitted player lost carried stockpile")
        });
    let desired = option.source_mass().min(option.direct_batch_mass());
    let gathered_mass = desired.min(available_capacity);
    let request_mass = if gathered_mass.is_zero() {
        desired
    } else {
        gathered_mass
    };
    match validate_start_player_surface_gathering_option(registries, state, option, request_mass) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous surface gathering became stale: {error}"
                )
            });
            let work = match state.player_work().active() {
                Some(crate::labor::PlayerWork::SurfaceGathering { work }) => work,
                other => {
                    panic!("runtime invariant broken: direct surface gathering started {other:?}")
                }
            };
            SecondaryActionOutcome::SurfaceGatheringStarted(work)
        }
        Err(error) => SecondaryActionOutcome::SurfaceGatheringRejected(error),
    }
}

/// A catalog gesture names the exact observed natural source and amount. The
/// option is re-resolved on execution, so a stolen/depleted or remote source
/// cannot be silently replaced by a different one.
pub(super) fn attempt_explicit_surface_gathering(
    registries: &Registries,
    state: &mut AppState,
    resource: SurfaceResourceId,
    mass: Mass,
) -> SecondaryActionOutcome {
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return SecondaryActionOutcome::PlayerNotAdmitted;
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return SecondaryActionOutcome::PlayerDead;
        }
    }
    if mass.is_zero() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::ZeroMass);
    }
    let options = player_surface_gathering_options(registries, state).unwrap_or_else(|error| {
        panic!("runtime invariant broken: explicit gathering catalog failed: {error}")
    });
    let mut matching = options
        .into_iter()
        .filter(|option| option.resource() == resource);
    let Some(option) = matching.next() else {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::ResourceUnavailableAtPlayer { resource },
        );
    };
    let additional = matching.count();
    if additional > 0 {
        return SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods {
            count: additional + 1,
        };
    }
    // These are already actor-visible immutable facts. Refuse impossible
    // amounts on the input frame instead of queueing a three-second wait for
    // an error the player could have predicted from the catalog.
    if mass > option.maximum_batch_mass() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::BatchTooLarge {
                requested: mass,
                maximum: option.maximum_batch_mass(),
            },
        );
    }
    if mass > option.source_mass() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(
            SurfaceGatheringError::InsufficientResource {
                requested: mass,
                remaining: option.source_mass(),
            },
        );
    }
    // A source's thermal blocker is visible now. Do not persist a doomed
    // one-shot request solely to reject it at the next slow boundary.
    if let Some(blocker) = option.hand_handling_blocker() {
        return SecondaryActionOutcome::SurfaceGatheringRejected(blocker);
    }
    if !state.control_tick().is_simulation_boundary() {
        return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
    }
    match validate_start_player_surface_gathering_option(registries, state, option, mass) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!("runtime invariant broken: explicit surface gathering became stale: {error}")
            });
            match state.player_work().active() {
                Some(crate::labor::PlayerWork::SurfaceGathering { work }) => {
                    SecondaryActionOutcome::SurfaceGatheringStarted(work)
                }
                other => panic!("explicit surface gathering started unexpected work: {other:?}"),
            }
        }
        Err(error) => SecondaryActionOutcome::SurfaceGatheringRejected(error),
    }
}

fn attempt_direct_drinking(
    registries: &Registries,
    state: &mut AppState,
    store: crate::fluid::FluidStoreId,
) -> SecondaryActionOutcome {
    if !state.control_tick().is_simulation_boundary() {
        return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
    }
    match validate_drink_store_to_full(registries, state, store) {
        Ok(Some(validated)) => {
            let outcome = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous local-drink use became stale: {error}"
                )
            });
            SecondaryActionOutcome::DrinkingStarted(outcome)
        }
        Ok(None) => SecondaryActionOutcome::AlreadySatisfied,
        Err(error) => SecondaryActionOutcome::DrinkingRejected(error),
    }
}

/// Resolves a single visible carried stack without selecting an arbitrary physical lot.
pub(super) fn resolve_unique_carried_stack(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> Result<crate::inventory::MaterialStackProjection, SecondaryActionOutcome> {
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active material selection requires player")
        });
    let mut matches = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: carried stack projection failed: {error}")
        })
        .into_iter()
        .filter(|stack| stack.commodity() == commodity);
    let Some(first) = matches.next() else {
        return Err(SecondaryActionOutcome::MaterialUnavailable);
    };
    let remaining = matches.count();
    if remaining != 0 {
        return Err(SecondaryActionOutcome::AmbiguousMaterialStacks {
            count: remaining + 1,
        });
    }
    Ok(first)
}

pub(super) fn attempt_selected_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    selection: PlayerActiveSelection,
    direction: ViewDirection,
) -> SecondaryActionOutcome {
    let commodity = match selection {
        PlayerActiveSelection::Equipment(_) => {
            return SecondaryActionOutcome::ActiveEquipmentNotContextual;
        }
        PlayerActiveSelection::Material(commodity) => commodity,
    };
    if let Some(outcome) =
        attempt_selected_storage_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    let points_at_block = matches!(
        raycast_player_block(registries, state, direction),
        Ok(Some(_))
    );
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    if commodity == timber
        && points_at_block
        && let Some(outcome) =
            attempt_selected_timber_placement(registries, state, commodity, direction)
    {
        // A downward aim at the ground under the player's feet cannot
        // materialize a post in their own occupied body. If the same log can
        // fuel an existing co-located fire, do not let that impossible
        // placement shadow the valid direct-use action. Real pointed post
        // placements retain priority over local combustion.
        if matches!(
            outcome,
            SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { .. }
        ) && let Some(combustion) = attempt_selected_combustion(registries, state, commodity)
        {
            return combustion;
        }
        return outcome;
    }
    if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
        && let Some(outcome) =
            attempt_selected_timber_cover_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    if let Some(outcome) = attempt_selected_combustion(registries, state, commodity) {
        return outcome;
    }
    if let Some(outcome) =
        attempt_selected_timber_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    if registries.survival().get_food(commodity).is_none() {
        return SecondaryActionOutcome::MaterialNotContextual;
    }
    if !state.control_tick().is_simulation_boundary() {
        return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
    }
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return outcome,
    };
    let stack = &stack;
    match validate_eat_stack_to_full(registries, state, stack) {
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
    }
}

fn attempt_selected_combustion(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
) -> Option<SecondaryActionOutcome> {
    let matching_stores = state
        .available_local_energy_stores()
        .filter(|store| {
            registries
                .energy()
                .get_store(store.definition())
                .is_some_and(|definition| definition.combustion_profile_for(commodity).is_some())
        })
        .map(|store| store.id())
        .collect::<Vec<_>>();
    match matching_stores.as_slice() {
        [store] => {
            let store = *store;
            let source = state
                .logistics()
                .player()
                .map(|player| player.carried_stockpile())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: local combustion store requires player")
                });
            return Some(
                match validate_combust_selected_fuel(registries, state, store, source, commodity) {
                    Ok(validated) => {
                        let released = validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous combustion became stale: {error}"
                        )
                    });
                        SecondaryActionOutcome::CombustionFuelAdded { store, released }
                    }
                    Err(error) => SecondaryActionOutcome::CombustionFuelRejected { store, error },
                },
            );
        }
        [] => {}
        stores => {
            return Some(SecondaryActionOutcome::AmbiguousLocalCombustionStores {
                count: stores.len(),
            });
        }
    }

    let definitions = registries
        .energy()
        .definitions()
        .filter(|definition| definition.combustion_profile().is_some())
        .filter(|definition| {
            definition.assembly_profile().is_some_and(|profile| {
                let inputs = profile.inputs();
                inputs.len() == 1 && inputs[0].commodity() == commodity
            })
        })
        .map(|definition| definition.id())
        .collect::<Vec<_>>();
    let definition = match definitions.as_slice() {
        [] => return None,
        [definition] => *definition,
        definitions => {
            return Some(SecondaryActionOutcome::AmbiguousCombustionDefinitions {
                count: definitions.len(),
            });
        }
    };
    let source = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: selected combustion assembly requires player")
        });
    Some(
        match validate_assemble_energy_store(registries, state, definition, source) {
            Ok(validated) => {
                let store = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous combustion-store assembly became stale: {error}"
                    )
                });
                SecondaryActionOutcome::EnergyStoreAssembled { definition, store }
            }
            Err(error) => SecondaryActionOutcome::EnergyStoreAssemblyRejected { definition, error },
        },
    )
}
