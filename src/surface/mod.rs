//! Finite loose matter in world space, actor-local observation, and direct player gathering.

mod creation;
mod drop_projection;
mod footing;
mod gathering;
mod material_drop;
mod observation;
mod pickup;
mod state;

#[cfg(test)]
pub(crate) use creation::insert_surface_resource_for_test;
pub(crate) use creation::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceSpec,
    SurfaceResourceSpecError, ValidatedSurfaceResourceCreation, validate_create_surface_resources,
    validate_create_surface_resources_at,
};
pub use drop_projection::{PlayerWorldDropProjection, available_player_world_drops};
pub(crate) use footing::{OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM, exposed_residue_compact_depth_um};
pub use gathering::{
    PlayerSurfaceGatheringOption, SurfaceGatheringCommitError, SurfaceGatheringError,
    SurfaceGatheringOutcome, SurfaceGatheringRequest, ValidatedSurfaceGatheringStart,
    player_surface_gathering_options, validate_start_player_surface_gathering,
    validate_start_player_surface_gathering_option, validate_start_surface_gathering,
};
pub(crate) use gathering::{
    SurfaceGatheringCancellationPlan, SurfaceGatheringTickError, SurfaceGatheringTickPlan,
    apply_surface_gathering_cancellation, apply_surface_gathering_tick,
    decide_surface_gathering_cancellation, decide_surface_gathering_tick,
};
pub use material_drop::{
    PlayerMaterialDropCommitError, PlayerMaterialDropError, PlayerMaterialDropOutcome,
    ValidatedPlayerMaterialDrop, validate_drop_player_material,
};
pub use observation::{AdjacentSurfaceResource, available_adjacent_surface_resources};
pub use pickup::SurfaceDropPickupOutcome;
pub(crate) use pickup::{
    apply_automatic_surface_drop_pickup, automatic_surface_pickup_requires_control_stepping,
    decide_automatic_surface_drop_pickup,
};

pub use state::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, SurfaceResourceRecord,
    SurfaceResourceValidationError,
};
pub(crate) use state::{
    SurfaceResourceState, serialize_surface_resource_state, validate_loaded_surface_resources,
};

/// Conserved relocation of an exposed player drop by an already-authorized adjacent scavenger.
///
/// The surface owner retains matter and physical history. The caller owns creature movement
/// legality; this function refuses any missing/depleted/non-drop parcel or stale location.
pub(crate) fn relocate_scavenged_drop_step(
    registries: &crate::registry::Registries,
    state: &mut crate::core::state::AppState,
    resource: SurfaceResourceId,
    from: crate::spatial::VoxelCoord,
    to: crate::spatial::VoxelCoord,
) -> bool {
    if !from.is_face_adjacent_to(to)
        || state::validation::surface_position_has_full_cube(registries.world(), state.world(), to)
        || !state.surface().get(resource).is_some_and(|record| {
            record.kind() == SurfaceResourceKind::Drop
                && record.lifecycle() == SurfaceResourceLifecycle::Available
                && record.position() == from
        })
    {
        return false;
    }
    let Some(next_revision) = state.surface().revision().checked_add(1) else {
        return false;
    };
    state
        .surface_state_mut()
        .apply_scavenged_drop_step(resource, from, to, next_revision);
    true
}

/// Conserved, bounded gathering by an opening scavenger at one physical patrol step.
///
/// The same natural source loses exactly the mass materialized as a new recoverable loose
/// parcel at the adjacent den. The parcel inherits the source's material profile, origin,
/// and ambient aging. Both surface revisions are reserved before either mutation.
pub(crate) fn forage_food_to_adjacent_den(
    registries: &crate::registry::Registries,
    state: &mut crate::core::state::AppState,
    source: SurfaceResourceId,
    from: crate::spatial::VoxelCoord,
    den: crate::spatial::VoxelCoord,
    at: crate::core::time::SimulationTick,
    maximum_mass: crate::core::quantity::Mass,
) -> Option<(SurfaceResourceId, crate::core::quantity::Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        source,
        from,
        den,
        at,
        maximum_mass,
        SurfaceResourceKind::Opportunity,
        None,
    )
}

/// A bulky *loose drop* can be split at the scout's actual adjacent return step.
/// Unlike an entire portable parcel, its original ID remains at the origin with
/// reduced mass; the separated recoverable parcel gets a new ID and equal mass.
pub(crate) fn split_bulk_scavenged_drop_step(
    registries: &crate::registry::Registries,
    state: &mut crate::core::state::AppState,
    source: SurfaceResourceId,
    from: crate::spatial::VoxelCoord,
    to: crate::spatial::VoxelCoord,
    at: crate::core::time::SimulationTick,
    maximum_mass: crate::core::quantity::Mass,
) -> Option<(SurfaceResourceId, crate::core::quantity::Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        source,
        from,
        to,
        at,
        maximum_mass,
        SurfaceResourceKind::Drop,
        None,
    )
}

