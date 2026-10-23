//! Improvised den warning devices built out of actual recoverable scavenged parts.
//!
//! This is a current-use assembly of exposed loose parts, not a free machine or a second
//! material ledger. Taking either component away immediately makes the device unusable.
//! There is no radio: the second edge needs a physical junction resonator.

use crate::content::{FORM_BOARD, FORM_CHIP, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};

use super::scavenging::is_servant_era_salvage;
use super::visibility::scout_visible_from;
use super::{FIRST_SCOUT_TICK, KoboldColony, SCOUT_STEP_TICKS};

const CORE_MINIMUM: Mass = Mass::from_milligrams(10_000);
const STRIKER_MINIMUM: Mass = Mass::from_milligrams(20_000);

/// Timber shims damp the rattling mechanism; bare stone clappers jam more often.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldJunkRigKind {
    TimberRatchet,
    StoneClatter,
}

/// Player-local acoustic evidence of a real alarm strike, not an index of
/// unobserved machinery or a second persistent signal ledger.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldAlarmPulse {
    /// An audible but unseen device has no disclosed exact origin.
    position: Option<VoxelCoord>,
    kind: KoboldJunkRigKind,
}

impl KoboldAlarmPulse {
    /// Identified source only when visually observable; hearing alone never
    /// reveals hidden apparatus coordinates.
    #[must_use]
    pub const fn position(self) -> Option<VoxelCoord> {
        self.position
    }

    #[must_use]
    pub const fn kind(self) -> KoboldJunkRigKind {
        self.kind
    }
}

/// An actual den arrangement made from still-recoverable loose parcels.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldJunkRig {
    position: VoxelCoord,
    core: SurfaceResourceId,
    striker: SurfaceResourceId,
    kind: KoboldJunkRigKind,
}

impl KoboldJunkRig {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn core(self) -> SurfaceResourceId {
        self.core
    }

    #[must_use]
    pub const fn striker(self) -> SurfaceResourceId {
        self.striker
    }

    #[must_use]
    pub const fn kind(self) -> KoboldJunkRigKind {
        self.kind
    }

    /// A mechanical lash-up misses beats; its deterministic phase is tied to the
    /// existing scout clock, not randomness or wall time.
    #[must_use]
    pub fn sounds_at(self, at: SimulationTick) -> bool {
        let Some(elapsed) = at.value().checked_sub(FIRST_SCOUT_TICK) else {
            return false;
        };
        if !elapsed.is_multiple_of(SCOUT_STEP_TICKS) {
            return false;
        }
        let steps = elapsed / SCOUT_STEP_TICKS;
        let cycle = match self.kind {
            KoboldJunkRigKind::TimberRatchet => 3,
            KoboldJunkRigKind::StoneClatter => 5,
        };
        steps.is_multiple_of(cycle)
    }
}

pub(super) fn is_junk_striker(resource: &SurfaceResourceRecord) -> bool {
    let commodity = resource.commodity();
    ((commodity.material() == MATERIAL_WOOD
        && [FORM_HANDLE, FORM_BOARD].contains(&commodity.form()))
        || (commodity.material() == MATERIAL_STONE && commodity.form() == FORM_CHIP))
        && resource.remaining_mass() >= STRIKER_MINIMUM
}

pub(super) fn den_needs_striker(state: &AppState, den: VoxelCoord) -> bool {
    let mut has_core = false;
    let mut has_striker = false;
    for resource in state.surface().available_resources_at(Some(den)) {
        if resource.kind() != SurfaceResourceKind::Drop {
            continue;
        }
        has_core |= is_servant_era_salvage(resource) && resource.remaining_mass() >= CORE_MINIMUM;
        has_striker |= is_junk_striker(resource);
    }
    has_core && !has_striker
}

/// Only drops currently on this voxel count as machine parts; stable parcel
/// identity selects a core even when several remnants are piled together.
fn selected_core_at(state: &AppState, position: VoxelCoord) -> Option<SurfaceResourceId> {
    state
        .surface()
        .available_resources_at(Some(position))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && is_servant_era_salvage(resource)
                && resource.remaining_mass() >= CORE_MINIMUM
        })
        .map(|resource| resource.id())
}

/// Only an operational den alarm may reserve a junction copper core and its
/// clapper for a relay. An inert precision parcel does not disable other work.
pub(super) fn junction_has_relay_core(state: &AppState, colony: &KoboldColony) -> bool {
    current_den_rig(state, colony).is_some() && selected_core_at(state, colony.route[1]).is_some()
}

fn selected_striker_at(
    state: &AppState,
    position: VoxelCoord,
) -> Option<(SurfaceResourceId, KoboldJunkRigKind)> {
    let drops = || {
        state
            .surface()
            .available_resources_at(Some(position))
            .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
    };
    if let Some(resource) = drops().find(|resource| {
        resource.commodity().material() == MATERIAL_WOOD
            && [FORM_HANDLE, FORM_BOARD].contains(&resource.commodity().form())
            && resource.remaining_mass() >= STRIKER_MINIMUM
    }) {
        return Some((resource.id(), KoboldJunkRigKind::TimberRatchet));
    }
    drops()
        .find(|resource| {
            resource.commodity().material() == MATERIAL_STONE
                && resource.commodity().form() == FORM_CHIP
                && resource.remaining_mass() >= STRIKER_MINIMUM
        })
        .map(|resource| (resource.id(), KoboldJunkRigKind::StoneClatter))
}

fn current_rig_at(
    state: &AppState,
    colony: &KoboldColony,
    position: VoxelCoord,
) -> Option<KoboldJunkRig> {
    if colony.mobilized_at.is_none() {
        return None;
    }
    let core = selected_core_at(state, position)?;
    let (striker, kind) = selected_striker_at(state, position)?;
    Some(KoboldJunkRig {
        position,
        core,
        striker,
        kind,
    })
}

