//! Recoverable improvised lever staged at a scavenger's work junction.
//!
//! A wooden handle with a stone, copper, or iron head remains ordinary
//! recoverable surface parcels. Each substitute brings its own real wear cost.

use crate::content::{
    FORM_CHIP, FORM_HANDLE, FORM_INGOT, FORM_SCRAP, MATERIAL_COPPER, MATERIAL_IRON, MATERIAL_STONE,
    MATERIAL_WOOD,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::material::FormId;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};

use super::visibility::scout_visible_from;
use super::{FIRST_SCOUT_TICK, KoboldColony, SCOUT_CARRY_LIMIT, SCOUT_STEP_TICKS};

const MINIMUM_PART_MASS: Mass = Mass::from_milligrams(20_000);
pub(super) const LEVER_SPLIT_BATCH: Mass = Mass::from_milligrams(500_000);
/// The stone wedge loses recoverable chips each time it helps divide a bulk pile.
pub(super) const WEDGE_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(10_000);
const COPPER_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(5_000);
const IRON_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(2_000);
/// A ragged tooth needs more material to clamp to the handle than a solid shoe.
const MINIMUM_IRON_SCRAP_TOOTH: Mass = Mass::from_milligrams(40_000);
const IRON_SCRAP_ABRASION_PER_SPLIT: Mass = Mass::from_milligrams(4_000);

/// Persisted proof of physical fitting work for a particular pair of parcels.
/// A replacement ingot or handle never inherits the predecessor's work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KoboldPryFitting {
    handle: SurfaceResourceId,
    head: SurfaceResourceId,
    started_at: SimulationTick,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    completed_at: Option<SimulationTick>,
}

impl KoboldPryFitting {
    fn matches(self, handle: SurfaceResourceId, head: SurfaceResourceId) -> bool {
        self.handle == handle && self.head == head
    }
}

/// Alternative physical pry heads with distinct material loss per successful operation.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldPryHeadKind {
    StoneWedge,
    CopperFulcrum,
    IronShoe,
    IronScrapTooth,
}

impl KoboldPryHeadKind {
    #[must_use]
    pub const fn abrasion_per_split(self) -> Mass {
        match self {
            Self::StoneWedge => WEDGE_ABRASION_PER_SPLIT,
            Self::CopperFulcrum => COPPER_ABRASION_PER_SPLIT,
            Self::IronShoe => IRON_ABRASION_PER_SPLIT,
            Self::IronScrapTooth => IRON_SCRAP_ABRASION_PER_SPLIT,
        }
    }

