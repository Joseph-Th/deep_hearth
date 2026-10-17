//! Ordered direct player interactions evaluated after authoritative body motion.

use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, FORM_BOARD, FORM_LOG, MATERIAL_WOOD,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use crate::core::state::AppState;
use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
use crate::inventory::{
    StorageDefinitionId, project_material_stacks, validate_place_player_storage_at,
};
use crate::logistics::{
    PlayerActiveSelection, player_body_occupies_voxel, take_player_pending_primary_break,
    take_player_pending_secondary_use, validate_buffer_player_primary_break,
    validate_buffer_player_secondary_use, validate_drop_equipment,
    validate_select_quick_access_slot,
};
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelBounds, VoxelCoord};
use crate::structural::{
    GroundedStructuralElementPlan, MultiSupportedStructuralElementPlan, StructuralElementGeometry,
    StructuralElementId, validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_multi_supported_structural_construction_from_stack,
    validate_start_player_structural_dismantling,
};
use crate::surface::{PlayerMaterialDropOutcome, validate_drop_player_material};
use crate::surface::{
    PlayerSurfaceGatheringOption, player_surface_gathering_options,
    validate_start_player_surface_gathering_option,
};
use crate::survival::{
    available_local_drink_sources, validate_drink_store_to_full, validate_eat_stack_to_full,
};
use crate::world::{
    BlockCollisionShape, BlockStateId, PlayerBlockTargetError, raycast_player_air_voxel,
    raycast_player_block, validate_start_player_block_excavation_active,
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
    pending_primary: Option<PrimaryActionOutcome>,
    pending_secondary: Option<SecondaryActionOutcome>,
) -> DirectActionOutcomes {
    let quick_access = attempt_quick_access_selection(state, input.quick_access_slot());
    attempt_direct_actions_after_quick_access(
        registries,
        state,
        input,
        quick_access,
        pending_primary,
        pending_secondary,
    )
}

pub(super) fn attempt_direct_actions_after_quick_access(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
    quick_access: Option<QuickAccessSelectionOutcome>,
    pending_primary: Option<PrimaryActionOutcome>,
    pending_secondary: Option<SecondaryActionOutcome>,
) -> DirectActionOutcomes {
    let primary = if pending_primary.is_some() {
        pending_primary
    } else if state.player_work().active().is_none() {
        attempt_primary_action(registries, state, input.primary())
    } else {
        None
    };
    let secondary = if state.player_work().active().is_some() {
        pending_secondary
    } else {
        attempt_secondary_action(registries, state, input.secondary()).or(pending_secondary)
    };
    let drop = attempt_active_selection_drop(registries, state, input.drop_active_selection());
    DirectActionOutcomes::new(quick_access, primary, secondary, drop)
}

pub(super) fn attempt_quick_access_selection(
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
        PrimaryControlIntent::BreakTarget { direction } => {
            Some(attempt_primary_break(registries, state, direction))
        }
    }
}

fn attempt_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<SecondaryControlIntent>,
) -> Option<SecondaryActionOutcome> {
    let intent = intent?;
    if state.logistics().pending_secondary_use().is_some() {
        return Some(SecondaryActionOutcome::QueuedUntilSimulationBoundary);
    }
    let SecondaryControlIntent::UseActive { direction } = intent;
    let outcome = attempt_secondary_action_now(registries, state, direction);
    if outcome != SecondaryActionOutcome::DeferredUntilSimulationBoundary {
        return Some(outcome);
    }
    Some(
        match validate_buffer_player_secondary_use(state, direction) {
            Ok(validated) => {
                validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous contextual-use buffering became stale: {error}"
                    )
                });
                SecondaryActionOutcome::QueuedUntilSimulationBoundary
            }
            Err(error) => SecondaryActionOutcome::BufferRejected(error),
        },
    )
}

