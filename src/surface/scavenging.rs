//! Surface-owned conserved scavenger relocations, foraging, splitting, and abrasion.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::MaterialLotProfile;
use crate::material::{CommodityKey, FormId, MaterialLotSpec};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, SurfaceResourceRecord,
    creation, state,
};

/// A timed player harvest has already reserved its exact source mass and
/// physical position. All scout-owned surface transfers must honor the same
/// exclusion as automatic player pickup, even when revision headroom remains.
pub(crate) fn resource_reserved_for_player_gathering(
    state: &AppState,
    resource: SurfaceResourceId,
) -> bool {
    state
        .player_work()
        .active()
        .and_then(|work| work.surface_gathering())
        .is_some_and(|work| work.resource() == resource)
}

/// Conserved relocation of an exposed player drop by an already-authorized adjacent scavenger.
///
/// The surface owner retains matter and physical history. The caller owns creature movement
/// legality; this function refuses any missing/depleted/non-drop parcel, stale location,
/// or mutation that would consume a surface revision reserved for active player work.
pub(crate) fn relocate_scavenged_drop_step(
    registries: &Registries,
    state: &mut AppState,
    resource: SurfaceResourceId,
    from: VoxelCoord,
    to: VoxelCoord,
) -> bool {
    if !state.can_spend_surface_revisions(1)
        || resource_reserved_for_player_gathering(state, resource)
        || !from.is_face_adjacent_to(to)
        || state::validation::surface_position_has_solid_world(
            registries.world(),
            state.world(),
            to,
        )
        || state.structures().blocks_full_cell(to)
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
    registries: &Registries,
    state: &mut AppState,
    source: SurfaceResourceId,
    from: VoxelCoord,
    den: VoxelCoord,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(SurfaceResourceId, Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        ScavengedPortionRequest::new(source, from, den, at, maximum_mass),
        SurfaceResourceKind::Opportunity,
        None,
    )
}

/// A bulky *loose drop* can be split at the scout's actual adjacent return step.
/// Unlike an entire portable parcel, its original ID remains at the origin with
/// reduced mass; the separated recoverable parcel gets a new ID and equal mass.
pub(crate) fn split_bulk_scavenged_drop_step(
    registries: &Registries,
    state: &mut AppState,
    source: SurfaceResourceId,
    from: VoxelCoord,
    to: VoxelCoord,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(SurfaceResourceId, Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        ScavengedPortionRequest::new(source, from, to, at, maximum_mass),
        SurfaceResourceKind::Drop,
        None,
    )
}

/// Physical source and one-step destination for a conserved scout material transfer.
#[derive(Clone, Copy)]
pub(crate) struct ScavengedPortionRequest {
    source: SurfaceResourceId,
    from: VoxelCoord,
    to: VoxelCoord,
    at: SimulationTick,
    maximum_mass: Mass,
}

impl ScavengedPortionRequest {
    pub(crate) const fn new(
        source: SurfaceResourceId,
        from: VoxelCoord,
        to: VoxelCoord,
        at: SimulationTick,
        maximum_mass: Mass,
    ) -> Self {
        Self {
            source,
            from,
            to,
            at,
            maximum_mass,
        }
    }
}

/// Produces one fully valid residue parcel from physical head wear. Same-form
/// abrasion retains its particle-size distribution; resetting that property
/// would erase the mass-weighted sizing evidence needed by later screening.
fn scavenged_abrasion_spec(
    registries: &Registries,
    resource: &SurfaceResourceRecord,
    position: VoxelCoord,
    at: SimulationTick,
    wear: Mass,
    residue_form: FormId,
) -> Option<creation::SurfaceResourceSpec> {
    if !registries
        .materials()
        .get_form(residue_form)
        .is_some_and(|form| form.is_residual())
    {
        return None;
    }
    let profile = if resource.commodity().form() == residue_form {
        resource.material_profile().clone()
    } else {
        let spec = MaterialLotSpec::with_composition(
            CommodityKey::new(resource.commodity().material(), residue_form),
            wear,
            resource.temperature(),
            resource.composition().clone(),
        )
        .ok()?;
        MaterialLotProfile::from_spec(&spec)
    };
    creation::SurfaceResourceSpec::from_existing_material(
        SurfaceResourceKind::Drop,
        position,
        wear,
        profile,
        resource.material_provenance(),
        resource.ambient_storage_age_parts_at(at)?,
    )
    .ok()
}

