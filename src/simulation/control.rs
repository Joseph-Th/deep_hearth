//! Transient direct-control intents above locomotion ownership.

use crate::logistics::{PlayerControlInput, PlayerQuickAccessSlot};
use crate::spatial::ViewDirection;

/// Held primary-control intent sampled alongside one 20 Hz locomotion input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryControlIntent {
    BlockExcavation { direction: ViewDirection },
}

/// Held contextual-use intent sampled alongside one 20 Hz locomotion input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryControlIntent {
    UseActive { direction: ViewDirection },
}

impl SecondaryControlIntent {
    #[must_use]
    pub const fn use_active(direction: ViewDirection) -> Self {
        Self::UseActive { direction }
    }
}

impl PrimaryControlIntent {
    #[must_use]
    pub const fn block_excavation(direction: ViewDirection) -> Self {
        Self::BlockExcavation { direction }
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