fn attempt_secondary_action_now(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> SecondaryActionOutcome {
    match state.logistics().active_selection() {
        None => attempt_empty_hand_secondary_action(registries, state),
        Some(selection) => {
            attempt_selected_secondary_action(registries, state, selection, direction)
        }
    }
}

pub(super) fn attempt_pending_secondary_action(
    registries: &Registries,
    state: &mut AppState,
) -> Option<SecondaryActionOutcome> {
    let pending = take_player_pending_secondary_use(state)?;
    let current_position = state.logistics().player().map(|player| player.position());
    if current_position != Some(pending.position())
        || state.logistics().active_selection() != pending.selection()
    {
        return Some(SecondaryActionOutcome::BufferedContextChanged);
    }
    let direction = pending.direction().unwrap_or_else(|| {
        panic!("runtime invariant broken: trusted buffered contextual use has invalid direction")
    });
    Some(attempt_secondary_action_now(registries, state, direction))
}

pub(super) fn attempt_pending_primary_break(
    registries: &Registries,
    state: &mut AppState,
) -> Option<PrimaryActionOutcome> {
    let pending = take_player_pending_primary_break(state)?;
    let current_position = state.logistics().player().map(|player| player.position());
    if current_position != Some(pending.position()) {
        return Some(PrimaryActionOutcome::BufferedContextChanged);
    }
    let direction = pending.direction().unwrap_or_else(|| {
        panic!("runtime invariant broken: trusted buffered structural break has invalid direction")
    });
    match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) if element == pending.element() => Some(attempt_structural_dismantling_now(
            registries, state, element,
        )),
        Ok(_)
        | Err(DirectStructuralTargetError::TargetUnavailable)
        | Err(DirectStructuralTargetError::NoTarget)
        | Err(DirectStructuralTargetError::Ambiguous { .. }) => {
            Some(PrimaryActionOutcome::BufferedContextChanged)
        }
    }
}

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
    for option in &options {
        let count = options
            .iter()
            .filter(|candidate| candidate.resource() == option.resource())
            .count();
        if count > 1 {
            return DirectSurfaceGatheringResolution::AmbiguousMethods { count };
        }
    }
    match options.as_slice() {
        [] => DirectSurfaceGatheringResolution::None,
        [option] => DirectSurfaceGatheringResolution::One(*option),
        options => DirectSurfaceGatheringResolution::AmbiguousResources {
            count: options.len(),
        },
    }
}