pub(super) fn current_den_rig(state: &AppState, colony: &KoboldColony) -> Option<KoboldJunkRig> {
    current_rig_at(state, colony, colony.route[0])
}

/// A second, material-backed resonator at the junction. Without the den's
/// working copper-and-striker source, the repeater cannot relay anything.
pub(super) fn current_junction_relay(
    state: &AppState,
    colony: &KoboldColony,
) -> Option<KoboldJunkRig> {
    let _ = current_den_rig(state, colony)?;
    current_rig_at(state, colony, colony.route[1])
}

/// Once the den's source device works, the scouts stage a second machine
/// at their actual junction. The missing wooden/stone clapper becomes a
/// salvage priority after a copper core is physically deposited there.
pub(super) fn den_or_relay_needs_striker(state: &AppState, den: VoxelCoord) -> bool {
    if den_needs_striker(state, den) {
        return true;
    }
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    current_den_rig(state, colony).is_some()
        && selected_core_at(state, colony.route[1]).is_some()
        && selected_striker_at(state, colony.route[1]).is_none()
}

/// A returning scavenger will not haul home the specific copper or clapper
/// already staged at its junction after the den rig becomes operational.
/// It may still take extra parcels. The player can freely recover any part,
/// and dismantling the den source releases the incomplete repeater as loot.
pub(super) fn is_staged_relay_component(
    state: &AppState,
    resource: &SurfaceResourceRecord,
) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    if resource.position() != colony.route[1] || current_den_rig(state, colony).is_none() {
        return false;
    }
    selected_core_at(state, colony.route[1]) == Some(resource.id())
        || selected_striker_at(state, colony.route[1])
            .is_some_and(|(striker, _)| striker == resource.id())
}

/// Show only a device at the player's genuinely observable den. This returns no
/// information about unvisited caches, missing components, or remote patrols.
#[must_use]
pub fn available_nearby_kobold_junk_rig(
    registries: &Registries,
    state: &AppState,
) -> Option<KoboldJunkRig> {
    let colony = state.kobold_state().colony.as_ref()?;
    let player = state.logistics().player()?.position();
    scout_visible_from(registries, state, colony.route[0], player)
        .then(|| current_den_rig(state, colony))
        .flatten()
}

/// A player can inspect the local material arrangement even when its den
/// source is disconnected. Remote signal availability is not exposed by sight.
#[must_use]
pub fn available_nearby_kobold_junk_relay(
    registries: &Registries,
    state: &AppState,
) -> Option<KoboldJunkRig> {
    let colony = state.kobold_state().colony.as_ref()?;
    let player = state.logistics().player()?.position();
    scout_visible_from(registries, state, colony.route[1], player)
        .then(|| current_rig_at(state, colony, colony.route[1]))
        .flatten()
}

/// Return only pulses from real striking apparatus near the player. The
/// den watchman must hold an outstanding report and strike an on-beat source;
/// the junction clatters only if both connected edges actually carried the
/// warning outward on that same step. A bounded local air route controls sound;
/// only direct sight identifies the exact origin.
pub(super) fn locally_audible_alarm_pulses(
    registries: &Registries,
    state: &AppState,
    at: SimulationTick,
    warning_pending: bool,
    two_edge_transmission: bool,
) -> Vec<KoboldAlarmPulse> {
    let Some(player) = state.logistics().player().map(|record| record.position()) else {
        return Vec::new();
    };
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return Vec::new();
    };
    if !warning_pending
        || colony.watch_deployed_at.is_some()
        || colony.mobilized_at.is_none_or(|reported| reported >= at)
    {
        return Vec::new();
    }
    let Some(source) = current_den_rig(state, colony).filter(|rig| rig.sounds_at(at)) else {
        return Vec::new();
    };
    let mut pulses = Vec::new();
    if super::acoustics::can_hear_junk_rig(registries, state, source, player) {
        pulses.push(KoboldAlarmPulse {
            position: scout_visible_from(registries, state, source.position(), player)
                .then_some(source.position()),
            kind: source.kind(),
        });
    }
    if two_edge_transmission
        && let Some(relay) = current_junction_relay(state, colony)
        && super::acoustics::can_hear_junk_rig(registries, state, relay, player)
    {
        pulses.push(KoboldAlarmPulse {
            position: scout_visible_from(registries, state, relay.position(), player)
                .then_some(relay.position()),
            kind: relay.kind(),
        });
    }
    pulses
}

/// A watchman's warning crosses one true den-to-junction edge. For a scout
/// at the outer tip, a second connected edge and synchronized junction
/// resonator are necessary; neither alone acts as a radio.
///
/// The returned signal drives the scout's normal pressured retreat in this same
/// authoritative step. No energy, matter, or alarm is fabricated at a distance.
pub(super) fn relay_watch_alarm(state: &mut AppState, at: SimulationTick) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    let [den, junction, tip] = colony.route;
    if !colony.watch_alarm_pending
        || colony.watch_deployed_at.is_some()
        || !(1..=2).contains(&colony.scout_step)
        || colony.mobilized_at.is_none_or(|reported| reported >= at)
        || !state.logistics().has_traversal_edge(den, junction)
        || !current_den_rig(state, colony).is_some_and(|rig| rig.sounds_at(at))
        || (colony.scout_step == 2
            && (!state.logistics().has_traversal_edge(junction, tip)
                || !current_junction_relay(state, colony).is_some_and(|rig| rig.sounds_at(at))))
    {
        return false;
    }
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .unwrap_or_else(|| unreachable!("same den retained across alarm decision"))
        .watch_alarm_pending = false;
    true
}

#[cfg(test)]
#[path = "junkworks_tests.rs"]
mod tests;
