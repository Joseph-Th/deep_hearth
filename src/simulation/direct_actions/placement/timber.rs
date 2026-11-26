//! Canonical timber post, framing, and cover placement using shared physical targeting.

use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_DRIVE_COUPLING_CROSS_SECTION, BASIC_TIMBER_DRIVE_COUPLING_LENGTH,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, FORM_BOARD, FORM_HANDLE, FORM_LOG,
    MATERIAL_WOOD, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
    STRUCTURAL_PROFILE_TIMBER_DRIVE_COUPLING,
};
use crate::core::state::AppState;
use crate::logistics::BufferedSecondaryTarget;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelBounds};
use crate::structural::{
    GroundedStructuralElementPlan, MultiSupportedStructuralElementPlan, StructuralElementGeometry,
    validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_multi_supported_structural_construction_from_stack,
};

use super::super::contextual::queue_secondary_use;
use super::super::secondary::resolve_unique_carried_stack;
use super::{
    SupportedTimberTarget, resolve_direct_placement_target, resolve_supported_timber_target,
    single_voxel_bounds, structural_placement_error, validate_full_footprint_placement_position,
};
use crate::simulation::outcome::SecondaryActionOutcome;

fn queue_or_commit_supported_timber(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    direction: ViewDirection,
    target: SupportedTimberTarget,
) -> SecondaryActionOutcome {
    if !state.control_tick().is_simulation_boundary() {
        return queue_secondary_use(state, direction, target.buffered_target());
    }
    commit_supported_timber(registries, state, stack, target.bounds, target.supports)
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
    let target = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return SecondaryActionOutcome::NoStructuralPlacementTarget,
        Err(error) => return error,
    };
    if let Err(error) = validate_full_footprint_placement_position(state, target.bounds.min()) {
        return structural_placement_error(error);
    }
    let commodity = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return outcome,
    };
    queue_or_commit_supported_timber(registries, state, &stack, direction, target)
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
    if !is_roller {
        match resolve_supported_timber_target(registries, state, direction) {
            Ok(Some(target)) => {
                return Some(queue_or_commit_supported_timber(
                    registries, state, &stack, direction, target,
                ));
            }
            Ok(None) => {}
            Err(error) => return Some(error),
        }
    }
    let bounds = match resolve_grounded_timber_bounds(registries, state, direction) {
        Ok(bounds) => bounds,
        Err(error) => return Some(error),
    };
    if !state.control_tick().is_simulation_boundary() {
        return Some(queue_secondary_use(
            state,
            direction,
            BufferedSecondaryTarget::GroundedTimber {
                destination: bounds.min(),
            },
        ));
    }
    Some(commit_grounded_timber(
        registries, state, &stack, bounds, is_roller,
    ))
}

fn resolve_grounded_timber_bounds(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<VoxelBounds, SecondaryActionOutcome> {
    let (face, position) = resolve_direct_placement_target(registries, state, direction)
        .map_err(structural_placement_error)?;
    if face != CubeFace::Top {
        return Err(SecondaryActionOutcome::StructuralPlacementRequiresTopFace { face });
    }
    single_voxel_bounds(position)
        .ok_or(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow)
}

fn commit_grounded_timber(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    bounds: VoxelBounds,
    is_roller: bool,
) -> SecondaryActionOutcome {
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
    }
}

/// Revalidate the original physical destination without allowing contextual
/// fallback to another member, terrain placement, storage, or combustion.
pub(in crate::simulation::direct_actions) fn attempt_buffered_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
    expected: BufferedSecondaryTarget,
) -> SecondaryActionOutcome {
    let is_board = commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let is_log = commodity == CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let is_roller = commodity == CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE);
    match expected {
        BufferedSecondaryTarget::SupportedTimber { .. } if is_board || is_log => {
            let Ok(Some(target)) = resolve_supported_timber_target(registries, state, direction)
            else {
                return SecondaryActionOutcome::BufferedContextChanged;
            };
            if target.buffered_target() != expected {
                return SecondaryActionOutcome::BufferedContextChanged;
            }
            if is_board
                && let Err(error) =
                    validate_full_footprint_placement_position(state, target.bounds.min())
            {
                return structural_placement_error(error);
            }
            let stack = match resolve_unique_carried_stack(registries, state, commodity) {
                Ok(stack) => stack,
                Err(outcome) => return outcome,
            };
            commit_supported_timber(registries, state, &stack, target.bounds, target.supports)
        }
        BufferedSecondaryTarget::GroundedTimber { destination } if is_log || is_roller => {
            if is_log
                && !matches!(
                    resolve_supported_timber_target(registries, state, direction),
                    Ok(None)
                )
            {
                return SecondaryActionOutcome::BufferedContextChanged;
            }
            let Ok(bounds) = resolve_grounded_timber_bounds(registries, state, direction) else {
                return SecondaryActionOutcome::BufferedContextChanged;
            };
            if bounds.min() != destination {
                return SecondaryActionOutcome::BufferedContextChanged;
            }
            let stack = match resolve_unique_carried_stack(registries, state, commodity) {
                Ok(stack) => stack,
                Err(outcome) => return outcome,
            };
            commit_grounded_timber(registries, state, &stack, bounds, is_roller)
        }
        _ => SecondaryActionOutcome::BufferedContextChanged,
    }
}
