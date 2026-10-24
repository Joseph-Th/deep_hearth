//! Transient direct-control intents above locomotion ownership.

use crate::inventory::StorageDefinitionId;
use crate::logistics::{PlayerControlInput, PlayerQuickAccessSlot};
use crate::spatial::ViewDirection;

/// Held primary-control intent sampled alongside one 20 Hz locomotion input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryControlIntent {
    BreakTarget {
        direction: ViewDirection,
    },
    /// Unarmed close-contact action, distinct from terrain excavation.
    Shove {
        direction: ViewDirection,
    },
    /// Strike using an authored, carried, selected melee weapon.
    Strike {
        direction: ViewDirection,
    },
    /// Lock the current weapon and facing for a ten-tick prepared blow.
    PrepareStrike {
        direction: ViewDirection,
    },
    /// Resolve live contact after the telegraphed preparation interval.
    ReleaseStrike {
        direction: ViewDirection,
    },
    /// Lower a prepared weapon without making contact.
    CancelStrike,
}

/// Held contextual-use intent sampled alongside one 20 Hz locomotion input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryControlIntent {
    UseActive {
        direction: ViewDirection,
    },
    /// Explicitly selected multi-material storage plan; material-stack selection alone cannot
    /// distinguish enclosures sharing the same timber or clay input.
    PlaceStorage {
        definition: StorageDefinitionId,
        direction: ViewDirection,
    },
}

impl SecondaryControlIntent {
    #[must_use]
    pub const fn use_active(direction: ViewDirection) -> Self {
        Self::UseActive { direction }
    }

    #[must_use]
    pub const fn place_storage(definition: StorageDefinitionId, direction: ViewDirection) -> Self {
        Self::PlaceStorage {
            definition,
            direction,
        }
    }
}

impl PrimaryControlIntent {
    #[must_use]
    pub const fn strike(direction: ViewDirection) -> Self {
        Self::Strike { direction }
    }

    #[must_use]
    pub const fn prepare_strike(direction: ViewDirection) -> Self {
        Self::PrepareStrike { direction }
    }

    #[must_use]
    pub const fn release_strike(direction: ViewDirection) -> Self {
        Self::ReleaseStrike { direction }
    }

    #[must_use]
    pub const fn cancel_strike() -> Self {
        Self::CancelStrike
    }

    #[must_use]
    pub const fn shove(direction: ViewDirection) -> Self {
        Self::Shove { direction }
    }

    #[must_use]
    pub const fn break_target(direction: ViewDirection) -> Self {
        Self::BreakTarget { direction }
    }
}

/// Complete transient player-control sample for one authoritative 20 Hz control tick.
///
/// Locomotion remains owned by `logistics::PlayerControlInput`. Higher-level interaction intent is
/// carried here so the logistics owner does not acquire mining/world workflow dependencies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerDirectControlInput {
    movement: PlayerControlInput,
    quick_access_slot: Option<PlayerQuickAccessSlot>,
    primary: Option<PrimaryControlIntent>,
    secondary: Option<SecondaryControlIntent>,
    drop_active_selection: bool,
}

impl PlayerDirectControlInput {
    pub const NEUTRAL: Self = Self {
        movement: PlayerControlInput::NEUTRAL,
        quick_access_slot: None,
        primary: None,
        secondary: None,
        drop_active_selection: false,
    };

    #[must_use]
    pub const fn new(movement: PlayerControlInput) -> Self {
        Self {
            movement,
            quick_access_slot: None,
            primary: None,
            secondary: None,
            drop_active_selection: false,
        }
    }

    /// Requests switching the direct-action hand to one persisted quick-access slot this step.
    #[must_use]
    pub const fn with_quick_access_slot(mut self, slot: PlayerQuickAccessSlot) -> Self {
        self.quick_access_slot = Some(slot);
        self
    }

    #[must_use]
    pub const fn with_primary(mut self, primary: PrimaryControlIntent) -> Self {
        self.primary = Some(primary);
        self
    }

    #[must_use]
    pub const fn with_secondary(mut self, secondary: SecondaryControlIntent) -> Self {
        self.secondary = Some(secondary);
        self
    }

    /// Requests dropping the currently active equipment/material selection on this control step.
    #[must_use]
    pub const fn with_drop_active_selection(mut self) -> Self {
        self.drop_active_selection = true;
        self
    }

    #[must_use]
    pub const fn movement(self) -> PlayerControlInput {
        self.movement
    }

    #[must_use]
    pub const fn quick_access_slot(self) -> Option<PlayerQuickAccessSlot> {
        self.quick_access_slot
    }

    #[must_use]
    pub const fn primary(self) -> Option<PrimaryControlIntent> {
        self.primary
    }

    #[must_use]
    pub const fn secondary(self) -> Option<SecondaryControlIntent> {
        self.secondary
    }

    #[must_use]
    pub const fn drop_active_selection(self) -> bool {
        self.drop_active_selection
    }
}
