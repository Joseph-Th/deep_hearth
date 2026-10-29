//! An informed scout can defend its real fitted worksite tool at arm's reach.
//! The head stays at the junction: this is not yet a portable armed patrol.

use crate::core::quantity::Temperature;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::surface::{SurfaceResourceId, abrade_scavenged_tool_head};
use crate::survival::{Vitality, apply_kobold_worksite_contact_injury};

use super::scrap_tools::{KoboldPryHeadKind, KoboldScrapLever, current_junction_scrap_lever};
use super::{KoboldColony, KoboldRole, SCOUT_DETERRENT_LIGHT_LEVEL};

const CONTACT_SAFE_TEMPERATURE: Temperature = Temperature::from_millikelvin(333_150);

/// One observable, ready-to-grip local weapon. The read-only sighting and
/// actual contact share this gate, so no warning appears for an unfitted,
/// stolen, hot, exhausted, frightened or unreported tool.
pub(super) fn ready_worksite_tool(
    state: &AppState,
    colony: &KoboldColony,
    at: SimulationTick,
) -> Option<KoboldScrapLever> {
    if colony.mobilized_at.is_none()
        || colony.scout_step != 1
        || colony.returning
        || colony.scavenged_drop.is_some()
        || colony.is_staggered(KoboldRole::Scout)
        || colony.recently_shoved(KoboldRole::Scout, state.control_tick())
        || colony.wary_until.is_some_and(|due| due > at)
    {
        return None;
    }
    let tool = current_junction_scrap_lever(state, colony)?;
    if tool.kind() == KoboldPryHeadKind::StoneWedge || !colony.pry_fitting?.completed_for(tool) {
        return None;
    }
    let head = state.surface().get(tool.head())?;
    (head.temperature() <= CONTACT_SAFE_TEMPERATURE
        && head.remaining_mass() > tool.kind().abrasion_per_split())
    .then_some(tool)
}

pub(super) fn scout_defends_visible_worksite(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
) -> bool {
    ready_worksite_tool(state, colony, state.tick()).is_some()
        && assess_local_illumination(registries, state, colony.scout_position()).level()
            < SCOUT_DETERRENT_LIGHT_LEVEL
}

/// Visible, physically bounded blow from a fitted tool. The head loses real mass
/// even when the player's body takes no matter-bearing injury output.
pub(super) fn attempt_worksite_counterstroke(
    registries: &Registries,
    state: &mut AppState,
    next_tick: SimulationTick,
) -> Option<(SurfaceResourceId, u32)> {
    let colony = state.kobold_state().colony.as_ref()?;
    let player = state.logistics().player()?.position();
    let resident = colony.scout_position();
    if !resident.is_face_adjacent_to(player)
        || !state.logistics().has_traversal_edge(resident, player)
        || !super::visibility::scout_visible_from(registries, state, resident, player)
        || assess_local_illumination(registries, state, resident).level()
            >= SCOUT_DETERRENT_LIGHT_LEVEL
    {
        return None;
    }
    let tool = ready_worksite_tool(state, colony, next_tick)?;
    let injury_ppm = match tool.kind() {
        KoboldPryHeadKind::StoneWedge => return None,
        KoboldPryHeadKind::CopperFulcrum => 8_000,
        KoboldPryHeadKind::IronScrapTooth => 10_000,
        KoboldPryHeadKind::IronShoe => 12_000,
    };
    if state.survival().player()?.vitality() == Vitality::ZERO
        || state.survival().player()?.vitality().parts_per_million() <= 1
        || !state.survival().can_advance_revision_by(1)
    {
        return None;
    }
    if !abrade_scavenged_tool_head(
        registries,
        state,
        tool.head(),
        resident,
        next_tick,
        tool.kind().abrasion_per_split(),
        tool.kind().abrasion_form(),
    ) {
        return None;
    }
    let suffered = apply_kobold_worksite_contact_injury(state, injury_ppm)
        .expect("prechecked living player and survival revision accept finite contact");
    Some((tool.head(), suffered))
}

#[cfg(test)]
#[path = "defense_tests.rs"]
mod tests;
