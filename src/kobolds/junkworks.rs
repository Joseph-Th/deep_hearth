//! Improvised den warning devices built out of actual recoverable scavenged parts.
//!
//! This is a current-use assembly of exposed loose parts, not a free machine or a second
//! material ledger. Taking either component away immediately makes the device unusable.
//! There is no radio: a den resident can warn a scout only one real traversal edge away.

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

pub(super) fn current_den_rig(state: &AppState, colony: &KoboldColony) -> Option<KoboldJunkRig> {
    if colony.mobilized_at.is_none() {
        return None;
    }
    let den = colony.route[0];
    let drops = || {
        state
            .surface()
            .available_resources_at(Some(den))
            .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
    };
    let core = drops()
        .find(|resource| {
            is_servant_era_salvage(resource) && resource.remaining_mass() >= CORE_MINIMUM
        })?
        .id();
    let wooden = drops().find(|resource| {
        resource.commodity().material() == MATERIAL_WOOD
            && [FORM_HANDLE, FORM_BOARD].contains(&resource.commodity().form())
            && resource.remaining_mass() >= STRIKER_MINIMUM
    });
    let (striker, kind) = if let Some(resource) = wooden {
        (resource.id(), KoboldJunkRigKind::TimberRatchet)
    } else {
        (
            drops()
                .find(|resource| {
                    resource.commodity().material() == MATERIAL_STONE
                        && resource.commodity().form() == FORM_CHIP
                        && resource.remaining_mass() >= STRIKER_MINIMUM
                })?
                .id(),
            KoboldJunkRigKind::StoneClatter,
        )
    };
    Some(KoboldJunkRig {
        position: den,
        core,
        striker,
        kind,
    })
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

/// A watchman's warning can propagate over one actual connected edge, but only
/// on a working beat and while the watcher is physically at the den.
///
/// The returned signal drives the scout's normal pressured retreat in this same
/// authoritative step. No energy, matter, or alarm is fabricated at a distance.
pub(super) fn relay_watch_alarm(state: &mut AppState, at: SimulationTick) -> bool {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return false;
    };
    if !colony.watch_alarm_pending
        || colony.watch_deployed_at.is_some()
        || colony.scout_step != 1
        || colony.mobilized_at.is_none_or(|reported| reported >= at)
        || !state
            .logistics()
            .has_traversal_edge(colony.route[0], colony.route[1])
        || !current_den_rig(state, colony).is_some_and(|rig| rig.sounds_at(at))
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