fn attempt_empty_hand_secondary_action(
    registries: &Registries,
    state: &mut AppState,
) -> SecondaryActionOutcome {
    let drink_sources = available_local_drink_sources(registries, state).collect::<Vec<_>>();
    let gathering = resolve_direct_surface_gathering(registries, state);
    match gathering {
        DirectSurfaceGatheringResolution::AmbiguousMethods { count } => {
            return SecondaryActionOutcome::AmbiguousSurfaceGatheringMethods { count };
        }
        DirectSurfaceGatheringResolution::AmbiguousResources { count } => {
            return SecondaryActionOutcome::AmbiguousLocalSurfaceResources { count };
        }
        DirectSurfaceGatheringResolution::None | DirectSurfaceGatheringResolution::One(_) => {}
    }
    if !drink_sources.is_empty() && matches!(gathering, DirectSurfaceGatheringResolution::One(_)) {
        return SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: drink_sources.len(),
            surface_resources: 1,
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

fn attempt_selected_secondary_action(
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
    if commodity == timber && points_at_block {
        if let Some(outcome) =
            attempt_selected_timber_placement(registries, state, commodity, direction)
        {
            return outcome;
        }
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
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active material selection requires player")
        });
    let stacks = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: carried stack projection failed: {error}")
        })
        .into_iter()
        .filter(|stack| stack.commodity() == commodity)
        .collect::<Vec<_>>();
    let stack = match stacks.as_slice() {
        [] => return SecondaryActionOutcome::MaterialUnavailable,
        [stack] => stack,
        stacks => {
            return SecondaryActionOutcome::AmbiguousMaterialStacks {
                count: stacks.len(),
            };
        }
    };
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
                .and_then(|definition| definition.combustion_profile())
                .is_some_and(|profile| profile.fuel() == commodity)
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
                match validate_combust_fuel(registries, state, store, source) {
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

enum DirectStructuralTargetError {
    TargetUnavailable,
    NoTarget,
    Ambiguous { count: usize },
}

fn resolve_direct_structural_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<StructuralElementId, DirectStructuralTargetError> {
    let target = raycast_player_air_voxel(registries, state, direction, |voxel, _face| {
        let mut elements = state
            .structures()
            .elements()
            .filter(|record| record.bounds().has_voxel(voxel))
            .map(|record| record.id());
        let first = elements.next()?;
        let count = 1 + elements.count();
        Some(if count == 1 { Ok(first) } else { Err(count) })
    })
    .map_err(|_| DirectStructuralTargetError::TargetUnavailable)?
    .ok_or(DirectStructuralTargetError::NoTarget)?;
    target.map_err(|count| DirectStructuralTargetError::Ambiguous { count })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectStructuralFaceTarget {
    element: StructuralElementId,
    voxel: VoxelCoord,
    face: CubeFace,
}

fn resolve_direct_structural_face_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<Option<DirectStructuralFaceTarget>, DirectStructuralTargetError> {
    let target = raycast_player_air_voxel(registries, state, direction, |voxel, face| {
        let mut elements = state
            .structures()
            .elements()
            .filter(|record| record.bounds().has_voxel(voxel))
            .map(|record| record.id());
        let first = elements.next()?;
        let count = 1 + elements.count();
        Some(if count == 1 {
            Ok(DirectStructuralFaceTarget {
                element: first,
                voxel,
                face,
            })
        } else {
            Err(count)
        })
    })
    .map_err(|_| DirectStructuralTargetError::TargetUnavailable)?;
    target
        .transpose()
        .map_err(|count| DirectStructuralTargetError::Ambiguous { count })
}

fn attempt_primary_break(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> PrimaryActionOutcome {
    let element = match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) => Some(element),
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return PrimaryActionOutcome::AmbiguousStructuralTarget { count };
        }
        Err(
            DirectStructuralTargetError::TargetUnavailable | DirectStructuralTargetError::NoTarget,
        ) => None,
    };

    if let Some(element) = element {
        if !state.control_tick().is_simulation_boundary() {
            return match validate_buffer_player_primary_break(state, element, direction) {
                Ok(validated) => {
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous structural-break buffering became stale: {error}"
                        )
                    });
                    PrimaryActionOutcome::QueuedUntilSimulationBoundary
                }
                Err(error) => PrimaryActionOutcome::BufferRejected(error),
            };
        }
        return attempt_structural_dismantling_now(registries, state, element);
    }

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
    }
}

fn validate_direct_placement_position(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<(), DirectPlacementTargetError> {
    match state.world().block_at(position) {
        None => return Err(DirectPlacementTargetError::WorldUnavailable { position }),
        Some(BlockStateId::AIR) => {}
        Some(block) => {
            return Err(DirectPlacementTargetError::Occupied {
                position,
                state: block,
            });
        }
    }
    if player_body_occupies_voxel(registries, state, position) {
        return Err(DirectPlacementTargetError::IntersectsPlayer { position });
    }
    if state.logistics().has_stationary_occupant_at(position)
        || state
            .structures()
            .elements()
            .any(|record| record.bounds().has_voxel(position))
    {
        return Err(DirectPlacementTargetError::LocationOccupied { position });
    }
    Ok(())
}

fn attempt_supported_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let target = match resolve_direct_structural_face_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) | Err(DirectStructuralTargetError::TargetUnavailable) => return None,
        Err(DirectStructuralTargetError::NoTarget) => return None,
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return Some(SecondaryActionOutcome::AmbiguousStructuralPlacementTarget { count });
        }
    };
    let Some(position) = target.face.adjacent_voxel(target.voxel) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    if let Err(error) = validate_direct_placement_position(registries, state, position) {
        return Some(match error {
            DirectPlacementTargetError::Target(error) => {
                SecondaryActionOutcome::StructuralTargetRejected(error)
            }
            DirectPlacementTargetError::NoTarget => {
                SecondaryActionOutcome::NoStructuralPlacementTarget
            }
            DirectPlacementTargetError::CoordinateOverflow => {
                SecondaryActionOutcome::StructuralPlacementCoordinateOverflow
            }
            DirectPlacementTargetError::WorldUnavailable { position } => {
                SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position }
            }
            DirectPlacementTargetError::Occupied { position, state } => {
                SecondaryActionOutcome::StructuralPlacementOccupied { position, state }
            }
            DirectPlacementTargetError::IntersectsPlayer { position } => {
                SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }
            }
            DirectPlacementTargetError::LocationOccupied { position } => {
                SecondaryActionOutcome::StructuralPlacementLocationOccupied { position }
            }
        });
    }
    let Some(bounds) = single_voxel_bounds(position) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("built-in supported timber geometry is invalid: {error}"));
    let supports = state
        .structures()
        .elements()
        .filter(|record| {
            record.lifecycle() == crate::structural::StructuralLifecycle::Active
                && bounds.has_face_contact(record.bounds())
        })
        .map(|record| record.id())
        .collect::<Vec<_>>();
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        supports,
    );
    Some(
        match validate_start_player_multi_supported_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous supported timber placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
}

