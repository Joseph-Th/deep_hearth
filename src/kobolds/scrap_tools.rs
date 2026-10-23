//! Recoverable improvised lever staged at a scavenger's work junction.
//!
//! A wooden handle and a stone chip or solid copper ingot remain ordinary
//! recoverable surface parcels. Each substitute brings its own real wear cost.

use crate::content::{
    FORM_CHIP, FORM_HANDLE, FORM_INGOT, FORM_SCRAP, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::FormId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};

use super::visibility::scout_visible_from;
use super::{KoboldColony, SCOUT_CARRY_LIMIT};

const MINIMUM_PART_MASS: Mass = Mass::from_milligrams(20_000);
pub(super) const LEVER_SPLIT_BATCH: Mass = Mass::from_milligrams(500_000);
/// The stone wedge loses recoverable chips each time it helps divide a bulk pile.
pub(super) const WEDGE_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(10_000);
const COPPER_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(5_000);

/// Alternative physical pry heads with distinct material loss per successful operation.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldPryHeadKind {
    StoneWedge,
    CopperFulcrum,
}

impl KoboldPryHeadKind {
    #[must_use]
    pub const fn abrasion_per_split(self) -> Mass {
        match self {
            Self::StoneWedge => WEDGE_ABRASION_PER_SPLIT,
            Self::CopperFulcrum => COPPER_ABRASION_PER_SPLIT,
        }
    }

    #[must_use]
    pub const fn abrasion_form(self) -> FormId {
        match self {
            Self::StoneWedge => FORM_CHIP,
            Self::CopperFulcrum => FORM_SCRAP,
        }
    }
}

/// A physical worksite arrangement rather than a virtual inventory item.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldScrapLever {
    position: VoxelCoord,
    handle: SurfaceResourceId,
    head: SurfaceResourceId,
    kind: KoboldPryHeadKind,
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
    pub const fn head(self) -> SurfaceResourceId {
        self.head
    }
    #[must_use]
    pub const fn kind(self) -> KoboldPryHeadKind {
        self.kind
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

fn is_copper_fulcrum(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity().material() == MATERIAL_COPPER
        && resource.commodity().form() == FORM_INGOT
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

fn selected_stone_fallback(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_wedge(resource))
        .map(|resource| resource.id())
}

fn selected_head(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<(SurfaceResourceId, KoboldPryHeadKind)> {
    let at_worksite = || {
        state
            .surface()
            .available_resources_at(Some(colony.route[1]))
    };
    at_worksite()
        .find(|resource| is_copper_fulcrum(resource))
        .map(|resource| (resource.id(), KoboldPryHeadKind::CopperFulcrum))
        .or_else(|| {
            selected_stone_fallback(state, colony).map(|id| (id, KoboldPryHeadKind::StoneWedge))
        })
}

/// A partial or stone-headed worksite can justify seeking a visible copper upgrade.
/// Without a staged handle, the scout never leaves ordinary nearby salvage
/// merely because a component for a hypothetical tool exists farther away.
pub(super) fn junction_awaits_better_head(state: &AppState, colony: &KoboldColony) -> bool {
    selected_handle(state, colony).is_some()
        && !matches!(
            selected_head(state, colony),
            Some((_, KoboldPryHeadKind::CopperFulcrum))
        )
}

pub(super) fn current_junction_scrap_lever(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<KoboldScrapLever> {
    let (head, kind) = selected_head(state, colony)?;
    Some(KoboldScrapLever {
        position: colony.route[1],
        handle: selected_handle(state, colony)?,
        head,
        kind,
    })
}

/// The scout protects one selected handle, the active head, and at most one
/// stone fallback. Surplus pieces remain ordinary contested salvage; none are
/// reserved from player pickup.
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
    let selected = selected_head(state, colony);
    resource.id() == handle
        || selected.is_some_and(|(head, _)| head == resource.id())
        || (selected.is_some_and(|(_, kind)| kind == KoboldPryHeadKind::CopperFulcrum)
            && selected_stone_fallback(state, colony) == Some(resource.id()))
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
        (selected_head(state, colony).is_none() && is_wedge(resource))
            || (selected_head(state, colony)
                .is_none_or(|(_, kind)| kind != KoboldPryHeadKind::CopperFulcrum)
                && is_copper_fulcrum(resource))
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
