//! Player-facing observation of real local kobolds through world visibility.
//!
//! This read model never grants resident state, knowledge or movement authority.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceLifecycle;

use super::{
    KoboldState, STAGGERED_BELOW_PPM, defense, living_player_observer, temple_archivist,
    temple_guard,
};

/// Identity of a locally seen resident, including the fixed den watchman.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KoboldRole {
    Scout,
    Watchman,
    /// A large, armored temple-bred resident; no combat system is claimed.
    TempleWarden,
    /// A longer-lived heavy caste attending the surviving mnemonic machinery.
    TempleArchivist,
}

/// Distinct physical presence, independent of an unimplemented damage/combat model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KoboldStature {
    Small,
    Heavy,
    Massive,
}

/// Player-local observation of a physical kobold; remote dens and patrols are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldSighting {
    role: KoboldRole,
    position: VoxelCoord,
    has_noticed_player_drop: bool,
    is_wary: bool,
    is_defending: bool,
    balance_ppm: Option<u32>,
    mobilized: bool,
    scavenged_material: Option<(CommodityKey, Mass)>,
}

impl KoboldSighting {
    #[must_use]
    pub const fn role(self) -> KoboldRole {
        self.role
    }

    #[must_use]
    pub const fn balance_ppm(self) -> Option<u32> {
        self.balance_ppm
    }

    #[must_use]
    pub const fn is_staggered(self) -> bool {
        match self.balance_ppm {
            Some(balance) => balance < STAGGERED_BELOW_PPM,
            None => false,
        }
    }

    #[must_use]
    pub const fn stature(self) -> KoboldStature {
        match self.role {
            KoboldRole::Scout | KoboldRole::Watchman => KoboldStature::Small,
            KoboldRole::TempleWarden => KoboldStature::Heavy,
            KoboldRole::TempleArchivist => KoboldStature::Massive,
        }
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn has_noticed_player_drop(self) -> bool {
        self.has_noticed_player_drop
    }

    /// Scouts avoid recent danger; wardens guard visibly breached machinery
    /// or heed a directly observed archivist's damaged-archive vigil.
    #[must_use]
    pub const fn is_wary(self) -> bool {
        self.is_wary
    }

    /// Visible guarding includes a fitted scout at a threatened industrial
    /// junction, a temple warden's challenge or an archivist's archive vigil.
    /// It conveys physical readiness, not a remote combat buff.
    #[must_use]
    pub const fn is_defending(self) -> bool {
        self.is_defending
    }

    /// An actual local sighting of engineered machinery mobilized this colony.
    #[must_use]
    pub const fn is_mobilized(self) -> bool {
        self.mobilized
    }

    /// The identity and mass of a currently carried loose parcel, if still present.
    #[must_use]
    pub const fn scavenged_material(self) -> Option<(CommodityKey, Mass)> {
        self.scavenged_material
    }
}

impl KoboldState {
    fn sightings_at<'a>(
        &'a self,
        registries: &'a Registries,
        state: &'a AppState,
        position: VoxelCoord,
    ) -> impl Iterator<Item = KoboldSighting> + 'a {
        self.colony
            .iter()
            .filter(move |colony| {
                super::visibility::player_sees_opening_resident_at(
                    registries,
                    state,
                    position,
                    colony.scout_position(),
                    state.tick(),
                )
            })
            .map(move |colony| KoboldSighting {
                role: KoboldRole::Scout,
                position: colony.scout_position(),
                has_noticed_player_drop: colony.noticed_player_drop,
                is_wary: colony.wary_until.is_some_and(|until| until > state.tick())
                    || colony.recently_shoved(KoboldRole::Scout, state.control_tick()),
                is_defending: defense::scout_defends_visible_worksite(registries, state, colony),
                balance_ppm: Some(colony.scout_balance_ppm),
                mobilized: colony.mobilized_at.is_some(),
                scavenged_material: colony
                    .scavenged_drop
                    .and_then(|id| state.surface().get(id))
                    .filter(|drop| {
                        drop.lifecycle() == SurfaceResourceLifecycle::Available
                            && crate::world::player_can_see_from_eye(
                                registries,
                                state,
                                position,
                                drop.position(),
                            )
                    })
                    .map(|drop| (drop.commodity(), drop.remaining_mass())),
            })
            .chain(self.colony.iter().filter_map(move |colony| {
                let guard = colony.watch_position();
                super::visibility::player_sees_opening_resident_at(
                    registries,
                    state,
                    position,
                    guard,
                    state.tick(),
                )
                .then_some(KoboldSighting {
                    role: KoboldRole::Watchman,
                    position: guard,
                    has_noticed_player_drop: false,
                    is_wary: colony.watch_alarm_pending
                        || colony.recently_shoved(KoboldRole::Watchman, state.control_tick()),
                    is_defending: false,
                    balance_ppm: Some(colony.watch_balance_ppm),
                    mobilized: colony.mobilized_at.is_some(),
                    scavenged_material: None,
                })
            }))
            .chain(
                crate::world::nearby_temple_residents(registries, state, position)
                    .flat_map(move |[post, archivist]| {
                        let step =
                            self.temple_warden_step(crate::world::ChunkCoord::containing(post));
                        let route = crate::world::temple_warden_route_at(registries, state, post)
                            .expect("generated temple residents retain a real warden route");
                        let warden = temple_guard::warden_position(route, step)
                            .expect("trusted temple cursor names a real cell");
                        let archivist_step =
                            self.temple_archivist_step(crate::world::ChunkCoord::containing(post));
                        let archivist_actual =
                            temple_archivist::archivist_position(archivist, archivist_step)
                                .expect("trusted archive cursor names a physical cell");
                        [
                            (
                                warden,
                                KoboldRole::TempleWarden,
                                step != 0
                                    && temple_guard::warden_is_challenging(
                                        registries, state, post, warden, position,
                                    ),
                            ),
                            (
                                archivist_actual,
                                KoboldRole::TempleArchivist,
                                temple_archivist::archivist_is_guarding_breach(
                                    registries,
                                    state,
                                    archivist,
                                    archivist_step,
                                    position,
                                ),
                            ),
                        ]
                    })
                    .filter(move |(resident, role, _)| match role {
                        KoboldRole::TempleArchivist => {
                            temple_archivist::archivist_visible_to_player(
                                registries, state, *resident, position,
                            )
                        }
                        KoboldRole::TempleWarden => temple_guard::warden_visible_to_player(
                            registries, state, *resident, position,
                        ),
                        KoboldRole::Scout | KoboldRole::Watchman => false,
                    })
                    .map(|(position, role, is_defending)| KoboldSighting {
                        role,
                        position,
                        has_noticed_player_drop: false,
                        is_wary: role == KoboldRole::TempleWarden
                            && temple_guard::warden_has_damage_warning(registries, state, position),
                        is_defending,
                        balance_ppm: None,
                        mobilized: false,
                        scavenged_material: None,
                    }),
            )
    }
}

/// Local-only observation, suitable for an adapter's entity/sighting presentation.
pub fn available_nearby_kobold_scouts<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = KoboldSighting> + 'a {
    let observer = living_player_observer(state);
    observer.into_iter().flat_map(move |position| {
        state
            .kobold_state()
            .sightings_at(registries, state, position)
    })
}
