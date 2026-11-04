//! Observable scout patrol receipts, not authoritative resident or parcel state.

use crate::core::quantity::{Mass, Volume};
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceId;

/// Locally witnessed scout activity. An observed care action survives an unseen departure,
/// but its event contains only facts witnessed at the original location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoboldScoutEvent {
    pub(super) position: VoxelCoord,
    pub(super) discovered_player_drop: bool,
    pub(super) raided_stockpile: Option<(StockpileId, Mass)>,
    pub(super) relocated_drop: Option<SurfaceResourceId>,
    pub(super) fled_player: bool,
    pub(super) fled_light: bool,
    pub(super) turned_back_at_barrier: bool,
    pub(super) cancelled_empty_cache_raid: bool,
    pub(super) abandoned_drop: Option<SurfaceResourceId>,
    pub(super) foraged_food: Option<(SurfaceResourceId, Mass)>,
    pub(super) split_drop: Option<(SurfaceResourceId, Mass)>,
    pub(super) consumed_food: Option<(SurfaceResourceId, Mass)>,
    pub(super) drank_water: Option<(FluidStoreId, Volume)>,
    pub(super) discarded_food: Option<SurfaceResourceId>,
    pub(super) cleared_cache: Option<SurfaceResourceId>,
    pub(super) heard_junk_alarm: bool,
    pub(super) defensive_contact: Option<(SurfaceResourceId, u32)>,
}

impl KoboldScoutEvent {
    /// Whether a previous finite water or den-care operation was visible this tick.
    pub(super) const fn has_care(self) -> bool {
        self.consumed_food.is_some()
            || self.drank_water.is_some()
            || self.discarded_food.is_some()
            || self.cleared_cache.is_some()
    }

    /// A stationary patrol work slot has no implied movement, cargo or material transfer.
    /// Observable care, theft and defensive contact are added by the actual tick owner.
    pub(super) const fn quiet_at(position: VoxelCoord) -> Self {
        Self {
            position,
            discovered_player_drop: false,
            raided_stockpile: None,
            relocated_drop: None,
            fled_player: false,
            fled_light: false,
            turned_back_at_barrier: false,
            cancelled_empty_cache_raid: false,
            abandoned_drop: None,
            foraged_food: None,
            split_drop: None,
            consumed_food: None,
            drank_water: None,
            discarded_food: None,
            cleared_cache: None,
            heard_junk_alarm: false,
            defensive_contact: None,
        }
    }

    /// Location of the reported activity, not necessarily the scout's unseen new position.
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn discovered_player_drop(self) -> bool {
        self.discovered_player_drop
    }

    /// Finite matter taken from a visibly reached, unenclosed ground store.
    /// Extraction is a separate work step before the scout carries the parcel.
    #[must_use]
    pub const fn raided_stockpile(self) -> Option<(StockpileId, Mass)> {
        self.raided_stockpile
    }

    /// One real parcel physically moved with the scout this tick.
    #[must_use]
    pub const fn relocated_drop(self) -> Option<SurfaceResourceId> {
        self.relocated_drop
    }

    /// A nearby living player forced this step to retreat rather than scavenge.
    #[must_use]
    pub const fn fled_player(self) -> bool {
        self.fled_player
    }

    /// The scout retreated because a finite local light source illuminated its position.
    #[must_use]
    pub const fn fled_light(self) -> bool {
        self.fled_light
    }

    /// An inaccessible or illuminated outbound cell forced the scout to turn home.
    #[must_use]
    pub const fn turned_back_at_barrier(self) -> bool {
        self.turned_back_at_barrier
    }

    /// The planned farther cache no longer has any loose matter it can recover.
    #[must_use]
    pub const fn cancelled_empty_cache_raid(self) -> bool {
        self.cancelled_empty_cache_raid
    }

    /// Previously carried material left in its existing loose surface custody.
    /// The parcel identity is reported only when the release cell was visible.
    #[must_use]
    pub const fn abandoned_drop(self) -> Option<SurfaceResourceId> {
        self.abandoned_drop
    }

    /// Finite wild food gathered into a loose, recoverable den parcel.
    #[must_use]
    pub const fn foraged_food(self) -> Option<(SurfaceResourceId, Mass)> {
        self.foraged_food
    }

    /// A loose bulk pile lost this much mass into a new recoverable parcel.
    #[must_use]
    pub const fn split_drop(self) -> Option<(SurfaceResourceId, Mass)> {
        self.split_drop
    }

    /// A nearby scout consumed finite food at its den on this step.
    #[must_use]
    pub const fn consumed_food(self) -> Option<(SurfaceResourceId, Mass)> {
        self.consumed_food
    }

    /// Fluid actually drunk from a source visible to the local observer.
    #[must_use]
    pub const fn drank_water(self) -> Option<(FluidStoreId, Volume)> {
        self.drank_water
    }

    /// Food unfit for consumption physically discarded onto the adjacent route.
    /// Names the moved parcel only if its destination is visible. For bulky
    /// cargo, this names the moved split portion, not its source.
    #[must_use]
    pub const fn discarded_food(self) -> Option<SurfaceResourceId> {
        self.discarded_food
    }

    /// A blocked den released inert cargo for nearby food or visible salvage.
    /// Names the moved parcel only if its destination is visible. For oversized
    /// cargo, this identifies the moved split portion, not its source.
    #[must_use]
    pub const fn cleared_cache(self) -> Option<SurfaceResourceId> {
        self.cleared_cache
    }

    /// A nearby scout heard a real den clatter-rig warning and changed course.
    #[must_use]
    pub const fn heard_junk_alarm(self) -> bool {
        self.heard_junk_alarm
    }
    /// The actual fitted head used and nonlethal vitality loss in ppm.
    #[must_use]
    pub const fn defensive_contact(self) -> Option<(SurfaceResourceId, u32)> {
        self.defensive_contact
    }
}
