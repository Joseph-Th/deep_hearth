//! Physical footing footprint of exposed, loose, player-created process residue.

use crate::core::state::AppState;
use crate::material::MaterialPhase;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::SurfaceResourceKind;

/// Minimum host-solid-equivalent layer that cannot be treated as bare, firm ground when placing
/// a rigid storage enclosure. Loose material has additional void space, so this estimate does
/// not require a packed-particle or weather model to establish an obstruction.
pub(crate) const OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM: u128 = 25_000;

/// The one-square-metre voxel footprint's accumulated host-solid-equivalent residue depth.
///
/// Only actual loose *drops* count: generated surface gathering opportunities are not a
/// manufactured spoil heap. Contained residue in a stockpile is not surface exposure.
/// Each parcel retains its own material density; accumulation happens before micrometre rounding
/// so splitting a drop into several lots cannot arbitrarily remove its footprint.
pub(crate) fn exposed_residue_compact_depth_um(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> u128 {
    let depth_nanometres = state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .filter(|resource| {
            registries
                .materials()
                .is_residual_commodity(resource.commodity())
        })
        .filter(|resource| {
            registries
                .materials()
                .get_form(resource.commodity().form())
                .is_some_and(|form| form.phase() == MaterialPhase::Solid)
        })
        .map(|resource| {
            let density = registries
                .materials()
                .get_material(resource.commodity().material())
                .unwrap_or_else(|| unreachable!("validated surface parcel has authored material"))
                .properties()
                .density_kg_per_m3();
            // mg / (kg/m³) is µm on one m²; retain three extra decimal places while summing.
            u128::from(resource.remaining_mass().milligrams()) * 1_000 / u128::from(density)
        })
        .sum::<u128>();
    depth_nanometres / 1_000
}