fn attempt_structural_dismantling_now(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
) -> PrimaryActionOutcome {
    match validate_start_player_structural_dismantling(
        registries,
        state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    ) {
        Ok(validated) => {
            let outcome = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous structural dismantling became stale: {error}"
                )
            });
            PrimaryActionOutcome::StructuralDismantlingStarted(outcome)
        }
        Err(error) => PrimaryActionOutcome::StructuralDismantlingRejected(error),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DirectPlacementTargetError {
    Target(PlayerBlockTargetError),
    NoTarget,
    CoordinateOverflow,
    WorldUnavailable {
        position: VoxelCoord,
    },
    Occupied {
        position: VoxelCoord,
        state: BlockStateId,
    },
    IntersectsPlayer {
        position: VoxelCoord,
    },
    LocationOccupied {
        position: VoxelCoord,
    },
}

fn resolve_direct_placement_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<(CubeFace, VoxelCoord), DirectPlacementTargetError> {
    let target = match raycast_player_block(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return Err(DirectPlacementTargetError::NoTarget),
        Err(error) => return Err(DirectPlacementTargetError::Target(error)),
    };
    let position = target
        .face()
        .adjacent_voxel(target.voxel())
        .ok_or(DirectPlacementTargetError::CoordinateOverflow)?;
    validate_direct_placement_position(registries, state, position)?;
    Ok((target.face(), position))
}

fn single_voxel_bounds(position: VoxelCoord) -> Option<VoxelBounds> {
    let max = VoxelCoord::new(
        position.x().checked_add(1)?,
        position.y().checked_add(1)?,
        position.z().checked_add(1)?,
    );
    VoxelBounds::new(position, max).ok()
}

fn attempt_selected_timber_cover_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    if commodity != CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
        return None;
    }
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active board selection requires player")
        });
    let stacks = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: carried board projection failed: {error}")
        })
        .into_iter()
        .filter(|stack| stack.commodity() == commodity)
        .collect::<Vec<_>>();
    let stack = match stacks.as_slice() {
        [] => return Some(SecondaryActionOutcome::MaterialUnavailable),
        [stack] => stack,
        stacks => {
            return Some(SecondaryActionOutcome::AmbiguousMaterialStacks {
                count: stacks.len(),
            });
        }
    };
    let target = match resolve_direct_structural_face_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) | Err(DirectStructuralTargetError::TargetUnavailable) => {
            return Some(SecondaryActionOutcome::NoStructuralPlacementTarget);
        }
        Err(DirectStructuralTargetError::NoTarget) => {
            return Some(SecondaryActionOutcome::NoStructuralPlacementTarget);
        }
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return Some(SecondaryActionOutcome::AmbiguousStructuralPlacementTarget { count });
        }
    };
    let Some(position) = target.face.adjacent_voxel(target.voxel) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    if let Err(error) = validate_direct_placement_position(registries, state, position) {
        return Some(match error {
            DirectPlacementTargetError::Target(error) => {
                SecondaryActionOutcome::StructuralTargetRejected(error)
            }
            DirectPlacementTargetError::NoTarget => {
                SecondaryActionOutcome::NoStructuralPlacementTarget
            }
            DirectPlacementTargetError::CoordinateOverflow => {
                SecondaryActionOutcome::StructuralPlacementCoordinateOverflow
            }
            DirectPlacementTargetError::WorldUnavailable { position } => {
                SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position }
            }
            DirectPlacementTargetError::Occupied { position, state } => {
                SecondaryActionOutcome::StructuralPlacementOccupied { position, state }
            }
            DirectPlacementTargetError::IntersectsPlayer { position } => {
                SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }
            }
            DirectPlacementTargetError::LocationOccupied { position } => {
                SecondaryActionOutcome::StructuralPlacementLocationOccupied { position }
            }
        });
    }
    let Some(bounds) = single_voxel_bounds(position) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap_or_else(|error| panic!("built-in timber cover-panel geometry is invalid: {error}"));
    let supports = state
        .structures()
        .elements()
        .filter(|record| {
            record.lifecycle() == crate::structural::StructuralLifecycle::Active
                && bounds.has_face_contact(record.bounds())
        })
        .map(|record| record.id())
        .collect::<Vec<_>>();
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        supports,
    );
    Some(
        match validate_start_player_multi_supported_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous timber cover placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
}