/// Split a bulky drop with a local stone wedge, retaining both exact physical
/// outputs: the hauled daughter and the wedge's abrasion debris at the worksite.
/// Creation and both source decrements are prevalidated as one surface operation.
/// No bulk split succeeds if the surface cannot also represent tool wear.
pub(crate) fn split_bulk_scavenged_drop_with_wedge_wear(
    registries: &crate::registry::Registries,
    state: &mut crate::core::state::AppState,
    source: SurfaceResourceId,
    from: crate::spatial::VoxelCoord,
    to: crate::spatial::VoxelCoord,
    at: crate::core::time::SimulationTick,
    maximum_mass: crate::core::quantity::Mass,
    wedge: SurfaceResourceId,
    wear_mass: crate::core::quantity::Mass,
) -> Option<(SurfaceResourceId, crate::core::quantity::Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        source,
        from,
        to,
        at,
        maximum_mass,
        SurfaceResourceKind::Drop,
        Some((wedge, wear_mass)),
    )
}

/// One surface-owned conserved transaction: materialize the adjacent daughter lot
/// first and then reduce the source by exactly the amount successfully created.
fn materialize_scavenged_portion(
    registries: &crate::registry::Registries,
    state: &mut crate::core::state::AppState,
    source: SurfaceResourceId,
    from: crate::spatial::VoxelCoord,
    to: crate::spatial::VoxelCoord,
    at: crate::core::time::SimulationTick,
    maximum_mass: crate::core::quantity::Mass,
    source_kind: SurfaceResourceKind,
    wedge_wear: Option<(SurfaceResourceId, crate::core::quantity::Mass)>,
) -> Option<(SurfaceResourceId, crate::core::quantity::Mass)> {
    let required_revisions = if wedge_wear.is_some() { 3 } else { 2 };
    if !from.is_face_adjacent_to(to) || !state.can_spend_surface_revisions(required_revisions) {
        return None;
    }
    let origin = state.surface().get(source)?;
    if origin.kind() != source_kind
        || origin.lifecycle() != SurfaceResourceLifecycle::Available
        || origin.position() != from
        || (source_kind == SurfaceResourceKind::Opportunity
            && origin.commodity().form() != crate::content::FORM_FOOD)
    {
        return None;
    }
    let mass = origin.remaining_mass().min(maximum_mass);
    // Whole portable drops move without new identity; the separation path
    // must leave a positive remnant at the original location.
    if source_kind == SurfaceResourceKind::Drop && mass == origin.remaining_mass() {
        return None;
    }
    if mass.is_zero() {
        return None;
    }
    let parcel = creation::SurfaceResourceSpec::from_existing_material(
        SurfaceResourceKind::Drop,
        to,
        mass,
        origin.material_profile().clone(),
        origin.material_provenance(),
        origin.ambient_storage_age_parts_at(at)?,
    )
    .ok()?;
    let abrasion_parcel = if let Some((wedge, abrasion)) = wedge_wear {
        let worn = state.surface().get(wedge)?;
        if wedge == source
            || abrasion.is_zero()
            || worn.kind() != SurfaceResourceKind::Drop
            || worn.lifecycle() != SurfaceResourceLifecycle::Available
            || worn.position() != from
            || abrasion >= worn.remaining_mass()
        {
            return None;
        }
        Some(
            creation::SurfaceResourceSpec::from_existing_material(
                SurfaceResourceKind::Drop,
                from,
                abrasion,
                worn.material_profile().clone(),
                worn.material_provenance(),
                worn.ambient_storage_age_parts_at(at)?,
            )
            .ok()?,
        )
    } else {
        None
    };
    let parcels = std::iter::once(parcel).chain(abrasion_parcel);
    let creation =
        creation::validate_create_surface_resources_at(registries, state, parcels, at).ok()?;
    let drop = creation.first_resource_id();
    creation.apply(state);
    let revision = state
        .surface()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("surface materialization reserved its source revision"));
    state
        .surface_state_mut()
        .apply_gathering(source, mass, at, revision);
    if let Some((wedge, abrasion)) = wedge_wear {
        let next_revision = state
            .surface()
            .revision()
            .checked_add(1)
            .expect("wedge abrasion reserved its surface revision before creation");
        state
            .surface_state_mut()
            .apply_gathering(wedge, abrasion, at, next_revision);
    }
    Some((drop, mass))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pickup_tests.rs"]
mod pickup_tests;
