//! Canonical timber post, framing, and cover placement using shared physical targeting.

use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION, BASIC_TIMBER_DRIVE_COUPLING_LENGTH,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, FORM_BOARD, FORM_HANDLE, FORM_LOG,
    MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
};
use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection};
use crate::structural::{
    GroundedStructuralElementPlan, MultiSupportedStructuralElementPlan, StructuralElementGeometry,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_multi_supported_structural_construction_from_stack,
};

use super::super::secondary::resolve_unique_carried_stack;
use super::{
    resolve_direct_placement_target, resolve_supported_timber_target, single_voxel_bounds,
    structural_placement_error, validate_full_footprint_placement_position,
};
use crate::simulation::outcome::SecondaryActionOutcome;

fn attempt_supported_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let (bounds, supports) = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return None,
        Err(error) => return Some(error),
    };
    Some(commit_supported_timber(
        registries, state, stack, bounds, supports,
    ))
}

fn validate_supported_timber(
    registries: &Registries,
    state: &AppState,
    stack: &crate::inventory::MaterialStackProjection,
    bounds: crate::spatial::VoxelBounds,
    supports: Vec<crate::structural::StructuralElementId>,
) -> Result<
    crate::structural::ValidatedPlayerSupportedStructuralConstructionStart,
    SecondaryActionOutcome,
> {
    let (length, area) = match stack.commodity() {
        commodity if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG) => {
            (BASIC_TIMBER_POST_LENGTH, BASIC_TIMBER_POST_CROSS_SECTION)
        }
        commodity if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) => (
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        ),
        _ => return Err(SecondaryActionOutcome::MaterialNotContextual),
    };
    let geometry = StructuralElementGeometry::new(bounds, length, area)
        .unwrap_or_else(|error| panic!("built-in timber geometry is invalid: {error}"));
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        supports,
    );
    validate_start_player_multi_supported_structural_construction_from_stack(
        registries,
        state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        plan,
        stack,
    )
    .map_err(SecondaryActionOutcome::StructuralPlacementRejected)
}

fn commit_supported_timber(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    bounds: crate::spatial::VoxelBounds,
    supports: Vec<crate::structural::StructuralElementId>,
) -> SecondaryActionOutcome {
    match validate_supported_timber(registries, state, stack, bounds, supports) {
        Ok(validated) => SecondaryActionOutcome::StructuralConstructionStarted(
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous timber placement became stale: {error}"
                )
            }),
        ),
        Err(error) => error,
    }
}

/// Explicit destination placement is an alternative to selecting a support's
/// struck face, not to physical reach, occupancy, support or work admission.
pub(in crate::simulation) fn validate_supported_timber_at(
    registries: &Registries,
    state: &AppState,
    stack: &crate::inventory::MaterialStackProjection,
    position: crate::spatial::VoxelCoord,
) -> Result<
    crate::structural::ValidatedPlayerSupportedStructuralConstructionStart,
    SecondaryActionOutcome,
> {
    let direction = crate::world::aim_player_at_voxel(registries, state, position)
        .map_err(SecondaryActionOutcome::StructuralTargetRejected)?;
    let reached =
        crate::world::raycast_player_air_voxel(registries, state, direction, |voxel, _| {
            (voxel == position).then_some(())
        })
        .map_err(SecondaryActionOutcome::StructuralTargetRejected)?;
    if reached.is_none() {
        return Err(SecondaryActionOutcome::NoStructuralPlacementTarget);
    }
    super::validate_direct_placement_position(registries, state, position)
        .map_err(structural_placement_error)?;
    if stack.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
        validate_full_footprint_placement_position(state, position)
            .map_err(structural_placement_error)?;
    }
    let bounds = single_voxel_bounds(position)
        .ok_or(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow)?;
    let supports = super::local_active_face_supports(state, position, bounds);
    validate_supported_timber(registries, state, stack, bounds, supports)
}

pub(in crate::simulation::direct_actions) fn attempt_selected_timber_cover_placement(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> SecondaryActionOutcome {
    let (bounds, supports) = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return SecondaryActionOutcome::NoStructuralPlacementTarget,
        Err(error) => return error,
    };
    if let Err(error) = validate_full_footprint_placement_position(state, bounds.min()) {
        return structural_placement_error(error);
    }
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return outcome,
    };
    // Physical context and carried material are observable on the input frame.
    // Construction admission still occurs on the next authoritative slow tick.
    if !state.control_tick().is_simulation_boundary() {
        return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
    }
    commit_supported_timber(registries, state, &stack, bounds, supports)
}

/// Placement needs one physical material cohort, but an invalid pointed post
/// should not hide a co-located fuel transaction that can combine several lots.
/// Structural-face support takes priority over the terrain-face rejection.
fn resolve_timber_stack_for_pointed_use(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Result<crate::inventory::MaterialStackProjection, SecondaryActionOutcome> {
    let selection = resolve_unique_carried_stack(registries, state, commodity);
    if matches!(
        &selection,
        Err(SecondaryActionOutcome::AmbiguousMaterialStacks { .. })
    ) && commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
        && matches!(
            resolve_supported_timber_target(registries, state, direction),
            Ok(None)
        )
        && let Ok((face, _)) = resolve_direct_placement_target(registries, state, direction)
        && face != CubeFace::Top
    {
        return Err(SecondaryActionOutcome::StructuralPlacementRequiresTopFace { face });
    }
    selection
}

pub(in crate::simulation::direct_actions) fn attempt_selected_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let is_roller = commodity == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    if !is_roller && commodity != CommodityKey::new(MATERIAL_WOOD, FORM_LOG) {
        return None;
    }
    let stack = match resolve_timber_stack_for_pointed_use(registries, state, commodity, direction)
    {
        Ok(stack) => stack,
        Err(outcome) => return Some(outcome),
    };
    let stack = &stack;
    if !is_roller {
        if state.control_tick().is_simulation_boundary() {
            if let Some(outcome) =
                attempt_supported_timber_placement(registries, state, stack, direction)
            {
                return Some(outcome);
            }
        } else {
            match resolve_supported_timber_target(registries, state, direction) {
                Ok(Some(_)) => {
                    return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
                }
                Ok(None) => {}
                Err(error) => return Some(error),
            }
        }
    }
    let (face, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(error) => return Some(structural_placement_error(error)),
    };
    if face != CubeFace::Top {
        return Some(SecondaryActionOutcome::StructuralPlacementRequiresTopFace { face });
    }
    let Some(bounds) = single_voxel_bounds(position) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let geometry = StructuralElementGeometry::new(
        bounds,
        if is_roller {
            BASIC_TIMBER_DRIVE_COUPLING_LENGTH
        } else {
            BASIC_TIMBER_POST_LENGTH
        },
        if is_roller {
            BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION
        } else {
            BASIC_TIMBER_POST_CROSS_SECTION
        },
    )
    .unwrap_or_else(|error| panic!("built-in timber-post geometry is invalid: {error}"));
    let plan = GroundedStructuralElementPlan::new(
        if is_roller {
            STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING
        } else {
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION
        },
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
