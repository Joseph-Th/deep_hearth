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
    structural_placement_error, validate_structural_placement_position,
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
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("built-in supported timber geometry is invalid: {error}"));
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

pub(in crate::simulation::direct_actions) fn attempt_selected_timber_cover_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    if commodity != CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
        return None;
    }
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return Some(outcome),
    };
    let stack = &stack;
    let (bounds, supports) = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return Some(SecondaryActionOutcome::NoStructuralPlacementTarget),
        Err(error) => return Some(error),
    };
    if let Err(error) = validate_structural_placement_position(state, bounds.min()) {
        return Some(structural_placement_error(error));
    }
    // Physical context and carried material are observable on the input frame.
    // Construction admission still occurs on the next authoritative slow tick.
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap_or_else(|error| panic!("built-in timber cover-panel geometry is invalid: {error}"));
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
