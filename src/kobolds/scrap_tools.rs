//! Recoverable improvised lever staged at a scavenger's work junction.
//!
//! A handle and stone wedge are still two ordinary surface parcels. Returning
//! scouts stage them on distinct trips; player pickup immediately disables the kit.

use crate::content::{FORM_CHIP, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};

use super::visibility::scout_visible_from;
use super::{KoboldColony, SCOUT_CARRY_LIMIT};

const MINIMUM_PART_MASS: Mass = Mass::from_milligrams(20_000);
pub(super) const LEVER_SPLIT_BATCH: Mass = Mass::from_milligrams(500_000);
/// The stone wedge loses recoverable chips each time it helps divide a bulk pile.
pub(super) const WEDGE_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(10_000);

/// A physical worksite arrangement rather than a virtual inventory item.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldScrapLever {
    position: VoxelCoord,
    handle: SurfaceResourceId,
    wedge: SurfaceResourceId,
}

impl KoboldScrapLever {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }
    #[must_use]
    pub const fn handle(self) -> SurfaceResourceId {
        self.handle
    }
    #[must_use]
    pub const fn wedge(self) -> SurfaceResourceId {
        self.wedge
    }
}

fn is_handle(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity().material() == MATERIAL_WOOD
        && resource.commodity().form() == FORM_HANDLE
        && resource.remaining_mass() >= MINIMUM_PART_MASS
        && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

fn is_wedge(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity().material() == MATERIAL_STONE
        && resource.commodity().form() == FORM_CHIP
        && resource.remaining_mass() >= MINIMUM_PART_MASS
        && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

fn selected_handle(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    // Active copper warning machinery needs its clapper before scrap tools.
    if super::junkworks::junction_has_relay_core(state, colony)
        || super::junkworks::den_or_relay_needs_striker(state, colony.route[0])
    {
        return None;
    }
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_handle(resource))
        .map(|resource| resource.id())
}

fn selected_wedge(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_wedge(resource))
        .map(|resource| resource.id())
}

/// A partially equipped worksite justifies a repair detour; merely wanting a
/// first handle does not outrank ordinary nearby salvage.
pub(super) fn junction_awaits_wedge(state: &AppState, colony: &KoboldColony) -> bool {
    selected_handle(state, colony).is_some() && selected_wedge(state, colony).is_none()
}

pub(super) fn current_junction_scrap_lever(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<KoboldScrapLever> {
    Some(KoboldScrapLever {
        position: colony.route[1],
        handle: selected_handle(state, colony)?,
        wedge: selected_wedge(state, colony)?,
    })
}

/// The scout protects its staged handle until a wedge is delivered, then
/// leaves both parts in place. Neither is reserved against the player.
pub(super) fn is_staged_scrap_tool_part(
    state: &AppState,
    resource: &SurfaceResourceRecord,
) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    if resource.position() != colony.route[1] {
        return false;
    }
    let Some(handle) = selected_handle(state, colony) else {
        return false;
    };
    resource.id() == handle || selected_wedge(state, colony) == Some(resource.id())
}

/// A scout can favor missing tool parts only at its real outer patrol cell.
pub(super) fn is_needed_outer_tool_part(
    state: &AppState,
    position: VoxelCoord,
    resource: &SurfaceResourceRecord,
) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    if position != colony.route[2]
        || super::junkworks::junction_has_relay_core(state, colony)
        || super::junkworks::den_or_relay_needs_striker(state, colony.route[0])
    {
        return false;
    }
    if selected_handle(state, colony).is_none() {
        is_handle(resource)
    } else {
        selected_wedge(state, colony).is_none() && is_wedge(resource)
    }
}

/// No remote disclosure: an observer must see the actual junction.
#[must_use]
pub fn available_nearby_kobold_scrap_lever(
    registries: &Registries,
    state: &AppState,
) -> Option<KoboldScrapLever> {
    let colony = state.kobold_state().colony.as_ref()?;
    let player = state.logistics().player()?.position();
    scout_visible_from(registries, state, colony.route[1], player)
        .then(|| current_junction_scrap_lever(state, colony))
        .flatten()
}

#[cfg(test)]
#[path = "scrap_tools_tests.rs"]
mod tests;
