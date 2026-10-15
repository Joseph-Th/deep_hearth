//! Transient direct-control intents above locomotion ownership.

use crate::equipment::EquipmentId;
use crate::logistics::PlayerControlInput;
use crate::mining::MiningMethodId;
use crate::spatial::ViewDirection;

/// Held primary-control intent sampled alongside one 20 Hz locomotion input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryControlIntent {
    BlockExcavation {
        direction: ViewDirection,
        method: MiningMethodId,
        equipment: EquipmentId,
    },
}

impl PrimaryControlIntent {
    #[must_use]
    pub const fn block_excavation(
        direction: ViewDirection,
        method: MiningMethodId,
        equipment: EquipmentId,
    ) -> Self {
        Self::BlockExcavation {
            direction,
            method,
            equipment,
        }
    }
}

/// Complete transient player-control sample for one authoritative 20 Hz control tick.
///
/// Locomotion remains owned by `logistics::PlayerControlInput`. Higher-level interaction intent is
/// carried here so the logistics owner does not acquire mining/world workflow dependencies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerDirectControlInput {
    movement: PlayerControlInput,
    primary: Option<PrimaryControlIntent>,
}

impl PlayerDirectControlInput {
    pub const NEUTRAL: Self = Self {
        movement: PlayerControlInput::NEUTRAL,
        primary: None,
    };

    #[must_use]
    pub const fn new(movement: PlayerControlInput) -> Self {
        Self {
            movement,
            primary: None,
        }
    }

    #[must_use]
    pub const fn with_primary(mut self, primary: PrimaryControlIntent) -> Self {
        self.primary = Some(primary);
        self
    }

    #[must_use]
    pub const fn movement(self) -> PlayerControlInput {
        self.movement
    }

    #[must_use]
    pub const fn primary(self) -> Option<PrimaryControlIntent> {
        self.primary
    }
}