fn attempt_selected_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    if commodity != CommodityKey::new(MATERIAL_WOOD, FORM_LOG) {
        return None;
    }
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: active timber selection requires player")
        });
    let stacks = project_material_stacks(registries, state, carried)
        .unwrap_or_else(|error| {
            panic!("runtime invariant broken: carried timber projection failed: {error}")
        })
        .into_iter()
        .filter(|stack| stack.commodity() == commodity)
        .collect::<Vec<_>>();
    let stack = match stacks.as_slice() {
        [] => return Some(SecondaryActionOutcome::MaterialUnavailable),
        [stack] => stack,
        stacks => {
            return Some(SecondaryActionOutcome::AmbiguousMaterialStacks {
                count: stacks.len(),
            });
        }
    };
    if let Some(outcome) = attempt_supported_timber_placement(registries, state, stack, direction) {
        return Some(outcome);
    }
    let (face, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(DirectPlacementTargetError::Target(error)) => {
            return Some(SecondaryActionOutcome::StructuralTargetRejected(error));
        }
        Err(DirectPlacementTargetError::NoTarget) => {
            return Some(SecondaryActionOutcome::NoStructuralPlacementTarget);
        }
        Err(DirectPlacementTargetError::CoordinateOverflow) => {
            return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
        }
        Err(DirectPlacementTargetError::WorldUnavailable { position }) => {
            return Some(SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position });
        }
        Err(DirectPlacementTargetError::Occupied { position, state }) => {
            return Some(SecondaryActionOutcome::StructuralPlacementOccupied { position, state });
        }
        Err(DirectPlacementTargetError::IntersectsPlayer { position }) => {
            return Some(SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position });
        }
        Err(DirectPlacementTargetError::LocationOccupied { position }) => {
            return Some(SecondaryActionOutcome::StructuralPlacementLocationOccupied { position });
        }
    };
    if face != CubeFace::Top {
        return Some(SecondaryActionOutcome::StructuralPlacementRequiresTopFace { face });
    }
    let Some(bounds) = single_voxel_bounds(position) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("built-in timber-post geometry is invalid: {error}"));
    let plan = GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
    );
    Some(
        match validate_start_player_grounded_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous timber placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
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
    let (_, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(DirectPlacementTargetError::Target(error)) => {
            return Some(SecondaryActionOutcome::StorageTargetRejected(error));
        }
        Err(DirectPlacementTargetError::NoTarget) => {
            return Some(SecondaryActionOutcome::NoStoragePlacementTarget);
        }
        Err(DirectPlacementTargetError::CoordinateOverflow) => {
            return Some(SecondaryActionOutcome::StoragePlacementCoordinateOverflow);
        }
        Err(DirectPlacementTargetError::WorldUnavailable { position }) => {
            return Some(SecondaryActionOutcome::StoragePlacementWorldUnavailable { position });
        }
        Err(DirectPlacementTargetError::Occupied { position, state }) => {
            return Some(SecondaryActionOutcome::StoragePlacementOccupied { position, state });
        }
        Err(DirectPlacementTargetError::IntersectsPlayer { position }) => {
            return Some(SecondaryActionOutcome::StoragePlacementIntersectsPlayer { position });
        }
        Err(DirectPlacementTargetError::LocationOccupied { position }) => {
            return Some(SecondaryActionOutcome::StoragePlacementLocationOccupied { position });
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
        return Some(SecondaryActionOutcome::StoragePlacementUnsupported { position });
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