/// A fitted improvised head loses actual material on contact, even when no
/// bulk stone is divided. Abrasion stays openly recoverable at its worksite.
/// Failure is atomic: no source mass is spent unless residue can be created.
pub(crate) fn abrade_scavenged_tool_head(
    registries: &Registries,
    state: &mut AppState,
    head: SurfaceResourceId,
    from: VoxelCoord,
    at: SimulationTick,
    wear: Mass,
    residue_form: FormId,
) -> bool {
    if !state.can_spend_surface_revisions(2)
        || resource_reserved_for_player_gathering(state, head)
        || wear.is_zero()
    {
        return false;
    }
    let Some(resource) = state.surface().get(head) else {
        return false;
    };
    if resource.kind() != SurfaceResourceKind::Drop
        || resource.lifecycle() != SurfaceResourceLifecycle::Available
        || resource.position() != from
        || resource.remaining_mass() <= wear
    {
        return false;
    }
    let Some(residue) = scavenged_abrasion_spec(registries, resource, from, at, wear, residue_form)
    else {
        return false;
    };
    let Ok(creation) =
        creation::validate_create_surface_resources_at(registries, state, [residue], at)
    else {
        return false;
    };
    creation.apply(state);
    let next_revision = state
        .surface()
        .revision()
        .checked_add(1)
        .expect("abrasion reserved source surface revision");
    state
        .surface_state_mut()
        .apply_gathering(head, wear, at, next_revision);
    true
}

/// Split a bulky drop with a local improvised head, retaining both exact physical
/// outputs: the hauled daughter and the head's abrasion debris at the worksite.
/// Creation and both source decrements are prevalidated as one surface operation.
/// No bulk split succeeds if the surface cannot also represent tool wear.
pub(crate) fn split_bulk_scavenged_drop_with_head_wear(
    registries: &Registries,
    state: &mut AppState,
    request: ScavengedPortionRequest,
    head: SurfaceResourceId,
    wear_mass: Mass,
    residue_form: FormId,
) -> Option<(SurfaceResourceId, Mass)> {
    materialize_scavenged_portion(
        registries,
        state,
        request,
        SurfaceResourceKind::Drop,
        Some((head, wear_mass, residue_form)),
    )
}

/// One surface-owned conserved transaction: materialize the adjacent daughter lot
/// first and then reduce the source by exactly the amount successfully created.
fn materialize_scavenged_portion(
    registries: &Registries,
    state: &mut AppState,
    request: ScavengedPortionRequest,
    source_kind: SurfaceResourceKind,
    head_wear: Option<(SurfaceResourceId, Mass, FormId)>,
) -> Option<(SurfaceResourceId, Mass)> {
    let ScavengedPortionRequest {
        source,
        from,
        to,
        at,
        maximum_mass,
    } = request;
    let required_revisions = if head_wear.is_some() { 3 } else { 2 };
    if !from.is_face_adjacent_to(to)
        || !state.can_spend_surface_revisions(required_revisions)
        || resource_reserved_for_player_gathering(state, source)
        || head_wear.is_some_and(|(head, _, _)| resource_reserved_for_player_gathering(state, head))
    {
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
    let abrasion_parcel = if let Some((head, abrasion, residue_form)) = head_wear {
        let worn = state.surface().get(head)?;
        if head == source
            || abrasion.is_zero()
            || worn.kind() != SurfaceResourceKind::Drop
            || worn.lifecycle() != SurfaceResourceLifecycle::Available
            || worn.position() != from
            || abrasion >= worn.remaining_mass()
        {
            return None;
        }
        Some(scavenged_abrasion_spec(
            registries,
            worn,
            from,
            at,
            abrasion,
            residue_form,
        )?)
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
    if let Some((head, abrasion, _)) = head_wear {
        let next_revision = state
            .surface()
            .revision()
            .checked_add(1)
            .expect("tool-head abrasion reserved its surface revision before creation");
        state
            .surface_state_mut()
            .apply_gathering(head, abrasion, at, next_revision);
    }
    Some((drop, mass))
}

#[cfg(test)]
#[path = "scavenging_tests.rs"]
mod tests;