    #[must_use]
    pub const fn abrasion_form(self) -> FormId {
        match self {
            Self::StoneWedge => FORM_CHIP,
            Self::CopperFulcrum | Self::IronShoe | Self::IronScrapTooth => FORM_SCRAP,
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

fn is_iron_shoe(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity().material() == MATERIAL_IRON
        && resource.commodity().form() == FORM_INGOT
        && resource.remaining_mass() >= MINIMUM_PART_MASS
        && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

/// This is a *head-sized* irregular fragment, not every tiny loose clapper.
/// It remains exposed, finite iron scrap even after an actual fitting.
fn is_iron_scrap_tooth(resource: &SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.commodity().material() == MATERIAL_IRON
        && resource.commodity().form() == FORM_SCRAP
        && resource.remaining_mass() >= MINIMUM_IRON_SCRAP_TOOTH
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

fn selected_copper(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_copper_fulcrum(resource))
        .map(|resource| resource.id())
}

fn selected_iron(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_iron_shoe(resource))
        .map(|resource| resource.id())
}

fn selected_iron_scrap(state: &AppState, colony: &KoboldColony) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(colony.route[1]))
        .find(|resource| is_iron_scrap_tooth(resource))
        .map(|resource| resource.id())
}

fn selected_metal_head(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<(SurfaceResourceId, KoboldPryHeadKind)> {
    selected_iron(state, colony)
        .map(|id| (id, KoboldPryHeadKind::IronShoe))
        .or_else(|| {
            selected_iron_scrap(state, colony).map(|id| (id, KoboldPryHeadKind::IronScrapTooth))
        })
        .or_else(|| selected_copper(state, colony).map(|id| (id, KoboldPryHeadKind::CopperFulcrum)))
}

fn selected_head(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<(SurfaceResourceId, KoboldPryHeadKind)> {
    selected_metal_head(state, colony).or_else(|| {
        selected_stone_fallback(state, colony).map(|id| (id, KoboldPryHeadKind::StoneWedge))
    })
}

/// A partial or weaker worksite can justify seeking a visible metal upgrade.
/// Without a staged handle, the scout never leaves ordinary nearby salvage
/// merely because a component for a hypothetical tool exists farther away.
pub(super) fn junction_awaits_better_head(state: &AppState, colony: &KoboldColony) -> bool {
    selected_handle(state, colony).is_some()
        && !matches!(
            selected_head(state, colony),
            Some((_, KoboldPryHeadKind::IronShoe))
        )
}

pub(super) fn current_junction_scrap_lever(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<KoboldScrapLever> {
    let handle = selected_handle(state, colony)?;
    // A newly found iron upgrade is not fitted merely by being put beside
    // an already working copper head. Until real fitting begins, the old
    // completed pair remains active and its identity is still recoverable.
    let fitted = colony
        .pry_fitting
        .filter(|work| work.completed_at.is_some())
        .and_then(|work| {
            selected_iron(state, colony)
                .filter(|head| work.matches(handle, *head))
                .map(|head| (head, KoboldPryHeadKind::IronShoe))
                .or_else(|| {
                    selected_iron_scrap(state, colony)
                        .filter(|head| work.matches(handle, *head))
                        .map(|head| (head, KoboldPryHeadKind::IronScrapTooth))
                })
                .or_else(|| {
                    selected_copper(state, colony)
                        .filter(|head| work.matches(handle, *head))
                        .map(|head| (head, KoboldPryHeadKind::CopperFulcrum))
                })
        });
    let (head, kind) = fitted.or_else(|| {
        selected_stone_fallback(state, colony).map(|id| (id, KoboldPryHeadKind::StoneWedge))
    })?;
    Some(KoboldScrapLever {
        position: colony.route[1],
        handle,
        head,
        kind,
    })
}

/// Two consecutive slow-clock work slots must be spent at the actual junction
/// to fit a solid metal head. Any interruption before completion forfeits the work.
/// Finished work is only valid for the exact original handle and head identities.
pub(super) fn apply_pry_fitting_tick(
    state: &mut AppState,
    at: SimulationTick,
    pressured: bool,
) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    let parts = selected_handle(state, colony)
        .zip(selected_metal_head(state, colony).map(|(head, _)| head));
    let can_work =
        colony.scout_step == 1 && colony.scavenged_drop.is_none() && !colony.hungry && !pressured;
    let previous = colony.pry_fitting;
    let retained = previous.filter(|work| {
        let still_assembled = work.completed_at.is_some()
            && selected_handle(state, colony) == Some(work.handle)
            && (selected_iron(state, colony) == Some(work.head)
                || selected_copper(state, colony) == Some(work.head)
                || selected_iron_scrap(state, colony) == Some(work.head));
        (!can_work && still_assembled)
            || (parts.is_some_and(|(handle, head)| work.matches(handle, head))
                && (work.completed_at.is_some() || can_work))
    });
    let next = if !can_work {
        retained
    } else if let Some((handle, head)) = parts {
        match retained {
            Some(work) if work.completed_at.is_some() => Some(work),
            Some(work) => Some(KoboldPryFitting {
                completed_at: Some(at),
                ..work
            }),
            None => Some(KoboldPryFitting {
                handle,
                head,
                started_at: at,
                completed_at: None,
            }),
        }
    } else {
        None
    };
    let occupied_work_slot = can_work && next.is_some() && next != previous;
    let colony = state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("fitting retains its resident colony");
    colony.pry_fitting = next;
    if occupied_work_slot {
        colony.next_step_at = at
            .checked_add_span(TickSpan::new(SCOUT_STEP_TICKS))
            .expect("bounded pry fitting clock cannot overflow");
    }
    occupied_work_slot
}

pub(super) fn validate_pry_fitting(
    state: &AppState,
    colony: &KoboldColony,
    now: SimulationTick,
) -> Result<(), &'static str> {
    let Some(work) = colony.pry_fitting else {
        return Ok(());
    };
    let started = work.started_at.value();
    let handle = state.surface().get(work.handle);
    let head = state.surface().get(work.head);
    // Depleted or player-carried records can be legitimate historical proof.
    // Their original physical identity and creation chronology still matter.
    if !handle.is_some_and(|record| {
        record.commodity().material() == MATERIAL_WOOD
            && record.commodity().form() == FORM_HANDLE
            && record.initial_mass() >= MINIMUM_PART_MASS
            && record.created_at() <= work.started_at
    }) || !head.is_some_and(|record| {
        let commodity = record.commodity();
        let valid_metal = (commodity.material() == MATERIAL_COPPER
            || commodity.material() == MATERIAL_IRON)
            && commodity.form() == FORM_INGOT
            && record.initial_mass() >= MINIMUM_PART_MASS;
        let valid_fragment = commodity.material() == MATERIAL_IRON
            && commodity.form() == FORM_SCRAP
            && record.initial_mass() >= MINIMUM_IRON_SCRAP_TOOTH;
        (valid_metal || valid_fragment) && record.created_at() <= work.started_at
    }) {
        return Err("kobold pry fitting refers to nonexistent or impossible parts");
    }
    if work.handle == work.head
        || started < FIRST_SCOUT_TICK
        || !(started - FIRST_SCOUT_TICK).is_multiple_of(SCOUT_STEP_TICKS)
        || work.started_at > now
    {
        return Err("kobold pry fitting has an impossible work start");
    }
    if let Some(done) = work.completed_at {
        if done.value() != started.saturating_add(SCOUT_STEP_TICKS) || done > now {
            return Err("kobold pry fitting lacks consecutive completion work");
        }
    } else if colony.scout_step != 1 || now.value() >= started.saturating_add(SCOUT_STEP_TICKS) {
        return Err("unfinished kobold pry fitting is not at its scheduled worksite");
    }
    Ok(())
}

/// Only locally observable, unfinished work by an actual resident scout.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldPryWorkSighting {
    position: VoxelCoord,
    handle: SurfaceResourceId,
    head: SurfaceResourceId,
    started_at: SimulationTick,
}

impl KoboldPryWorkSighting {
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
    pub const fn started_at(self) -> SimulationTick {
        self.started_at
    }
}

/// Does not expose a remote work order, a concealed scout or stale abandoned work.
#[must_use]
pub fn available_nearby_kobold_pry_work(
    registries: &Registries,
    state: &AppState,
) -> Option<KoboldPryWorkSighting> {
    let colony = state.kobold_state().colony.as_ref()?;
    let work = colony
        .pry_fitting
        .filter(|work| work.completed_at.is_none())?;
    if colony.scout_position() != colony.route[1]
        || selected_handle(state, colony) != Some(work.handle)
        || selected_metal_head(state, colony).map(|(head, _)| head) != Some(work.head)
    {
        return None;
    }
    let player = state.logistics().player()?.position();
    scout_visible_from(registries, state, colony.route[1], player).then_some(
        KoboldPryWorkSighting {
            position: colony.route[1],
            handle: work.handle,
            head: work.head,
            started_at: work.started_at,
        },
    )
}

/// The scout leaves its selected handle, currently fitted head, next upgrade
/// and one stone fallback at the actual worksite. All other pieces remain
/// ordinary salvage; even staged parts are freely recoverable by the player.
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
        || current_junction_scrap_lever(state, colony)
            .is_some_and(|tool| tool.head() == resource.id())
        || (selected.is_some_and(|(_, kind)| kind != KoboldPryHeadKind::StoneWedge)
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
                .is_none_or(|(_, kind)| kind == KoboldPryHeadKind::StoneWedge)
                && is_copper_fulcrum(resource))
            || (selected_head(state, colony).is_none_or(|(_, kind)| {
                kind == KoboldPryHeadKind::StoneWedge || kind == KoboldPryHeadKind::CopperFulcrum
            }) && is_iron_scrap_tooth(resource))
            || (selected_head(state, colony)
                .is_none_or(|(_, kind)| kind != KoboldPryHeadKind::IronShoe)
                && is_iron_shoe(resource))
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
